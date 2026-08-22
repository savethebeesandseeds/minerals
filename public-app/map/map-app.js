import { mountMineralsMap } from "./map-loader.js";

export { mountMineralsMap } from "./map-loader.js";

/**
 * Adapter for the public SPA's optional map-module contract.
 * Catalog and navigation capabilities intentionally remain outside the map.
 */
export async function mountWaajacuMap({
  container,
  theme,
  signal,
  wasmUrl,
} = {}) {
  return mountMineralsMap(container, { theme, signal, wasmUrl });
}
