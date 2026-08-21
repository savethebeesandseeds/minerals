# Map handoff for the static public application

The public catalog lives under `public-app/`: one static HTML application loads
a sanitized, read-only SQLite snapshot through SQLite WebAssembly. The private
Axum application remains the administration and publication control plane.

The delivered map stays independent of the catalog database. Its deployable
package contains these files without Askama placeholders or server API
dependencies:

```text
public-app/map/map-loader.js
public-app/map/map.css
public-app/map/minerals_map.wasm
```

`map-loader.js` is the ESM entry point and exports an idempotent mount function
directly:

```js
export async function mountMineralsMap(container, {
  wasmUrl = new URL("./minerals_map.wasm", import.meta.url),
  theme,
  signal,
} = {}) {
  // Mount into container and return an optional cleanup function.
}
```

`container` is the route-owned connected element, `theme` is `"light"` or
`"dark"`, and `signal` is aborted when the route is left or superseded. The
optional `wasmUrl` exists for controlled embedding and tests; its default is
resolved relative to `import.meta.url`. The production SPA passes only
`container`, `theme`, and `signal`. The map has no catalog or navigation
facade because it does not consume mineral records.

The current `minerals_map.wasm` is 265,477 bytes with SHA-256
`f095257a885fe1545c7ccf1b18e480da4685b0726f5b9a2532c2cecd6212799f`.
It has no imports and exposes this dependency-free ABI:

```text
memory
render(width, height, theme)
render_view(width, height, theme, view)
render_globe(width, height, theme, yaw)
render_globe_pose(width, height, theme, yaw, pitch)
pixel_ptr()
pixel_len()
forest_at(x, y)
```

Yaw is a modulo-65,536 full turn. Pitch uses the same signed units and is
clamped safely by the renderer. Projection, pose application, rasterization,
and hit-testing stay in Rust/WASM; DOM, pointer capture, keyboard semantics,
canvas presentation, and accessibility stay in JavaScript.

`public-app/map/` is the canonical source and build destination for all three
deployable map assets. `map-wasm/build.ps1`, the native source contract test,
and `map-wasm/test-wasm-abi.mjs` all validate that location directly; no
duplicate map package is kept under the private server's `static/` directory.

Requirements:

- Resolve the WASM URL relative to `import.meta.url`; do not hard-code
  `/static/...` or another deployment root.
- Fetch only same-origin assets and require no dynamic backend endpoint.
- Initialize only after the SPA has mounted the map route.
- Support route re-entry without duplicate listeners, observers, or canvases;
  returning a cleanup function is preferred, and honor the supplied abort
  signal.
- Preserve the current accessible canvas labels, keyboard inspection, fallback
  copy, and flat/globe controls.
- Keep the fixed Greenwich-centred default. The globe does not rotate
  automatically: direct pointer/touch dragging changes yaw and pitch,
  Control+Arrow keys provide keyboard rotation, and `0` resets the pose.
- Coalesce gesture updates into animation frames and redraw only after load,
  resize, theme, projection, or direct pose changes.
- Keep the map database-independent. If mineral data is added later, query it
  through the public catalog worker rather than opening the private database.
- Keep all three map files in the static publisher's explicit
  application-asset allowlist. `map-loader.js` dynamically resolves both the
  stylesheet and WASM beside itself; all three files are required when the map
  package is installed.
