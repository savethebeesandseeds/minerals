(() => {
  "use strict";

  const component = document.querySelector("[data-map-component]");
  if (!(component instanceof HTMLElement)) return;

  const canvas = component.querySelector("[data-forest-map]");
  if (!(canvas instanceof HTMLCanvasElement)) return;

  const stage = component.querySelector("[data-map-stage]");
  const status = component.querySelector("[data-map-status]");
  const statusIndicator = component.querySelector("[data-map-status-indicator]");
  const fallback = component.querySelector("[data-map-fallback]");
  const detail = component.querySelector("[data-map-detail]");
  const detailNote = component.querySelector("[data-map-detail-note]");
  const instructions = component.querySelector("[data-map-instructions]");
  const inspectionStatus = component.querySelector("[data-map-inspection-status]");
  const marker = component.querySelector("[data-map-marker]");
  const viewButtons = Array.from(component.querySelectorAll("[data-map-view]"))
    .filter((button) => button instanceof HTMLButtonElement);
  const root = document.documentElement;
  const context = canvas.getContext("2d", { alpha: false });

  const MAX_RENDER_WIDTH = 2048;
  const MAX_DEVICE_PIXEL_RATIO = 2;
  const DRAG_DEVICE_PIXEL_RATIO = 1;
  const PHASE_TURN = 65_536;
  const DRAG_THRESHOLD_PX = 5;
  const KEYBOARD_YAW_STEP = Math.round(PHASE_TURN / 36);
  const KEYBOARD_PITCH_STEP = Math.round(PHASE_TURN / 72);
  const MAX_PITCH_PHASE = Math.round(PHASE_TURN * 75 / 360);
  const VIEW_FLAT = 0;
  const VIEW_GLOBE = 1;
  let wasm = null;
  let activeView = "globe";
  let supportsGlobePose = false;
  let supportsGlobeRotation = false;
  let supportsViewRendering = false;
  let compatibilityFlat = false;
  let rendererFailed = false;
  let frameRequest = 0;
  let renderPending = false;
  let renderAnnouncementPending = false;
  let pendingPointer = null;
  let dragState = null;
  let suppressNextClick = false;
  let selection = null;
  let pinned = false;
  let globePhase = 0;
  let globePitch = 0;
  let lastRenderKey = "";
  let cachedImage = null;
  let cachedPixels = null;
  let cachedMemoryBuffer = null;
  let cachedPixelPointer = -1;
  let cachedPixelLength = -1;
  let cachedImageIsDirect = false;

  const setStatus = (state, message) => {
    component.dataset.state = state;
    if (statusIndicator instanceof HTMLElement) statusIndicator.dataset.state = state;
    if (!(status instanceof HTMLElement)) return;
    if (status.dataset.state === state && status.textContent.trim() === message) return;
    status.dataset.state = state;
    status.textContent = message;
  };

  const setText = (element, message) => {
    if (!(element instanceof HTMLElement) || element.textContent.trim() === message) return;
    element.textContent = message;
  };

  const hideDetail = () => {
    if (!(detail instanceof HTMLElement)) return;
    detail.hidden = true;
    setText(detail, "");
  };

  const setCanvasLabel = (message) => {
    if (canvas.getAttribute("aria-label") !== message) canvas.setAttribute("aria-label", message);
  };

  const canRenderGlobe = () => (
    supportsGlobePose || supportsGlobeRotation || supportsViewRendering
  );

  const canManipulateGlobe = () => supportsGlobePose || supportsGlobeRotation;

  const globeIsDraggable = () => Boolean(
    wasm
    && !rendererFailed
    && activeView === "globe"
    && canManipulateGlobe(),
  );

  const baseCanvasLabel = () => {
    if (activeView === "flat") {
      return "Flat world map of estimated forest presence in 2020";
    }
    if (!wasm || supportsGlobePose) {
      return "Draggable single orthographic globe map of estimated forest presence in 2020";
    }
    if (supportsGlobeRotation) {
      return "Horizontally draggable single orthographic globe map of estimated forest presence in 2020";
    }
    return "Single orthographic globe map of estimated forest presence in 2020";
  };

  const defaultDetailNote = () => activeView === "globe"
    ? "The inspector reports forest, land, or water inside the globe. Space outside it has no sample; the coastline is a non-data overlay."
    : "The inspector reports the underlying forest, land, or water state; the coastline is a non-data overlay.";

  const globeInstructions = () => {
    if (!wasm || supportsGlobePose) {
      return "Drag to rotate; tap or click to select. Control plus Arrow keys rotates, 0 resets, Arrow keys inspect, and Escape clears.";
    }
    if (supportsGlobeRotation) {
      return "Drag left or right to rotate; tap or click to select. Control plus Left or Right rotates, 0 resets, Arrow keys inspect, and Escape clears.";
    }
    return "Tap or click to select. Arrow keys inspect and Escape clears.";
  };

  const updateViewButtons = () => {
    for (const button of viewButtons) {
      const buttonView = button.dataset.mapView;
      const selected = buttonView === activeView;
      const unavailable = Boolean(wasm) && !canRenderGlobe() && buttonView === "globe";
      button.disabled = unavailable;
      button.setAttribute("aria-disabled", String(unavailable));
      button.setAttribute("aria-checked", String(selected));
      button.tabIndex = selected ? 0 : -1;
    }
  };

  const updateDragAffordance = () => {
    const enabled = globeIsDraggable();
    canvas.dataset.dragEnabled = String(enabled);
    canvas.dataset.dragAxes = enabled
      ? supportsGlobePose ? "both" : "horizontal"
      : "none";
    canvas.dataset.dragging = String(Boolean(enabled && dragState && dragState.moved));
    if (!enabled) {
      canvas.removeAttribute("aria-keyshortcuts");
      return;
    }
    canvas.setAttribute(
      "aria-keyshortcuts",
      supportsGlobePose
        ? "Control+ArrowLeft Control+ArrowRight Control+ArrowUp Control+ArrowDown 0"
        : "Control+ArrowLeft Control+ArrowRight 0",
    );
  };

  const updateViewCopy = () => {
    setCanvasLabel(baseCanvasLabel());
    setText(
      instructions,
      activeView === "globe"
        ? globeInstructions()
        : "Tap or click to select. Arrow keys inspect and Escape clears.",
    );
    hideDetail();
    setText(inspectionStatus, "");
    setText(detailNote, defaultDetailNote());
    updateDragAffordance();
  };

  function cancelActiveDrag() {
    const state = dragState;
    dragState = null;
    pendingPointer = null;
    canvas.dataset.dragging = "false";
    if (!state || typeof canvas.hasPointerCapture !== "function") return;
    try {
      if (canvas.hasPointerCapture(state.pointerId)) {
        canvas.releasePointerCapture(state.pointerId);
      }
    } catch (_captureError) {
      // Pointer capture may already have been released by the browser.
    }
  }

  const fail = () => {
    rendererFailed = true;
    renderPending = false;
    renderAnnouncementPending = false;
    pendingPointer = null;
    if (frameRequest) cancelAnimationFrame(frameRequest);
    frameRequest = 0;
    cancelActiveDrag();
    updateDragAffordance();
    setStatus("error", "Map unavailable");
    if (stage instanceof HTMLElement) stage.setAttribute("aria-busy", "false");
    if (fallback instanceof HTMLElement) fallback.hidden = false;
    canvas.hidden = true;
    if (marker instanceof HTMLElement) marker.hidden = true;
    for (const button of viewButtons) {
      button.disabled = true;
      button.setAttribute("aria-disabled", "true");
    }
    hideDetail();
    setText(detailNote, "The source and limitations remain available.");
  };

  const exportedNumber = (name) => {
    const value = wasm[name];
    const resolved = typeof value === "function" ? value() : value?.value;
    if (typeof resolved !== "number" && typeof resolved !== "bigint") {
      throw new TypeError("Missing numeric WebAssembly export: " + name);
    }
    const number = Number(resolved);
    if (!Number.isSafeInteger(number) || number < 0) {
      throw new RangeError("Invalid WebAssembly export: " + name);
    }
    return number;
  };

  const readyMessage = () => {
    return "Map available.";
  };

  const renderMap = (announceStatus) => {
    if (!wasm || !context) return false;

    const bounds = canvas.getBoundingClientRect();
    if (bounds.width < 1) return false;

    const maximumPixelRatio = dragState && dragState.moved
      ? DRAG_DEVICE_PIXEL_RATIO
      : MAX_DEVICE_PIXEL_RATIO;
    const pixelRatio = Math.min(
      Math.max(window.devicePixelRatio || 1, 1),
      maximumPixelRatio,
    );
    const width = Math.max(2, Math.min(MAX_RENDER_WIDTH, Math.round(bounds.width * pixelRatio)));
    const height = Math.max(1, Math.round(width / 2));
    const theme = root.getAttribute("data-theme") === "dark" ? 1 : 0;
    const phase = Math.floor(globePhase) & 0xffff;
    const pitch = Math.trunc(globePitch);
    const renderKey = [
      activeView,
      width,
      height,
      theme,
      activeView === "globe" ? phase : 0,
      activeView === "globe" && supportsGlobePose ? pitch : 0,
    ].join(":");

    if (renderKey === lastRenderKey) {
      if (stage instanceof HTMLElement) stage.setAttribute("aria-busy", "false");
      if (announceStatus) setStatus("ready", readyMessage());
      return false;
    }

    const renderLegacy = () => {
      const renderResult = Number(wasm.render(width, height, theme));
      return renderResult;
    };
    let renderResult;
    if (activeView === "globe") {
      if (supportsGlobePose) {
        renderResult = Number(wasm.render_globe_pose(width, height, theme, phase, pitch));
      } else if (supportsGlobeRotation) {
        renderResult = Number(wasm.render_globe(width, height, theme, phase));
      } else {
        renderResult = Number(wasm.render_view(width, height, theme, VIEW_GLOBE));
      }
    } else {
      renderResult = supportsViewRendering
        ? Number(wasm.render_view(width, height, theme, VIEW_FLAT))
        : renderLegacy();
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

    const memoryBuffer = wasm.memory.buffer;
    const memoryLength = memoryBuffer.byteLength;
    if (pixelPointer > memoryLength || pixelLength > memoryLength - pixelPointer) {
      throw new RangeError("WebAssembly returned an out-of-bounds pixel buffer");
    }

    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }

    const cacheMatches = (
      cachedImage
      && cachedMemoryBuffer === memoryBuffer
      && cachedPixelPointer === pixelPointer
      && cachedPixelLength === pixelLength
      && cachedImage.width === width
      && cachedImage.height === height
    );
    if (!cacheMatches) {
      cachedPixels = new Uint8ClampedArray(memoryBuffer, pixelPointer, pixelLength);
      cachedImageIsDirect = false;
      if (typeof ImageData === "function") {
        try {
          cachedImage = new ImageData(cachedPixels, width, height);
          cachedImageIsDirect = true;
        } catch (_imageDataError) {
          cachedImage = null;
        }
      }
      if (!cachedImage) cachedImage = context.createImageData(width, height);
      cachedMemoryBuffer = memoryBuffer;
      cachedPixelPointer = pixelPointer;
      cachedPixelLength = pixelLength;
    }
    if (!cachedImageIsDirect) cachedImage.data.set(cachedPixels);
    context.putImageData(cachedImage, 0, 0);
    lastRenderKey = renderKey;

    if (stage instanceof HTMLElement) stage.setAttribute("aria-busy", "false");
    if (announceStatus) setStatus("ready", readyMessage());
    if (selection) inspect(selection.u, selection.v, pinned ? "Selected" : "Hovered");
    return true;
  };

  const hasFrameWork = () => Boolean(
    renderPending || pendingPointer || (dragState && dragState.pendingPoint),
  );

  const requestFrame = () => {
    if (frameRequest || rendererFailed || document.hidden || !hasFrameWork()) return;
    frameRequest = requestAnimationFrame(flushFrame);
  };

  const scheduleRender = ({ announce = false } = {}) => {
    renderPending = true;
    renderAnnouncementPending ||= announce;
    requestFrame();
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
    if (!dragState || !dragState.moved || !dragState.pendingPoint) return;
    const point = dragState.pendingPoint;
    dragState.pendingPoint = null;
    const deltaX = point.x - dragState.startX;
    const deltaY = point.y - dragState.startY;
    const nextPhase = normalizePhase(
      dragState.startPhase - deltaX * PHASE_TURN / dragState.width,
    );
    const nextPitch = supportsGlobePose
      ? clampPitch(dragState.startPitch + deltaY * PHASE_TURN / dragState.width)
      : globePitch;
    if (nextPhase === globePhase && nextPitch === globePitch) return;
    globePhase = nextPhase;
    globePitch = nextPitch;
    renderPending = true;
  };

  function flushFrame() {
    frameRequest = 0;
    applyPendingDrag();

    if (renderPending) {
      const announceStatus = renderAnnouncementPending;
      renderPending = false;
      renderAnnouncementPending = false;
      try {
        renderMap(announceStatus);
      } catch (error) {
        console.error("Unable to render the local forest map", error);
        fail();
        return;
      }
    }

    if (pendingPointer && !dragState) {
      const point = pendingPointer;
      pendingPointer = null;
      inspect(point.u, point.v, "Hovered");
    }
    requestFrame();
  }

  const forestStateAt = (u, v) => {
    if (!wasm || typeof wasm.forest_at !== "function") return null;
    const x = Math.min(canvas.width - 1, Math.max(0, Math.floor(u * canvas.width)));
    const y = Math.min(canvas.height - 1, Math.max(0, Math.floor(v * canvas.height)));
    const value = Number(wasm.forest_at(x, y));
    if (value === 100) return "forest";
    if (value === 0) return "land";
    if (value === 254) return "outside";
    if (value === 255) return "water";
    return null;
  };

  const inspect = (u, v, interaction) => {
    const normalizedU = Math.min(1, Math.max(0, u));
    const normalizedV = Math.min(1, Math.max(0, v));
    selection = { u: normalizedU, v: normalizedV };

    if (marker instanceof HTMLElement) {
      marker.hidden = false;
      marker.style.left = String(normalizedU * 100) + "%";
      marker.style.top = String(normalizedV * 100) + "%";
    }

    const forestState = forestStateAt(normalizedU, normalizedV);
    const stateText = {
      forest: "Estimated forest presence at this sampled cell.",
      land: "Forest not shown at this sampled cell.",
      water: "Water / no estimate at this sampled cell.",
      outside: "Outside the globe; no map sample at this point.",
    };
    const chipText = {
      forest: "Forest",
      land: "Land",
      water: "Water",
      outside: "No data",
    };
    const accessibleResult = forestState === null
      ? interaction + " point: sample unavailable."
      : interaction + " point. " + stateText[forestState];
    if (detail instanceof HTMLElement) {
      detail.hidden = false;
      setText(
        detail,
        forestState === null ? "No data" : chipText[forestState],
      );
    }
    if (interaction !== "Hovered") setText(inspectionStatus, accessibleResult);
    setText(
      detailNote,
      forestState === "outside"
        ? "Move onto the orthographic globe to inspect the map, or choose Flat for the full rectangular extent."
        : pinned
          ? "Selection kept. Click another point or press Escape to clear it. The coastline is a non-data overlay."
          : defaultDetailNote(),
    );
    if (interaction !== "Hovered") {
      setCanvasLabel(
        forestState === null
          ? baseCanvasLabel() + ". The current sample is unavailable."
          : baseCanvasLabel() + ". " + stateText[forestState],
      );
    }
  };

  const pointFromEvent = (event) => {
    const bounds = canvas.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) return null;
    return {
      u: (event.clientX - bounds.left) / bounds.width,
      v: (event.clientY - bounds.top) / bounds.height,
    };
  };

  const clearSelection = () => {
    pinned = false;
    selection = null;
    pendingPointer = null;
    if (marker instanceof HTMLElement) marker.hidden = true;
    updateViewCopy();
  };

  const clearUnpinnedSelection = () => {
    if (!pinned) clearSelection();
  };

  const markDragMoved = (x, y) => {
    if (!dragState || dragState.moved) return Boolean(dragState && dragState.moved);
    const deltaX = x - dragState.startX;
    const deltaY = y - dragState.startY;
    const distance = supportsGlobePose ? Math.hypot(deltaX, deltaY) : Math.abs(deltaX);
    if (distance < DRAG_THRESHOLD_PX) return false;
    dragState.moved = true;
    pendingPointer = null;
    clearSelection();
    canvas.dataset.dragging = "true";
    return true;
  };

  const finishDrag = (event, canceled) => {
    if (!dragState || event.pointerId !== dragState.pointerId) return;
    const state = dragState;
    if (
      state.moved
      && Number.isFinite(event.clientX)
      && Number.isFinite(event.clientY)
    ) {
      state.pendingPoint = { x: event.clientX, y: event.clientY };
      applyPendingDrag();
    }
    dragState = null;
    canvas.dataset.dragging = "false";
    if (typeof canvas.hasPointerCapture === "function") {
      try {
        if (canvas.hasPointerCapture(state.pointerId)) {
          canvas.releasePointerCapture(state.pointerId);
        }
      } catch (_captureError) {
        // Pointer capture may already have been released by the browser.
      }
    }
    if (!state.moved) return;

    pendingPointer = null;
    if (!canceled) {
      suppressNextClick = true;
      window.setTimeout(() => {
        suppressNextClick = false;
      }, 0);
    }
    scheduleRender();
  };

  const setActiveView = (nextView, { render = true } = {}) => {
    if (nextView !== "flat" && nextView !== "globe") return;
    if (nextView === "globe" && wasm && !canRenderGlobe()) return;

    const changed = activeView !== nextView;
    activeView = nextView;
    updateViewButtons();
    if (!changed) return;
    cancelActiveDrag();
    clearSelection();

    if (render && wasm) {
      if (stage instanceof HTMLElement) stage.setAttribute("aria-busy", "true");
      setStatus("loading", "Loading map…");
      scheduleRender({ announce: true });
    }
  };

  for (const button of viewButtons) {
    button.addEventListener("click", () => {
      setActiveView(button.dataset.mapView);
    });

    button.addEventListener("keydown", (event) => {
      const enabledButtons = viewButtons.filter((candidate) => !candidate.disabled);
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

  updateViewButtons();
  updateViewCopy();

  canvas.addEventListener("pointerdown", (event) => {
    if (
      !globeIsDraggable()
      || dragState
      || !event.isPrimary
      || (event.pointerType !== "touch" && event.button !== 0)
    ) return;

    const bounds = canvas.getBoundingClientRect();
    if (bounds.width < 1) return;
    pendingPointer = null;
    dragState = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      width: bounds.width,
      startPhase: globePhase,
      startPitch: globePitch,
      moved: false,
      pendingPoint: null,
    };
    if (typeof canvas.setPointerCapture === "function") {
      try {
        canvas.setPointerCapture(event.pointerId);
      } catch (_captureError) {
        // The gesture can still work while the pointer stays over the canvas.
      }
    }
  });

  canvas.addEventListener("pointermove", (event) => {
    if (dragState && event.pointerId === dragState.pointerId) {
      const coalesced = typeof event.getCoalescedEvents === "function"
        ? event.getCoalescedEvents()
        : [];
      const latest = coalesced.length ? coalesced[coalesced.length - 1] : event;
      if (!markDragMoved(latest.clientX, latest.clientY)) return;
      dragState.pendingPoint = { x: latest.clientX, y: latest.clientY };
      event.preventDefault();
      requestFrame();
      return;
    }

    if (pinned || event.pointerType === "touch") return;
    pendingPointer = pointFromEvent(event);
    if (pendingPointer) requestFrame();
  });

  canvas.addEventListener("pointerup", (event) => {
    finishDrag(event, false);
  });

  canvas.addEventListener("pointercancel", (event) => {
    finishDrag(event, true);
  });

  canvas.addEventListener("lostpointercapture", (event) => {
    finishDrag(event, true);
  });

  canvas.addEventListener("pointerleave", () => {
    if (dragState) return;
    pendingPointer = null;
    clearUnpinnedSelection();
  });

  canvas.addEventListener("click", (event) => {
    if (suppressNextClick) {
      suppressNextClick = false;
      event.preventDefault();
      return;
    }
    const point = pointFromEvent(event);
    if (!point) return;
    pendingPointer = null;
    pinned = true;
    inspect(point.u, point.v, "Selected");
  });

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

      if (activeView !== "globe" || !canRenderGlobe()) return candidate;
      const state = forestStateAt(candidate.u, candidate.v);
      if (state !== "outside") return candidate;
      if (
        (direction[0] < 0 && candidate.u === 0)
        || (direction[0] > 0 && candidate.u === 1)
        || (direction[1] < 0 && candidate.v === 0)
        || (direction[1] > 0 && candidate.v === 1)
      ) {
        break;
      }
    }

    return start;
  };

  const rotateFromKeyboard = (direction) => {
    if (activeView !== "globe" || !canManipulateGlobe()) return false;
    let nextPhase = globePhase;
    let nextPitch = globePitch;
    if (direction[0] < 0) nextPhase = normalizePhase(globePhase + KEYBOARD_YAW_STEP);
    if (direction[0] > 0) nextPhase = normalizePhase(globePhase - KEYBOARD_YAW_STEP);
    if (supportsGlobePose && direction[1] < 0) {
      nextPitch = clampPitch(globePitch - KEYBOARD_PITCH_STEP);
    }
    if (supportsGlobePose && direction[1] > 0) {
      nextPitch = clampPitch(globePitch + KEYBOARD_PITCH_STEP);
    }
    if (direction[1] !== 0 && !supportsGlobePose) return false;
    if (nextPhase === globePhase && nextPitch === globePitch) return true;
    globePhase = nextPhase;
    globePitch = nextPitch;
    clearSelection();
    scheduleRender();
    return true;
  };

  canvas.addEventListener("keydown", (event) => {
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
      activeView === "globe"
      && canManipulateGlobe()
      && !event.ctrlKey
      && !event.altKey
      && !event.metaKey
      && (event.key === "0" || event.code === "Numpad0")
    ) {
      event.preventDefault();
      const changed = globePhase !== 0 || globePitch !== 0;
      globePhase = 0;
      globePitch = 0;
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
    const start = selection || defaultInspectionPoint();
    const multiplier = event.shiftKey ? 5 : 1;
    const next = nextKeyboardPoint(start, direction, multiplier);
    pinned = true;
    inspect(next.u, next.v, "Selected");
  });

  canvas.addEventListener("focus", () => {
    if (!selection && !dragState) {
      const point = defaultInspectionPoint();
      inspect(point.u, point.v, "Focused");
    }
  });

  canvas.addEventListener("blur", () => {
    if (!pinned) clearSelection();
  });

  document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
      if (dragState && dragState.moved) renderPending = true;
      cancelActiveDrag();
      if (frameRequest) cancelAnimationFrame(frameRequest);
      frameRequest = 0;
      return;
    }
    requestFrame();
  });

  const instantiate = async () => {
    if (!context || !("WebAssembly" in window)) {
      fail();
      return;
    }

    try {
      const configuredPath = canvas.dataset.wasmUrl;
      if (!configuredPath) throw new TypeError("Missing local WebAssembly URL");
      const url = new URL(configuredPath, window.location.href);
      if (url.origin !== window.location.origin) {
        throw new TypeError("The map WebAssembly module must be same-origin");
      }

      const response = await fetch(url, {
        credentials: "same-origin",
        cache: "no-cache",
      });
      if (!response.ok) throw new Error("Map module request failed with " + response.status);

      let result;
      if (typeof WebAssembly.instantiateStreaming === "function") {
        try {
          result = await WebAssembly.instantiateStreaming(response.clone(), {});
        } catch (_streamingError) {
          result = await WebAssembly.instantiate(await response.arrayBuffer(), {});
        }
      } else {
        result = await WebAssembly.instantiate(await response.arrayBuffer(), {});
      }

      const instance = result.instance || result;
      wasm = instance.exports;
      supportsGlobePose = typeof wasm.render_globe_pose === "function";
      supportsGlobeRotation = typeof wasm.render_globe === "function";
      supportsViewRendering = typeof wasm.render_view === "function";
      if (
        !(wasm.memory instanceof WebAssembly.Memory)
        || (!supportsViewRendering && typeof wasm.render !== "function")
        || typeof wasm.pixel_ptr !== "function"
        || typeof wasm.pixel_len !== "function"
        || typeof wasm.forest_at !== "function"
      ) {
        throw new TypeError("The map module does not provide the expected safe interface");
      }

      compatibilityFlat = !canRenderGlobe();
      if (compatibilityFlat) {
        setActiveView("flat", { render: false });
      } else {
        updateViewButtons();
        updateViewCopy();
      }

      scheduleRender({ announce: true });

      if ("ResizeObserver" in window && stage instanceof HTMLElement) {
        const observer = new ResizeObserver(() => scheduleRender());
        observer.observe(stage);
      } else {
        window.addEventListener("resize", () => scheduleRender(), { passive: true });
      }

      const themeObserver = new MutationObserver((mutations) => {
        if (mutations.some((mutation) => mutation.attributeName === "data-theme")) {
          scheduleRender();
        }
      });
      themeObserver.observe(root, { attributes: true, attributeFilter: ["data-theme"] });
    } catch (error) {
      console.error("Unable to start the local forest map", error);
      fail();
    }
  };

  instantiate();
})();
