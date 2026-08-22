const MAP_STYLESHEET_URL = new URL("./map.css", import.meta.url);
const ACTIVE_MOUNTS = new WeakMap();

const MAX_RENDER_WIDTH = 2_048;
const MAX_DEVICE_PIXEL_RATIO = 2;
const DRAG_DEVICE_PIXEL_RATIO = 1;
const PHASE_TURN = 65_536;
const DRAG_THRESHOLD_PX = 5;
const KEYBOARD_YAW_STEP = Math.round(PHASE_TURN / 36);
const KEYBOARD_PITCH_STEP = Math.round(PHASE_TURN / 72);
const MAX_PITCH_PHASE = Math.round(PHASE_TURN * 75 / 360);
const VIEW_FLAT = 0;
const VIEW_GLOBE = 1;

let instanceSequence = 0;

function isElement(value) {
  return Boolean(
    value
    && value.nodeType === 1
    && value.ownerDocument
    && typeof value.replaceChildren === "function",
  );
}

function sameOriginHttpUrl(value, baseUrl, windowObject, label) {
  const url = new URL(value, baseUrl);
  if (
    (url.protocol !== "http:" && url.protocol !== "https:")
    || url.origin !== windowObject.location.origin
    || url.username
    || url.password
  ) {
    throw new TypeError(`${label} must be an uncredentialed same-origin HTTP(S) URL`);
  }
  url.hash = "";
  return url;
}

function ensureStylesheet(documentObject, windowObject) {
  const url = sameOriginHttpUrl(
    MAP_STYLESHEET_URL,
    import.meta.url,
    windowObject,
    "The map stylesheet",
  );
  const existing = Array.from(documentObject.querySelectorAll('link[rel~="stylesheet"]'))
    .find((link) => link.href === url.href);
  if (existing) return existing;

  const link = documentObject.createElement("link");
  link.rel = "stylesheet";
  link.href = url.href;
  link.dataset.mineralsMapStylesheet = "";
  documentObject.head.append(link);
  return link;
}

function mapMarkup(documentObject, id) {
  const root = documentObject.createElement("div");
  root.className = "minerals-map";
  root.dataset.mineralsMap = "";
  root.setAttribute("role", "region");
  root.setAttribute("aria-label", "Estimated forest presence, 2020");
  root.innerHTML = `
    <div class="minerals-map__stage" data-map-stage aria-busy="true">
      <canvas
        id="${id}-canvas"
        data-forest-map
        width="960"
        height="480"
        tabindex="0"
        role="img"
        aria-roledescription="interactive world map"
        aria-label="Draggable single orthographic globe map of estimated forest presence in 2020"
        aria-describedby="${id}-instructions ${id}-detail-note ${id}-caveat"
      >Interactive map unavailable.</canvas>

      <div class="minerals-map__legend-bar">
        <span class="minerals-map__status" data-map-status data-state="loading" role="status" aria-live="polite">
          <span class="minerals-map__sr-only" data-map-status-text>Loading map…</span>
        </span>
        <ul class="minerals-map__legend" aria-label="Map legend">
          <li><span class="minerals-map__swatch minerals-map__swatch--forest" aria-hidden="true"></span><span>Forest</span></li>
          <li><span class="minerals-map__swatch minerals-map__swatch--land" aria-hidden="true"></span><span>Land</span></li>
          <li><span class="minerals-map__swatch minerals-map__swatch--water" aria-hidden="true"></span><span>Water</span></li>
        </ul>
      </div>

      <div class="minerals-map__view-options" role="radiogroup" aria-label="Map projection">
        <button type="button" role="radio" aria-checked="false" aria-controls="${id}-canvas" tabindex="-1" data-map-view="flat">Flat</button>
        <button type="button" role="radio" aria-checked="true" aria-controls="${id}-canvas" aria-label="Globe — single orthographic sphere" tabindex="0" data-map-view="globe">Globe</button>
      </div>

      <output id="${id}-detail" class="minerals-map__chip" data-map-detail aria-live="polite" hidden></output>
      <time class="minerals-map__year" datetime="2020">2020</time>

      <div class="minerals-map__fallback" data-map-fallback hidden role="note">
        <strong>Map unavailable</strong>
        <span>The legend and data credits remain available.</span>
      </div>
    </div>

    <p class="minerals-map__credits">
      <a href="https://doi.org/10.2905/JRC.354CG88" target="_blank" rel="noopener noreferrer">JRC forest cover (modified display)</a>
      <span aria-hidden="true"> · </span>
      <a href="https://www.naturalearthdata.com/" target="_blank" rel="noopener noreferrer">Natural Earth</a>
    </p>

    <p id="${id}-instructions" class="minerals-map__sr-only" data-map-instructions>
      Drag the globe left, right, up, or down to rotate it. Hover to inspect an estimate; tap or click without dragging to keep a point selected. With the map focused, use Control plus the arrow keys to rotate, 0 to reset the globe, unmodified arrow keys to move the inspector, and Escape to clear the selection. Space outside the globe has no map sample.
    </p>
    <p id="${id}-detail-note" class="minerals-map__sr-only" data-map-detail-note aria-live="polite">
      Move over the globe or focus the map with the keyboard to inspect a map cell.
    </p>
    <p id="${id}-caveat" class="minerals-map__sr-only">
      This modified display is a coarse visual overview only and is not suitable for quantitative, local, legal, conservation, or land-use decisions.
    </p>
  `;
  return root;
}

