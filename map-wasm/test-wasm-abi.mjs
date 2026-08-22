import { readFile } from "node:fs/promises";

const modulePath = new URL("../public-app/map/minerals_map.wasm", import.meta.url);
const bytes = await readFile(modulePath);
const compiled = await WebAssembly.compile(bytes);
const imports = WebAssembly.Module.imports(compiled);
if (imports.length !== 0) {
  throw new Error(`map WASM unexpectedly imports ${JSON.stringify(imports)}`);
}

const { exports: wasm } = await WebAssembly.instantiate(compiled, {});
for (const name of ["memory", "render", "render_view", "render_globe", "render_globe_pose", "pixel_ptr", "pixel_len", "forest_at"]) {
  if (!(name in wasm)) throw new Error(`missing WebAssembly export: ${name}`);
}

if (wasm.render(720, 360, 0) !== 1) throw new Error("valid render was rejected");
const pointer = Number(wasm.pixel_ptr());
const length = Number(wasm.pixel_len());
const expectedLength = 720 * 360 * 4;
if (length !== expectedLength) throw new Error(`unexpected pixel length: ${length}`);
if (pointer < 0 || pointer + length > wasm.memory.buffer.byteLength) {
  throw new Error("pixel buffer is outside exported memory");
}

const states = new Map([[0, 0], [100, 0], [255, 0]]);
for (let y = 0; y < 360; y += 1) {
  for (let x = 0; x < 720; x += 1) {
    const value = Number(wasm.forest_at(x, y));
    if (!states.has(value)) throw new Error(`unexpected forest state: ${value}`);
    states.set(value, states.get(value) + 1);
  }
}
const expectedStates = new Map([[0, 70_650], [100, 18_883], [255, 169_667]]);
for (const [state, count] of expectedStates) {
  if (states.get(state) !== count) {
    throw new Error(`state ${state} count changed: ${states.get(state)}`);
  }
}

if (wasm.render_view(720, 360, 0, 1) !== 1) {
  throw new Error("orthographic globe render was rejected");
}
if (wasm.pixel_len() !== expectedLength) {
  throw new Error(`unexpected globe pixel length: ${wasm.pixel_len()}`);
}
if (wasm.forest_at(0, 0) !== 254 || wasm.forest_at(719, 359) !== 254) {
  throw new Error("pixels outside the globe disc are not state 254");
}
if (wasm.forest_at(720, 0) !== 255) {
  throw new Error("invalid display coordinates must remain state 255");
}

const globeStates = new Map([[0, 0], [100, 0], [254, 0], [255, 0]]);
const phaseZeroStates = new Uint8Array(720 * 360);
for (let y = 0; y < 360; y += 1) {
  for (let x = 0; x < 720; x += 1) {
    const value = Number(wasm.forest_at(x, y));
    if (!globeStates.has(value)) throw new Error(`unexpected globe state: ${value}`);
    globeStates.set(value, globeStates.get(value) + 1);
    phaseZeroStates[y * 720 + x] = value;
  }
}
const expectedGlobeStates = new Map([
  [0, 25_987],
  [100, 8_711],
  [254, 169_480],
  [255, 55_022],
]);
for (const [state, count] of expectedGlobeStates) {
  if (globeStates.get(state) !== count) {
    throw new Error(`globe state ${state} count changed: ${globeStates.get(state)}`);
  }
}

const globePixels = new Uint8Array(wasm.memory.buffer, pointer, expectedLength);
const phaseZeroPixels = Uint8Array.from(globePixels);
if (
  globePixels[0] !== 238
  || globePixels[1] !== 240
  || globePixels[2] !== 234
  || globePixels[3] !== 255
) {
  throw new Error("outside-disc light palette changed");
}

const sameBytes = (left, right) => {
  if (left.length !== right.length) return false;
  for (let index = 0; index < left.length; index += 1) {
    if (left[index] !== right[index]) return false;
  }
  return true;
};

