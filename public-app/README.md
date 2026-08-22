# Waajacu public catalog app

This directory is a standalone, static browser application. The browser loads a content-addressed public SQLite projection; it never opens the operational registry or sends SQL from the main thread.

## Serve and deploy

`public-app/` is the checked-in application source, while `public-catalog/` is
the checked-in sanitized data source. Neither directory is deployed by itself.
On every push to `main`, the Pages workflow uses `export-public
--assemble-catalog` to copy only the explicit application allowlist and the four
validated catalog artifacts into a fresh release directory. It then proves that
the deployed live files match that commit. The operational `data/minerals.db`
is never an input to GitHub Pages.

Serve the assembled release at an origin root or same-origin directory subpath
over HTTPS. Plain HTTP is supported only on literal loopback addresses for
development. Opening `index.html` with `file:` is unsupported because module
workers, WebAssembly, Web Crypto, and `fetch()` require a secure HTTP origin.
Application, worker, map, manifest, database, and SQLite URLs resolve from their
owning modules, so a release mounted at a path such as `/catalog/` stays within
that path.

The deployment must:

- serve `.mjs` with a JavaScript MIME type and `.wasm` as `application/wasm`; prefer `application/vnd.sqlite3` for `.sqlite3` (a generic binary MIME type is also safe because the database is fetched as bytes rather than executed);
- use canonical hash routes for portable directory-subpath deployments; clean-path rewrites are optional and require host-specific asset-base handling;
- send `Cache-Control: no-cache` for `catalog-manifest.json`, and may send `Cache-Control: public, max-age=31536000, immutable` for `data/catalog-<sha256>.sqlite3`;
- keep the generated `.sqlite3.gz` beside the canonical database: the worker prefers that smaller same-origin file, decompresses it with the browser's native gzip stream, bounds the decoded size, verifies the manifest SHA-256, and falls back to the canonical `.sqlite3` file when the native decoder or sidecar is unavailable;
- when the host supports transparent precompressed-file negotiation, it may also select the generated `.sqlite3.br` or `.sqlite3.gz` representation for the canonical `.sqlite3` URL, preferring Brotli, then gzip, then the uncompressed file; `fetch()` supplies the decoded bytes that the same manifest checks verify;
- preserve same-origin URLs for the manifest, database, worker, SQLite runtime, and optional map module; and
- reproduce the CSP in `index.html` as an HTTP response header in production, adding `frame-ancestors 'none'` (which a CSP meta element cannot enforce) and `X-Content-Type-Options: nosniff`.

For a local smoke test, run any static HTTP server in this directory and use the hash route form described below. The dependency-free unit suite is `node tests.mjs`.

## Runtime files

Generated releases add the manifest and content-addressed database to the checked-in application files:

```text
assets/logo_transparent.png
assets/logo_transparent_dark.png
catalog-manifest.json
data/catalog-<64 lowercase hex SHA-256>.sqlite3
data/catalog-<64 lowercase hex SHA-256>.sqlite3.br
data/catalog-<64 lowercase hex SHA-256>.sqlite3.gz
vendor/sqlite/index.mjs
vendor/sqlite/sqlite3.wasm
vendor/sqlite/LICENSE.txt
```

`vendor/sqlite/index.mjs` and `vendor/sqlite/sqlite3.wasm` must be the matching browser ESM and WASM artifacts from official `@sqlite.org/sqlite-wasm` version `3.53.0-build1`. The worker imports `./vendor/sqlite/index.mjs`; do not substitute an unrelated SQLite wrapper or a different WASM build.

## Precompressed database delivery

The exporter writes Brotli and gzip representations beside every canonical
database. The manifest continues to name the uncompressed
`data/catalog-<sha256>.sqlite3` URL and its uncompressed byte length and digest.
Configure the static origin to select the matching sidecar from the request's
`Accept-Encoding` header:

```text
Accept-Encoding includes br    -> serve .sqlite3.br with Content-Encoding: br
Accept-Encoding includes gzip  -> serve .sqlite3.gz with Content-Encoding: gzip
otherwise                      -> serve .sqlite3 unchanged
```