function listen(state, target, type, handler, options = undefined) {
  target.addEventListener(type, handler, options);
  const capture = typeof options === "boolean" ? options : Boolean(options?.capture);
  state.removers.push(() => target.removeEventListener(type, handler, capture));
}

function bindExternalSignal(state, signal) {
  if (!signal || typeof signal.addEventListener !== "function") return;
  if (state.externalSignals.has(signal)) return;
  if (signal.aborted) {
    state.cleanup();
    return;
  }
  const onAbort = () => state.cleanup();
  signal.addEventListener("abort", onAbort, { once: true });
  state.externalSignals.set(signal, onAbort);
}

function createMapController(container, { wasmUrl, theme }) {
  const documentObject = container.ownerDocument;
  const windowObject = documentObject.defaultView;
  if (!windowObject) throw new TypeError("The map container must belong to a browser document");

  ensureStylesheet(documentObject, windowObject);
  const resolvedWasmUrl = sameOriginHttpUrl(
    wasmUrl,
    import.meta.url,
    windowObject,
    "The map WebAssembly module",
  );
  const id = `minerals-map-${++instanceSequence}`;
  const root = mapMarkup(documentObject, id);
  container.replaceChildren(root);

  const required = (selector) => {
    const element = root.querySelector(selector);
    if (!element) throw new Error(`Map markup is missing ${selector}`);
    return element;
  };
  const canvas = required("[data-forest-map]");
  const stage = required("[data-map-stage]");
  const status = required("[data-map-status]");
  const statusText = required("[data-map-status-text]");
  const fallback = required("[data-map-fallback]");
  const detail = required("[data-map-detail]");
  const detailNote = required("[data-map-detail-note]");
  const instructions = required("[data-map-instructions]");
  const viewButtons = Array.from(root.querySelectorAll("[data-map-view]"));
  const context = typeof canvas.getContext === "function"
    ? canvas.getContext("2d", { alpha: false })
    : null;
  const darkColorScheme = windowObject.matchMedia("(prefers-color-scheme: dark)");

  const state = {
    container,
    documentObject,
    windowObject,
    root,
    canvas,
    stage,
    status,
    statusText,
    fallback,
    detail,
    detailNote,
    instructions,
    viewButtons,
    context,
    darkColorScheme,
    resolvedWasmUrl,
    preferredTheme: theme,
    wasm: null,
    activeView: "globe",
    supportsGlobePose: false,
    supportsGlobeRotation: false,
    supportsViewRendering: false,
    compatibilityFlat: false,
    rendererFailed: false,
    disposed: false,
    animationFrame: 0,
    renderPending: false,
    renderAnnouncementPending: false,
    pendingPointer: null,
    dragState: null,
    suppressNextClick: false,
    clickSuppressionTimer: 0,
    selection: null,
    pinned: false,
    globePhase: 0,
    globePitch: 0,
    lastRenderKey: "",
    cachedImage: null,
    cachedPixels: null,
    cachedMemoryBuffer: null,
    cachedPixelPointer: -1,
    cachedPixelLength: -1,
    cachedImageIsDirect: false,
    removers: [],
    observers: [],
    externalSignals: new Map(),
    fetchController: new windowObject.AbortController(),
    ready: null,
    cleanup: null,
  };

  const setStatus = (nextState, message) => {
    if (
      state.status.dataset.state === nextState
      && state.statusText.textContent.trim() === message
    ) return;
    state.status.dataset.state = nextState;
    state.statusText.textContent = message;
  };

  const setText = (element, message) => {
    if (element.textContent.trim() !== message) element.textContent = message;
  };

  const setCanvasLabel = (message) => {
    if (state.canvas.getAttribute("aria-label") !== message) {
      state.canvas.setAttribute("aria-label", message);
    }
  };

  const viewName = () => state.activeView === "globe" ? "Globe" : "Flat";

  const canRenderGlobe = () => (
    state.supportsGlobePose || state.supportsGlobeRotation || state.supportsViewRendering
  );

  const canManipulateGlobe = () => state.supportsGlobePose || state.supportsGlobeRotation;

  const globeIsDraggable = () => Boolean(
    state.wasm
    && !state.rendererFailed
    && state.activeView === "globe"
    && canManipulateGlobe(),
  );

  const baseCanvasLabel = () => {
    if (state.activeView === "flat") {
      return "Flat world map of estimated forest presence in 2020";
    }
    if (!state.wasm || state.supportsGlobePose) {
      return "Draggable single orthographic globe map of estimated forest presence in 2020";
    }
    if (state.supportsGlobeRotation) {
      return "Horizontally draggable single orthographic globe map of estimated forest presence in 2020";
    }
    return "Single orthographic globe map of estimated forest presence in 2020";
  };

  const defaultDetail = () => state.activeView === "globe"
    ? "Move over the globe or focus the map with the keyboard to inspect a map cell."
    : "Move over the map or focus it with the keyboard to inspect a map cell.";

  const defaultDetailNote = () => state.activeView === "globe"
    ? "The inspector reports forest, land, or water inside the globe. Space outside it has no sample; the coastline is a non-data overlay."
    : "The inspector reports the underlying forest, land, or water state; the coastline is a non-data overlay.";

  const globeInstructions = () => {
    if (!state.wasm || state.supportsGlobePose) {
      return "Drag the globe left, right, up, or down to rotate it. Hover to inspect an estimate; tap or click without dragging to keep a point selected. With the map focused, use Control plus the arrow keys to rotate, 0 to reset the globe, unmodified arrow keys to move the inspector, and Escape to clear the selection. Space outside the globe has no map sample.";
    }
    if (state.supportsGlobeRotation) {
      return "Drag the globe left or right to rotate it. Hover to inspect an estimate; tap or click without dragging to keep a point selected. With the map focused, use Control plus Left or Right to rotate, 0 to reset the globe, unmodified arrow keys to move the inspector, and Escape to clear the selection. Space outside the globe has no map sample.";
    }
    return "Globe view shows one static orthographic sphere. Hover to inspect an estimate; tap or click to keep a point selected. With the map focused, use the arrow keys to move the inspector and Escape to clear the selection. Space outside the globe has no map sample.";
  };

  const resolveDarkTheme = () => {
    if (state.preferredTheme === "dark") return true;
    if (state.preferredTheme === "light") return false;
    const documentTheme = state.documentObject.documentElement.getAttribute("data-theme");
    if (documentTheme === "dark") return true;
    if (documentTheme === "light") return false;
    return state.darkColorScheme.matches;
  };

  const applyResolvedTheme = () => {
    state.root.dataset.mapTheme = resolveDarkTheme() ? "dark" : "light";
  };

  const updateViewButtons = () => {
    for (const button of state.viewButtons) {
      const buttonView = button.dataset.mapView;
      const selected = buttonView === state.activeView;
      const unavailable = Boolean(state.wasm) && !canRenderGlobe() && buttonView === "globe";
      button.disabled = unavailable;
      button.setAttribute("aria-disabled", String(unavailable));
      button.setAttribute("aria-checked", String(selected));
      button.tabIndex = selected ? 0 : -1;
    }
  };

  const updateDragAffordance = () => {
    const enabled = globeIsDraggable();
    state.canvas.dataset.dragEnabled = String(enabled);
    state.canvas.dataset.dragAxes = enabled
      ? state.supportsGlobePose ? "both" : "horizontal"
      : "none";
    state.canvas.dataset.dragging = String(Boolean(enabled && state.dragState?.moved));
    if (!enabled) {
      state.canvas.removeAttribute("aria-keyshortcuts");
      return;
    }
    state.canvas.setAttribute(
      "aria-keyshortcuts",
      state.supportsGlobePose
        ? "Control+ArrowLeft Control+ArrowRight Control+ArrowUp Control+ArrowDown 0"
        : "Control+ArrowLeft Control+ArrowRight 0",
    );
  };

  const updateViewCopy = () => {
    setCanvasLabel(baseCanvasLabel());
    setText(
      state.instructions,
      state.activeView === "globe"
        ? globeInstructions()
        : "Flat view shows the full world plane. Hover to inspect an estimate; tap or click to keep a point selected. With the map focused, use the arrow keys to move the inspector and Escape to clear the selection.",
    );
    state.detail.hidden = true;
    state.detail.removeAttribute("data-category");
    setText(state.detail, "");
    setText(state.detailNote, `${defaultDetail()} ${defaultDetailNote()}`);
    updateDragAffordance();
  };

  const cancelActiveDrag = () => {
    const drag = state.dragState;
    state.dragState = null;
    state.pendingPointer = null;
    state.canvas.dataset.dragging = "false";
    if (!drag || typeof state.canvas.hasPointerCapture !== "function") return;
    try {
      if (state.canvas.hasPointerCapture(drag.pointerId)) {
        state.canvas.releasePointerCapture(drag.pointerId);
      }
    } catch {
      // Pointer capture may already have been released by the browser.
    }
  };

  const cancelScheduledFrame = () => {
    if (!state.animationFrame) return;
    state.windowObject.cancelAnimationFrame(state.animationFrame);
    state.animationFrame = 0;
  };

  const fail = () => {
    if (state.disposed) return;
    state.rendererFailed = true;
    state.renderPending = false;
    state.renderAnnouncementPending = false;
    state.pendingPointer = null;
    cancelScheduledFrame();
    cancelActiveDrag();
    updateDragAffordance();
    setStatus("error", "Map unavailable");
    state.stage.setAttribute("aria-busy", "false");
    state.fallback.hidden = false;
    state.canvas.hidden = true;
    state.detail.hidden = true;
    for (const button of state.viewButtons) {
      button.disabled = true;
      button.setAttribute("aria-disabled", "true");
    }
    setText(state.detailNote, "The forest layer could not be loaded in this browser. The legend and data credits remain available.");
  };

  const exportedNumber = (name) => {
    const value = state.wasm[name];
    const resolved = typeof value === "function" ? value() : value?.value;
    if (typeof resolved !== "number" && typeof resolved !== "bigint") {
      throw new TypeError(`Missing numeric WebAssembly export: ${name}`);
    }
    const number = Number(resolved);
    if (!Number.isSafeInteger(number) || number < 0) {
      throw new RangeError(`Invalid WebAssembly export: ${name}`);
    }
    return number;
  };

  const forestStateAt = (u, v) => {
    if (!state.wasm || typeof state.wasm.forest_at !== "function") return null;
    const x = Math.min(state.canvas.width - 1, Math.max(0, Math.floor(u * state.canvas.width)));
    const y = Math.min(state.canvas.height - 1, Math.max(0, Math.floor(v * state.canvas.height)));
    const value = Number(state.wasm.forest_at(x, y));
    if (value === 100) return "forest";
    if (value === 0) return "land";
    if (value === 254) return "outside";
    if (value === 255) return "water";
    return null;
  };

  const inspect = (u, v, interaction) => {
    if (state.disposed) return;
    const normalizedU = Math.min(1, Math.max(0, u));
    const normalizedV = Math.min(1, Math.max(0, v));
    state.selection = { u: normalizedU, v: normalizedV };

    const forestState = forestStateAt(normalizedU, normalizedV);
    const categoryText = {
      forest: "Forest",
      land: "Land",
      water: "Water",
      outside: "Outside",
    };
    const stateText = {
      forest: "Estimated forest presence at this sampled cell.",
      land: "Forest not shown at this sampled cell.",
      water: "Water / no estimate at this sampled cell.",
      outside: "Outside the globe; no map sample at this point.",
    };
    const category = forestState === null ? "unavailable" : forestState;
    state.detail.dataset.category = category;
    state.detail.dataset.pinned = String(state.pinned);
    state.detail.hidden = false;
    setText(state.detail, forestState === null ? "Unavailable" : categoryText[forestState]);
    setText(
      state.detailNote,
      forestState === null
        ? `${interaction} point: sample unavailable.`
        : `${interaction} point. ${stateText[forestState]}${state.pinned ? " Selection kept; click another point or press Escape to clear it." : ""}`,
    );
    setCanvasLabel(
      forestState === null
        ? `${baseCanvasLabel()}. The current sample is unavailable.`
        : `${baseCanvasLabel()}. ${stateText[forestState]}`,
    );
  };

  const readyMessage = () => {
    if (state.compatibilityFlat) {
      return "Map ready · Flat view";
    }
    if (state.activeView === "globe" && !canManipulateGlobe()) {
      return "Map ready · Globe view";
    }
    return `Map ready · ${viewName()} view`;
  };

  const renderMap = (announceStatus) => {
    if (!state.wasm || !state.context || state.disposed) return false;
    const bounds = state.canvas.getBoundingClientRect();
    if (bounds.width < 1) return false;

    const maximumPixelRatio = state.dragState?.moved
      ? DRAG_DEVICE_PIXEL_RATIO
      : MAX_DEVICE_PIXEL_RATIO;
    const pixelRatio = Math.min(
      Math.max(state.windowObject.devicePixelRatio || 1, 1),
      maximumPixelRatio,
    );
    const width = Math.max(2, Math.min(MAX_RENDER_WIDTH, Math.round(bounds.width * pixelRatio)));
    const height = Math.max(1, Math.round(width / 2));
    const themeValue = resolveDarkTheme() ? 1 : 0;
    const phase = Math.floor(state.globePhase) & 0xffff;
    const pitch = Math.trunc(state.globePitch);
    const renderKey = [
      state.activeView,
      width,
      height,
      themeValue,
      state.activeView === "globe" ? phase : 0,
      state.activeView === "globe" && state.supportsGlobePose ? pitch : 0,
    ].join(":");

    if (renderKey === state.lastRenderKey) {
      state.stage.setAttribute("aria-busy", "false");
      if (announceStatus) setStatus("ready", readyMessage());
      return false;
    }

    let renderResult;
    if (state.activeView === "globe") {
      if (state.supportsGlobePose) {
        renderResult = Number(state.wasm.render_globe_pose(width, height, themeValue, phase, pitch));
      } else if (state.supportsGlobeRotation) {
        renderResult = Number(state.wasm.render_globe(width, height, themeValue, phase));
      } else {
        renderResult = Number(state.wasm.render_view(width, height, themeValue, VIEW_GLOBE));
      }
    } else {
      renderResult = state.supportsViewRendering
        ? Number(state.wasm.render_view(width, height, themeValue, VIEW_FLAT))
        : Number(state.wasm.render(width, height, themeValue));
    }
    if (renderResult !== 1) {
      throw new RangeError("WebAssembly rejected the requested map dimensions");
    }

    const pixelPointer = exportedNumber("pixel_ptr");
    const pixelLength = exportedNumber("pixel_len");
    const expectedLength = width * height * 4;
    if (pixelLength !== expectedLength) {
      throw new RangeError("WebAssembly returned an unexpected pixel buffer length");
    }
    const memoryBuffer = state.wasm.memory.buffer;
    const memoryLength = memoryBuffer.byteLength;
    if (pixelPointer > memoryLength || pixelLength > memoryLength - pixelPointer) {
      throw new RangeError("WebAssembly returned an out-of-bounds pixel buffer");
    }

    if (state.canvas.width !== width || state.canvas.height !== height) {
      state.canvas.width = width;
      state.canvas.height = height;
    }

    const cacheMatches = (
      state.cachedImage
      && state.cachedMemoryBuffer === memoryBuffer
      && state.cachedPixelPointer === pixelPointer
      && state.cachedPixelLength === pixelLength
      && state.cachedImage.width === width
      && state.cachedImage.height === height
    );
    if (!cacheMatches) {
      state.cachedPixels = new Uint8ClampedArray(memoryBuffer, pixelPointer, pixelLength);
      state.cachedImageIsDirect = false;
      if (typeof state.windowObject.ImageData === "function") {
        try {
          state.cachedImage = new state.windowObject.ImageData(state.cachedPixels, width, height);
          state.cachedImageIsDirect = true;
        } catch {
          state.cachedImage = null;
        }
      }
      if (!state.cachedImage) state.cachedImage = state.context.createImageData(width, height);
      state.cachedMemoryBuffer = memoryBuffer;
      state.cachedPixelPointer = pixelPointer;
      state.cachedPixelLength = pixelLength;
    }
    if (!state.cachedImageIsDirect) state.cachedImage.data.set(state.cachedPixels);
    state.context.putImageData(state.cachedImage, 0, 0);
    state.lastRenderKey = renderKey;
    state.stage.setAttribute("aria-busy", "false");

    if (announceStatus) setStatus("ready", readyMessage());
    if (state.selection) {
      inspect(state.selection.u, state.selection.v, state.pinned ? "Selected" : "Hovered");
    }
    return true;
  };

  let runAnimationFrame;
  const hasFrameWork = () => Boolean(
    state.renderPending || state.pendingPointer || state.dragState?.pendingPoint,
  );

  const requestAnimationWork = () => {
    if (
      state.animationFrame
      || state.rendererFailed
      || state.disposed
      || state.documentObject.hidden
      || !hasFrameWork()
    ) return;
    state.animationFrame = state.windowObject.requestAnimationFrame(runAnimationFrame);
  };

  const scheduleRender = ({ announce = false } = {}) => {
    if (state.disposed || state.rendererFailed) return;
    state.renderPending = true;
    state.renderAnnouncementPending ||= announce;
    requestAnimationWork();
  };

  const normalizePhase = (value) => {
    const rounded = Math.round(value) % PHASE_TURN;
    return rounded < 0 ? rounded + PHASE_TURN : rounded;
  };

  const clampPitch = (value) => Math.max(
    -MAX_PITCH_PHASE,
    Math.min(MAX_PITCH_PHASE, Math.round(value)),
  );

  const applyPendingDrag = () => {
    if (!state.dragState?.moved || !state.dragState.pendingPoint) return;
    const point = state.dragState.pendingPoint;
    state.dragState.pendingPoint = null;
    const deltaX = point.x - state.dragState.startX;
    const deltaY = point.y - state.dragState.startY;
    const nextPhase = normalizePhase(
      state.dragState.startPhase - deltaX * PHASE_TURN / state.dragState.width,
    );
    const nextPitch = state.supportsGlobePose
      ? clampPitch(state.dragState.startPitch + deltaY * PHASE_TURN / state.dragState.width)
      : state.globePitch;
    if (nextPhase === state.globePhase && nextPitch === state.globePitch) return;
    state.globePhase = nextPhase;
    state.globePitch = nextPitch;
    state.renderPending = true;
  };

  runAnimationFrame = () => {
    state.animationFrame = 0;
    if (state.disposed) return;
    applyPendingDrag();

    if (state.renderPending) {
      const announceStatus = state.renderAnnouncementPending;
      state.renderPending = false;
      state.renderAnnouncementPending = false;
      try {
        renderMap(announceStatus);
      } catch (error) {
        console.error("Unable to render the local forest map", error);
        fail();
        return;
      }
    }
    if (state.pendingPointer && !state.dragState) {
      const point = state.pendingPointer;
      state.pendingPointer = null;
      inspect(point.u, point.v, "Hovered");
    }
    requestAnimationWork();
  };

  const pointFromEvent = (event) => {
    const bounds = state.canvas.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) return null;
    return {
      u: (event.clientX - bounds.left) / bounds.width,
      v: (event.clientY - bounds.top) / bounds.height,
    };
  };

  const clearSelection = () => {
    state.pinned = false;
    state.selection = null;
    state.pendingPointer = null;
    updateViewCopy();
  };

  const clearUnpinnedSelection = () => {
    if (!state.pinned) clearSelection();
  };

  const markDragMoved = (x, y) => {
    if (!state.dragState || state.dragState.moved) return Boolean(state.dragState?.moved);
    const deltaX = x - state.dragState.startX;
    const deltaY = y - state.dragState.startY;
    const distance = state.supportsGlobePose ? Math.hypot(deltaX, deltaY) : Math.abs(deltaX);
    if (distance < DRAG_THRESHOLD_PX) return false;
    state.dragState.moved = true;
    state.pendingPointer = null;
    clearSelection();
    state.canvas.dataset.dragging = "true";
    return true;
  };

  const finishDrag = (event, canceled) => {
    if (!state.dragState || event.pointerId !== state.dragState.pointerId) return;
    const drag = state.dragState;
    if (
      drag.moved
      && Number.isFinite(event.clientX)
      && Number.isFinite(event.clientY)
    ) {
      drag.pendingPoint = { x: event.clientX, y: event.clientY };
      applyPendingDrag();
    }
    state.dragState = null;
    state.canvas.dataset.dragging = "false";
    if (typeof state.canvas.hasPointerCapture === "function") {
      try {
        if (state.canvas.hasPointerCapture(drag.pointerId)) {
          state.canvas.releasePointerCapture(drag.pointerId);
        }
      } catch {
        // Pointer capture may already have been released by the browser.
      }
    }
    if (!drag.moved) return;

    state.pendingPointer = null;
    if (!canceled) {
      state.suppressNextClick = true;
      if (state.clickSuppressionTimer) {
        state.windowObject.clearTimeout(state.clickSuppressionTimer);
      }
      state.clickSuppressionTimer = state.windowObject.setTimeout(() => {
        state.suppressNextClick = false;
        state.clickSuppressionTimer = 0;
      }, 0);
    }
    scheduleRender();
  };

  const setActiveView = (nextView, { render = true } = {}) => {
    if (nextView !== "flat" && nextView !== "globe") return;
    if (nextView === "globe" && state.wasm && !canRenderGlobe()) return;

    const changed = state.activeView !== nextView;
    state.activeView = nextView;
    updateViewButtons();
    if (!changed) return;
    cancelActiveDrag();
    clearSelection();
    if (render && state.wasm) {
      state.stage.setAttribute("aria-busy", "true");
      setStatus("loading", `Rendering ${viewName()} view…`);
      scheduleRender({ announce: true });
    }
  };

  const defaultInspectionPoint = () => ({ u: 0.5, v: 0.5 });

  const nextKeyboardPoint = (start, direction, multiplier) => {
    const stepU = direction[0] / 48;
    const stepV = direction[1] / 24;
    const firstStep = Math.max(1, multiplier);
    for (let step = firstStep; step <= 96; step += 1) {
      const candidate = {
        u: Math.min(1, Math.max(0, start.u + stepU * step)),
        v: Math.min(1, Math.max(0, start.v + stepV * step)),
      };
      if (state.activeView !== "globe" || !canRenderGlobe()) return candidate;
      if (forestStateAt(candidate.u, candidate.v) !== "outside") return candidate;
      if (
        (direction[0] < 0 && candidate.u === 0)
        || (direction[0] > 0 && candidate.u === 1)
        || (direction[1] < 0 && candidate.v === 0)
        || (direction[1] > 0 && candidate.v === 1)
      ) break;
    }
    return start;
  };

  const rotateFromKeyboard = (direction) => {
    if (state.activeView !== "globe" || !canManipulateGlobe()) return false;
    let nextPhase = state.globePhase;
    let nextPitch = state.globePitch;
    if (direction[0] < 0) nextPhase = normalizePhase(state.globePhase + KEYBOARD_YAW_STEP);
    if (direction[0] > 0) nextPhase = normalizePhase(state.globePhase - KEYBOARD_YAW_STEP);
    if (state.supportsGlobePose && direction[1] < 0) {
      nextPitch = clampPitch(state.globePitch - KEYBOARD_PITCH_STEP);
    }
    if (state.supportsGlobePose && direction[1] > 0) {
      nextPitch = clampPitch(state.globePitch + KEYBOARD_PITCH_STEP);
    }
    if (direction[1] !== 0 && !state.supportsGlobePose) return false;
    if (nextPhase === state.globePhase && nextPitch === state.globePitch) return true;
    state.globePhase = nextPhase;
    state.globePitch = nextPitch;
    clearSelection();
    scheduleRender();
    return true;
  };

  for (const button of state.viewButtons) {
    listen(state, button, "click", () => setActiveView(button.dataset.mapView));
    listen(state, button, "keydown", (event) => {
      const enabledButtons = state.viewButtons.filter((candidate) => !candidate.disabled);
      const currentIndex = enabledButtons.indexOf(button);
      if (currentIndex < 0) return;
      let nextIndex = null;
      if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
        nextIndex = (currentIndex - 1 + enabledButtons.length) % enabledButtons.length;
      } else if (event.key === "ArrowRight" || event.key === "ArrowDown") {
        nextIndex = (currentIndex + 1) % enabledButtons.length;
      } else if (event.key === "Home") {
        nextIndex = 0;
      } else if (event.key === "End") {
        nextIndex = enabledButtons.length - 1;
      }
      if (nextIndex === null) return;
      event.preventDefault();
      const nextButton = enabledButtons[nextIndex];
      nextButton.focus();
      setActiveView(nextButton.dataset.mapView);
    });
  }

  listen(state, state.canvas, "pointerdown", (event) => {
    if (
      !globeIsDraggable()
      || state.dragState
      || !event.isPrimary
      || (event.pointerType !== "touch" && event.button !== 0)
    ) return;

    const bounds = state.canvas.getBoundingClientRect();
    if (bounds.width < 1) return;
    state.pendingPointer = null;
    state.dragState = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      width: bounds.width,
      startPhase: state.globePhase,
      startPitch: state.globePitch,
      moved: false,
      pendingPoint: null,
    };
    if (typeof state.canvas.setPointerCapture === "function") {
      try {
        state.canvas.setPointerCapture(event.pointerId);
      } catch {
        // The gesture can still work while the pointer stays over the canvas.
      }
    }
  });

  listen(state, state.canvas, "pointermove", (event) => {
    if (state.dragState && event.pointerId === state.dragState.pointerId) {
      const coalesced = typeof event.getCoalescedEvents === "function"
        ? event.getCoalescedEvents()
        : [];
      const latest = coalesced.length ? coalesced[coalesced.length - 1] : event;
      if (!markDragMoved(latest.clientX, latest.clientY)) return;
      state.dragState.pendingPoint = { x: latest.clientX, y: latest.clientY };
      event.preventDefault();
      requestAnimationWork();
      return;
    }

    if (state.pinned || event.pointerType === "touch") return;
    state.pendingPointer = pointFromEvent(event);
    if (state.pendingPointer) requestAnimationWork();
  });

  listen(state, state.canvas, "pointerup", (event) => finishDrag(event, false));
  listen(state, state.canvas, "pointercancel", (event) => finishDrag(event, true));
  listen(state, state.canvas, "lostpointercapture", (event) => finishDrag(event, true));

  listen(state, state.canvas, "pointerleave", () => {
    if (state.dragState) return;
    state.pendingPointer = null;
    clearUnpinnedSelection();
  });

  listen(state, state.canvas, "click", (event) => {
    if (state.suppressNextClick) {
      state.suppressNextClick = false;
      event.preventDefault();
      return;
    }
    const point = pointFromEvent(event);
    if (!point) return;
    state.pendingPointer = null;
    state.pinned = true;
    inspect(point.u, point.v, "Selected");
  });

  listen(state, state.canvas, "keydown", (event) => {
    const keyDirections = {
      ArrowLeft: [-1, 0],
      ArrowRight: [1, 0],
      ArrowUp: [0, -1],
      ArrowDown: [0, 1],
    };
    const direction = keyDirections[event.key];

    if (direction && (event.ctrlKey || event.altKey || event.metaKey)) {
      if (event.ctrlKey && rotateFromKeyboard(direction)) event.preventDefault();
      return;
    }

    if (
      state.activeView === "globe"
      && canManipulateGlobe()
      && !event.ctrlKey
      && !event.altKey
      && !event.metaKey
      && (event.key === "0" || event.code === "Numpad0")
    ) {
      event.preventDefault();
      const changed = state.globePhase !== 0 || state.globePitch !== 0;
      state.globePhase = 0;
      state.globePitch = 0;
      clearSelection();
      if (changed) scheduleRender();
      return;
    }

    if (event.key === "Escape") {
      event.preventDefault();
      clearSelection();
      return;
    }
    if (!direction) return;
    event.preventDefault();
    const start = state.selection || defaultInspectionPoint();
    const next = nextKeyboardPoint(start, direction, event.shiftKey ? 5 : 1);
    state.pinned = true;
    inspect(next.u, next.v, "Selected");
  });

  listen(state, state.canvas, "focus", () => {
    if (!state.selection && !state.dragState) {
      const point = defaultInspectionPoint();
      inspect(point.u, point.v, "Focused");
    }
  });

  listen(state, state.canvas, "blur", () => {
    if (!state.pinned) clearSelection();
  });

  const handleColorSchemeChange = () => {
    if (state.preferredTheme === "light" || state.preferredTheme === "dark") return;
    applyResolvedTheme();
    scheduleRender();
  };
  if (typeof state.darkColorScheme.addEventListener === "function") {
    listen(state, state.darkColorScheme, "change", handleColorSchemeChange);
  } else if (typeof state.darkColorScheme.addListener === "function") {
    state.darkColorScheme.addListener(handleColorSchemeChange);
    state.removers.push(() => state.darkColorScheme.removeListener(handleColorSchemeChange));
  }

  listen(state, state.documentObject, "visibilitychange", () => {
    if (state.documentObject.hidden) {
      if (state.dragState?.moved) state.renderPending = true;
      cancelActiveDrag();
      cancelScheduledFrame();
      return;
    }
    requestAnimationWork();
  });

  if (typeof windowObject.MutationObserver === "function") {
    const themeObserver = new windowObject.MutationObserver((mutations) => {
      if (!mutations.some((mutation) => mutation.attributeName === "data-theme")) return;
      applyResolvedTheme();
      scheduleRender();
    });
    themeObserver.observe(documentObject.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    state.observers.push(themeObserver);
  }

  if (typeof windowObject.ResizeObserver === "function") {
    const resizeObserver = new windowObject.ResizeObserver(() => scheduleRender());
    resizeObserver.observe(stage);
    state.observers.push(resizeObserver);
  } else {
    listen(state, windowObject, "resize", () => scheduleRender(), { passive: true });
  }

  state.cleanup = () => {
    if (state.disposed) return;
    state.disposed = true;
    state.fetchController.abort();
    cancelScheduledFrame();
    cancelActiveDrag();
    if (state.clickSuppressionTimer) {
      state.windowObject.clearTimeout(state.clickSuppressionTimer);
      state.clickSuppressionTimer = 0;
    }
    for (const observer of state.observers.splice(0)) observer.disconnect();
    for (const remove of state.removers.splice(0)) remove();
    for (const [externalSignal, handler] of state.externalSignals) {
      externalSignal.removeEventListener("abort", handler);
    }
    state.externalSignals.clear();
    state.wasm = null;
    state.cachedImage = null;
    state.cachedPixels = null;
    state.cachedMemoryBuffer = null;
    state.canvas.width = 1;
    state.canvas.height = 1;
    state.root.remove();
    if (ACTIVE_MOUNTS.get(state.container) === state) ACTIVE_MOUNTS.delete(state.container);
  };

  state.setTheme = (nextTheme) => {
    state.preferredTheme = nextTheme;
    applyResolvedTheme();
    scheduleRender();
  };

  applyResolvedTheme();
  updateViewButtons();
  updateViewCopy();
  Object.assign(state, {
    fail,
    scheduleRender,
    setActiveView,
    updateViewButtons,
    updateViewCopy,
  });
  return state;
}