if (wasm.render_globe(720, 360, 0, 16_384) !== 1) {
  throw new Error("quarter-turn globe render was rejected");
}
const quarterTurnPixels = new Uint8Array(wasm.memory.buffer, pointer, expectedLength);
if (sameBytes(phaseZeroPixels, quarterTurnPixels)) {
  throw new Error("quarter-turn phase did not rotate the globe");
}
let phaseSensitiveSample = false;
for (let y = 0; y < 360 && !phaseSensitiveSample; y += 1) {
  for (let x = 0; x < 720; x += 1) {
    if (wasm.forest_at(x, y) !== phaseZeroStates[y * 720 + x]) {
      phaseSensitiveSample = true;
      break;
    }
  }
}
if (!phaseSensitiveSample) throw new Error("forest_at did not track the quarter-turn phase");

if (wasm.render_globe_pose(720, 360, 0, 0, 8_192) !== 1) {
  throw new Error("pitched globe render was rejected");
}
const pitchedPixels = Uint8Array.from(
  new Uint8Array(wasm.memory.buffer, pointer, expectedLength),
);
if (sameBytes(phaseZeroPixels, pitchedPixels)) {
  throw new Error("positive pitch did not rotate the globe");
}
if (wasm.forest_at(0, 0) !== 254 || wasm.forest_at(720, 0) !== 255) {
  throw new Error("pitched globe hit testing did not preserve outside/invalid states");
}

if (wasm.render_globe_pose(720, 360, 0, 0, 14_564) !== 1) {
  throw new Error("maximum supported pitch was rejected");
}
const clampedPitchPixels = Uint8Array.from(
  new Uint8Array(wasm.memory.buffer, pointer, expectedLength),
);
if (wasm.render_globe_pose(720, 360, 0, 0, 2_147_483_647) !== 1) {
  throw new Error("oversized pitch was rejected instead of clamped");
}
if (!sameBytes(
  clampedPitchPixels,
  new Uint8Array(wasm.memory.buffer, pointer, expectedLength),
)) {
  throw new Error("pitch did not clamp at the documented 80-degree limit");
}

if (wasm.render_globe(720, 360, 0, 65_536) !== 1) {
  throw new Error("wrapped phase globe render was rejected");
}
const wrappedPixels = new Uint8Array(wasm.memory.buffer, pointer, expectedLength);
if (!sameBytes(phaseZeroPixels, wrappedPixels)) {
  throw new Error("phase 65536 did not wrap exactly to phase zero");
}
for (let y = 0; y < 360; y += 1) {
  for (let x = 0; x < 720; x += 1) {
    if (wasm.forest_at(x, y) !== phaseZeroStates[y * 720 + x]) {
      throw new Error("forest_at did not apply phase modulo 65536");
    }
  }
}

if (wasm.render_view(720, 360, 0, 1) !== 1) {
  throw new Error("phase-zero view wrapper was rejected");
}
const wrappedViewPixels = new Uint8Array(wasm.memory.buffer, pointer, expectedLength);
if (!sameBytes(phaseZeroPixels, wrappedViewPixels)) {
  throw new Error("render_view globe is not the phase-zero sphere");
}

if (wasm.render_globe(720, 360, 1, 0) !== 1) {
  throw new Error("dark globe render was rejected");
}
const darkGlobePixels = new Uint8Array(wasm.memory.buffer, pointer, expectedLength);
if (
  darkGlobePixels[0] !== 12
  || darkGlobePixels[1] !== 24
  || darkGlobePixels[2] !== 24
  || darkGlobePixels[3] !== 255
) {
  throw new Error("outside-disc dark palette changed");
}

if (wasm.render_view(720, 360, 0, 2) !== 0 || wasm.pixel_len() !== 0) {
  throw new Error("unknown view did not fail closed");
}
if (wasm.render_globe(2_049, 1, 0, 0) !== 0 || wasm.pixel_len() !== 0) {
  throw new Error("oversized render request did not fail closed");
}

if (wasm.render(720, 360, 0) !== 1 || wasm.forest_at(0, 0) !== 255) {
  throw new Error("legacy render did not restore the flat view");
}

console.log(`WASM ABI smoke test passed (${bytes.length} bytes)`);