Every variant must use `Content-Type: application/vnd.sqlite3`,
`Vary: Accept-Encoding`, and `Cache-Control: public, max-age=31536000,
immutable`. Use representation-specific strong ETags, or weak ETags; do not
reuse one strong ETag across differently encoded bodies. Caddy's
`file_server { precompressed br gzip }` and equivalent CDN
precompressed-file features implement this negotiation. For nginx, enable
`gzip_static on;` and `gzip_vary on;`; `gzip_static` requires nginx's
`ngx_http_gzip_static_module`, and a build with the separate Brotli module can
additionally use `brotli_static on;`. A basic static server that lacks
precompressed-file negotiation safely falls back to the uncompressed
`.sqlite3` file. GitHub Pages uses that fallback; Cloudflare can dynamically
compress the canonical response as described in the
[Pages deployment guide](../docs/GITHUB_PAGES.md#compression-rule).

HTTP Fetch transparently decodes `Content-Encoding` before the worker receives
the response body. The worker therefore needs no decompression dependency: it
still verifies the decoded byte length and SHA-256 before opening SQLite.

The checked-in `assets/` directory contains the light- and dark-theme macaw
artwork used by the public shell. The exporter treats both PNG files as required
allowlisted assets and copies their bytes unchanged into every release.

The repository exporter also copies the finalized map package at
`map/map-loader.js`, `map/map.css`, and `map/minerals_map.wasm`. None of those
files is fetched before the `/map` route
mounts its connected container, and their absence must not break catalog
routes. This directory is also the canonical map build output; there is no
duplicate copy under the private server's `static/` assets.

## Manifest contract

`catalog-manifest.json` is UTF-8 JSON with this v1 shape:

```json
{
  "format": "waajacu-public-catalog-v1",
  "schema_version": 1,
  "database": {
    "path": "data/catalog-<64 lowercase hexadecimal characters>.sqlite3",
    "sha256": "sha256:<same 64 lowercase hexadecimal characters>",
    "bytes": 123456
  },
  "generated_at": "<RFC 3339 timestamp>",
  "release_id": "sha256:<64 lowercase hexadecimal characters>",
  "mineral_count": 123
}
```

`database.path` is exactly `data/catalog-<bare digest from database.sha256>.sqlite3`, with no query or fragment. `release_id` is an independent logical-release digest and is not required to equal the database digest. `database.bytes` is a positive safe integer and `mineral_count` is a non-negative safe integer. The app rejects an unknown format or schema version, malformed identifiers, cross-origin or non-content-addressed database paths, and inconsistent release metadata.

## SQLite schema v1

Tables are created in this order. Column names below are the ordered public projection contract; no live registry IDs or private foreign keys are exposed.

```text
catalog_meta(key, value)

minerals(
  slug, public_id, canonical_name, formula, description, mineral_family,
  nomenclature_status, verification_status, data_quality_score, source_kind,
  license_spdx, cas_number, identifiers_json, properties_json, safety_json,
  discovery_country, first_reference, second_reference, source_status,
  evidence_count, active_offer_count
)

evidence(
  mineral_slug, position, title, publisher, canonical_url, license_spdx,
  claim_scope, claim_json, confidence, review_status, retrieved_at, content_hash,
  attribution_party, work_title, work_url, license_url, changes_notice,
  no_endorsement_notice, derived_output_license_spdx
)

offers(
  mineral_slug, position, provider_name, provider_slug,
  provider_verification_status, provider_trust_score, title, product_url,
  currency_code, price_minor, currency_exponent, pricing_basis,
  minimum_order_quantity, minimum_order_unit, stock_status, purity_text, grade,
  origin_country_code, verification_status, last_checked_at, expires_at
)

mineral_search(
  slug UNINDEXED, canonical_name, formula, mineral_family, search_text
)
```

`catalog_meta` is exactly `key TEXT PRIMARY KEY, value TEXT NOT NULL` and uses `WITHOUT ROWID`. It contains exactly the TEXT keys `format`, `schema_version`, `generated_at`, `mineral_count`, and `release_id`; their values must agree with the manifest (`schema_version` is `"1"` and `mineral_count` is decimal text).

The other declared v1 types and constraints are also fixed. `minerals` uses a TEXT primary-key `slug`, a unique non-null TEXT `public_id`, non-null TEXT for every other text field except nullable `cas_number`, non-null REAL `data_quality_score` constrained to 0–1, valid non-null JSON text for the three `*_json` columns, and non-negative non-null INTEGER evidence/offer counts. `evidence` has primary key `(mineral_slug, position)`, a foreign key to `minerals(slug)`, non-negative INTEGER `position`, REAL `confidence` constrained to 0–1, valid `claim_json`, the nullable attribution/notice columns listed after `content_hash`, and otherwise non-null TEXT. `offers` has the same composite-key/foreign-key pattern, non-negative INTEGER `position`, REAL `provider_trust_score` constrained to 0–1, nullable non-negative INTEGER `price_minor`, INTEGER `currency_exponent` constrained to 0–6, nullable positive REAL `minimum_order_quantity`, nullable TEXT `expires_at`, and otherwise non-null TEXT. All three projection tables use `WITHOUT ROWID`.

`mineral_search` is FTS5 with the five implicit-type columns shown above and tokenizer `unicode61 remove_diacritics 2`. The worker validates ordered names, declared affinities, nullability, primary and foreign keys, the public-ID uniqueness constraint, `WITHOUT ROWID`, the tokenizer, metadata, JSON validity, row counts, search coverage, and orphan references before serving queries.

All search, pagination, detail, evidence, and offer operations are fixed worker operations with bounded, parameterized SQL. The UI cannot submit arbitrary SQL. JSON text is parsed defensively, and catalog text is rendered with DOM text APIs rather than HTML injection.

## Verification and read-only guarantees

The worker validates the manifest, fetches the named database, checks the exact byte count, computes SHA-256 with Web Crypto, and compares the bare result to the digest in both `database.sha256` and the content-addressed filename. Only then does it deserialize through the official SQLite WASM API with `SQLITE_DESERIALIZE_READONLY | SQLITE_DESERIALIZE_FREEONCLOSE`, enable `PRAGMA query_only`, and validate schema and metadata. Any mismatch fails closed; unverified bytes are never queried. This is an integrity check rooted in the same-origin manifest, not a digital signature: deployment still depends on HTTPS and origin security for authenticity.

## Routing

Generated links use hash routes (`#/`, `#/minerals`, `#/minerals/:slug`, `#/map`, and `#/about`) so an ordinary static host works without rewrite rules. Navigation and back/forward use the History API; search state remains in `q`, `page`, and `page_size` query parameters. Clean pathname routes are also parsed when a host rewrites them to the root `index.html`, and a query fallback is accepted:

- canonical hash route: `/#/minerals?q=smoky&page=2`
- query route: `/?route=%2Fminerals%2Fquartz%3Fq%3Dsmoky%26page%3D2`

Navigation remains client-side in the selected mode, and back/forward events rerender the route. Slugs, page numbers, page sizes, and search text are normalized and bounded before they reach the worker.

Theme and UI-language choices are browser-local preferences. They do not alter the immutable catalog database.

## Optional map integration

This checkout and its exporter bundle the finalized map package under `map/`.
The shell still treats it as an optional runtime feature so a custom
catalog-only deployment degrades gracefully when those files are omitted.
`index.html` identifies the ESM entry point with:

```html
<meta name="waajacu-map-module" content="./map/map-loader.js">
```

The application dynamically imports that URL only after the `/map` route has inserted its map markup and connected container into the document. The module must export:

```js
export async function mountMineralsMap(container, {
  theme,
  signal,
}) {
  // Mount into container. Return an optional cleanup function.
}
```

The arguments are:

- `container`: the connected `HTMLElement` owned by the route;
- `theme`: the resolved `"light"` or `"dark"` theme;
- `signal`: an `AbortSignal` that fires when the route is left or superseded.

`mountMineralsMap` may resolve to `undefined` or a cleanup function. On route
teardown the shell aborts `signal` and then invokes that cleanup function once.
The module keeps all rendering inside `container`, honors aborts, and does not
replace history, global preference handlers, or the catalog worker. It renders
only after load, resize, theme, projection, or direct globe input; the globe has
a fixed Greenwich-centred default and never rotates automatically. A missing
module is a supported deployment state and produces an accessible
map-unavailable message rather than breaking the rest of the catalog.