async function instantiateMap(state) {
  if (state.disposed) return;
  if (!state.context || !("WebAssembly" in state.windowObject)) {
    state.fail();
    return;
  }

  try {
    const response = await state.windowObject.fetch(state.resolvedWasmUrl, {
      credentials: "same-origin",
      cache: "no-cache",
      redirect: "error",
      signal: state.fetchController.signal,
    });
    if (!response.ok) throw new Error(`Map module request failed with ${response.status}`);
    if (response.url) {
      sameOriginHttpUrl(response.url, state.resolvedWasmUrl, state.windowObject, "The map response");
    }
    if (state.disposed) return;

    let result;
    if (typeof state.windowObject.WebAssembly.instantiateStreaming === "function") {
      try {
        result = await state.windowObject.WebAssembly.instantiateStreaming(response.clone(), {});
      } catch (streamingError) {
        if (state.disposed || streamingError?.name === "AbortError") return;
        result = await state.windowObject.WebAssembly.instantiate(await response.arrayBuffer(), {});
      }
    } else {
      result = await state.windowObject.WebAssembly.instantiate(await response.arrayBuffer(), {});
    }
    if (state.disposed) return;

    const instance = result.instance || result;
    state.wasm = instance.exports;
    state.supportsGlobePose = typeof state.wasm.render_globe_pose === "function";
    state.supportsGlobeRotation = typeof state.wasm.render_globe === "function";
    state.supportsViewRendering = typeof state.wasm.render_view === "function";
    if (
      !(state.wasm.memory instanceof state.windowObject.WebAssembly.Memory)
      || (!state.supportsViewRendering && typeof state.wasm.render !== "function")
      || typeof state.wasm.pixel_ptr !== "function"
      || typeof state.wasm.pixel_len !== "function"
      || typeof state.wasm.forest_at !== "function"
    ) {
      throw new TypeError("The map module does not provide the expected safe interface");
    }

    state.compatibilityFlat = !(
      state.supportsGlobePose || state.supportsGlobeRotation || state.supportsViewRendering
    );
    if (state.compatibilityFlat) state.setActiveView("flat", { render: false });
    state.updateViewButtons();
    state.updateViewCopy();
    state.scheduleRender({ announce: true });
  } catch (error) {
    if (state.disposed || error?.name === "AbortError") return;
    console.error("Unable to start the local forest map", error);
    state.fail();
  }
}

/**
 * Mount the self-contained forest map into a route-owned container.
 * Repeated calls for the same live container reuse the existing mount.
 * The resolved function is idempotent and removes every per-mount resource.
 */
export async function mountMineralsMap(
  container,
  {
    wasmUrl = new URL("./minerals_map.wasm", import.meta.url),
    signal,
    theme,
  } = {},
) {
  if (!isElement(container)) throw new TypeError("A map container element is required");

  const existing = ACTIVE_MOUNTS.get(container);
  if (existing && !existing.disposed) {
    if (theme !== undefined) existing.setTheme(theme);
    bindExternalSignal(existing, signal);
    return existing.ready;
  }

  const state = createMapController(container, { wasmUrl, theme });
  ACTIVE_MOUNTS.set(container, state);
  bindExternalSignal(state, signal);
  state.ready = instantiateMap(state).then(() => state.cleanup);
  return state.ready;
}
