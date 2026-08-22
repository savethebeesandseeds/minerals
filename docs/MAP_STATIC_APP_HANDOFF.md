# Map handoff for the static public application

The public catalog lives under `public-app/`: one static HTML application loads
a sanitized, read-only SQLite snapshot through SQLite WebAssembly. The private
Axum application remains the administration and publication control plane.

The delivered map stays independent of the catalog database. Its deployable
package contains these files without Askama placeholders or server API
dependencies:

```text
public-app/map/map.css
public-app/map/map-app.js
public-app/map/map-loader.js
public-app/map/minerals_map.wasm
```

The JavaScript contract is an ES module exporting an idempotent mount function:

```js
export async function mountWaajacuMap({
  container,
  catalog,
  navigate,
  locale,
  theme,
  signal,
}) {
  // Mount into container and return an optional cleanup function.
}
```

`catalog` is a read-only facade with `search(input)`, `detail(slug)`,
`evidence(slug)`, and `offers(slug)` promise methods. `navigate(to, options?)`
uses the SPA router. `signal` is aborted when the route is left or superseded.
The map should resolve its WASM URL with
`new URL("./minerals_map.wasm", import.meta.url)`.

Requirements:

- Resolve the WASM URL relative to `import.meta.url`; do not hard-code
  `/static/...` or another deployment root.
- Fetch only same-origin assets and require no dynamic backend endpoint.
- Initialize only after the SPA has mounted a connected map container. This can
  be the full map route or the compact All Minerals preview.
- Support route re-entry without duplicate listeners, observers, or canvases;
  returning a cleanup function is preferred, and honor the supplied abort
  signal.
- Preserve the current accessible canvas labels, keyboard inspection, fallback
  copy, and flat/globe controls.
- Keep the map database-independent. If mineral data is added later, query it
  through the public catalog worker rather than opening the private database.
- The All Minerals preview is world/forest context only. It must not imply that
  the map contains mineral occurrences until reviewed locality data is added to
  the public snapshot.
- Keep all four map files in the static publisher's explicit application-asset
  allowlist. `map-app.js` imports `map-loader.js`; neither file is optional when
  the map package is installed.

`map-app.js` is the stable SPA adapter; `map-loader.js` contains the reusable
map controller behind it.
