# Minerals public forest-map renderer

This crate is the renderer used by the static public catalog's map route. It has no
dependencies, allocator, framework, network client, WebGL code, map engine, or
`wasm-bindgen` shim. Rust/WebAssembly selects the categorical map cells,
applies the light or dark palette, draws the coastline, and writes the RGBA
frame. A small same-origin JavaScript loader copies that frame to Canvas 2D.

The atlas supports a fixed equirectangular plane and one manually controlled
orthographic globe. There are no cities, routes, labels, political boundaries,
remote tiles, geolocation, pan, or zoom. Repeated globe frames cache source
classes, geometry, and the current pitch mapping; changing only yaw avoids the
projection math entirely.

## Build

From PowerShell at the repository root:

```powershell
rustup toolchain install 1.96.0 --profile minimal
rustup target add --toolchain 1.96.0 wasm32-unknown-unknown
& .\map-wasm\build.ps1
```

The script runs the native renderer tests, builds the release module, and
copies it to the canonical deployable location at
`public-app/map/minerals_map.wasm`. There is no second server-static copy to
synchronize. The compiled module is checked in so deployment needs no Rust
toolchain. The build script pins Rust 1.96.0 and refuses map-data or WASM hashes
that have not been reviewed. The expected module SHA-256 is
`f095257a885fe1545c7ccf1b18e480da4685b0726f5b9a2532c2cecd6212799f`.

When Node.js is available, verify the compiled browser ABI as well:

```powershell
node .\map-wasm\test-wasm-abi.mjs
```

Raw ABI exported to the browser:

- `render(width, height, theme) -> 1 | 0`
- `render_view(width, height, theme, view) -> 1 | 0`
- `render_globe(width, height, theme, phase) -> 1 | 0`
- `render_globe_pose(width, height, theme, yaw, pitch) -> 1 | 0`
- `pixel_ptr() -> u32`
- `pixel_len() -> u32`
- `forest_at(x, y) -> 0 | 100 | 254 | 255`
- WebAssembly `memory`

`render` remains the flat-view compatibility entry point. `render_view` uses
view 0 for the same flat plane and view 1 for a Greenwich-centred orthographic
globe; other values fail closed. `render_globe` renders the same sphere with
`phase` modulo 65536 representing a full eastward centre-longitude turn: 0 is
Greenwich, 16384 is 90 degrees east, and 32768 is 180 degrees. The
`render_globe_pose` entry point adds a signed `pitch` in the same
65536-units-per-turn scale: positive values centre farther north and values are
clamped to +/-14564 (approximately 80 degrees). Its `yaw` argument has the same
meaning as `render_globe`'s `phase`. The renderer rejects frames larger than
2048 x 1024. `forest_at` uses the most recently rendered view, yaw, and clamped
pitch; it reports 100 for estimated forest presence, 0 for land where forest
is not shown, 254 outside the globe disc, and 255 for water/no estimate or
invalid input.

## Rebuild the pinned map asset

Download the two exact inputs listed in [SOURCES.md](SOURCES.md), verify their
SHA-256 hashes, install the pinned developer-only converter dependency, then
run:

```powershell
python -m pip install -r .\tools\map-demo-requirements.txt
python .\tools\build_map_demo_asset.py `
  --forest-png .\gfc2020-v3-720x360.png `
  --land-geojson .\ne_110m_land.geojson `
  --output .\map-wasm\assets\world_forest_v1.bin
```

The converter refuses unreviewed input hashes, Pillow versions, and derived
output hashes. Raw downloads are not needed by the application and should not
be added to the repository.
