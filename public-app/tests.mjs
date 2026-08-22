import assert from "node:assert/strict";
import { once } from "node:events";
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import test from "node:test";
import { brotliDecompressSync, gunzipSync } from "node:zlib";

import {
  CATALOG_FORMAT,
  CATALOG_SCHEMA_VERSION,
  MAX_QUERY_LENGTH,
  isOfferActiveAt,
  normalizeSearchParams,
  normalizeSearchQuery,
  normalizeSlug,
  parseRoute,
  routeHref,
  validateManifest,
  validateWorkerRequest,
  validateWorkerResponse,
} from "./app-core.mjs";

const DIGEST = "0123456789abcdef".repeat(4);
const RELEASE_DIGEST = "fedcba9876543210".repeat(4);

function validManifest() {
  return {
    format: CATALOG_FORMAT,
    schema_version: CATALOG_SCHEMA_VERSION,
    database: {
      path: `data/catalog-${DIGEST}.sqlite3`,
      sha256: `sha256:${DIGEST}`,
      bytes: 4096,
    },
    generated_at: "2026-08-21T10:30:00Z",
    release_id: `sha256:${RELEASE_DIGEST}`,
    mineral_count: 12,
  };
}

test("hash routes are canonical and take precedence over clean paths", () => {
  const route = parseRoute("https://catalog.example/about#/minerals/quartz?q=smoky&page=2&page_size=12");
  assert.deepEqual(route, {
    name: "mineral",
    path: "/minerals/quartz",
    slug: "quartz",
    source: "hash",
    search: { query: "smoky", page: 2, pageSize: 12 },
  });
  assert.equal(routeHref("/minerals/quartz", { q: "smoky", page: "2" }), "#/minerals/quartz?q=smoky&page=2");
});

test("clean pathname and encoded query fallbacks remain parseable", () => {
  assert.equal(parseRoute("https://catalog.example/minerals/hematite").name, "mineral");
  const fallback = parseRoute("https://catalog.example/?route=%2Fminerals%3Fq%3Diron%26page%3D3");
  assert.equal(fallback.source, "query");
  assert.equal(fallback.name, "minerals");
  assert.deepEqual(fallback.search, { query: "iron", page: 3, pageSize: 24 });
});

test("invalid and encoded-separator slugs cannot become detail routes", () => {
  assert.equal(parseRoute("https://catalog.example/#/minerals/not%2Fa-slug").name, "not-found");
  assert.equal(parseRoute("https://catalog.example/#/minerals/%3Cscript%3E").name, "not-found");
  assert.equal(normalizeSlug("Quartz"), "quartz");
  assert.equal(normalizeSlug("../quartz"), null);
});

test("search input is Unicode-normalized, whitespace-collapsed, and bounded", () => {
  assert.equal(normalizeSearchQuery("  smoky\n\t quartz  "), "smoky quartz");
  assert.equal([...normalizeSearchQuery("界".repeat(MAX_QUERY_LENGTH + 50))].length, MAX_QUERY_LENGTH);
  assert.deepEqual(normalizeSearchParams("?q=iron&page=-2&page_size=999"), {
    query: "iron",
    page: 1,
    pageSize: 50,
  });
  assert.deepEqual(normalizeSearchParams({ query: " calcite ", page: 4, pageSize: 8 }), {
    query: "calcite",
    page: 4,
    pageSize: 8,
  });
});

test("the exact manifest contract resolves an independent release digest", () => {
  const manifest = validateManifest(validManifest(), "https://catalog.example/releases/catalog-manifest.json");
  assert.equal(manifest.release_id, `sha256:${RELEASE_DIGEST}`);
  assert.equal(manifest.database.digest, DIGEST);
  assert.equal(manifest.database.url, `https://catalog.example/releases/data/catalog-${DIGEST}.sqlite3`);
});

test("manifest validation fails closed on hash encoding, content path, and extra keys", () => {
  const bareHash = validManifest();
  bareHash.database.sha256 = DIGEST;
  assert.throws(() => validateManifest(bareHash), /sha256:/);

  const wrongPath = validManifest();
  wrongPath.database.path = `catalog-${DIGEST}.sqlite3`;
  assert.throws(() => validateManifest(wrongPath), /content-addressed/);

  const extra = validManifest();
  extra.debug = true;
  assert.throws(() => validateManifest(extra), /invalid shape/);
});

test("worker requests are shape-checked and normalized", () => {
  assert.deepEqual(validateWorkerRequest({
    id: 7,
    type: "search",
    payload: { query: "  SiO2 ", page: "2", pageSize: 500 },
  }), {
    id: 7,
    type: "search",
    payload: { query: "SiO2", page: 2, pageSize: 50 },
  });
  assert.throws(() => validateWorkerRequest({ id: 8, type: "detail", payload: { slug: "../secret" } }), /slug/);
  assert.throws(() => validateWorkerRequest({ id: 9, type: "sql", payload: {} }), /unsupported/);
  assert.throws(() => validateWorkerRequest({ id: 10, type: "offers", payload: { slug: "quartz", extra: true } }), /shape/);
});

test("worker responses reject mismatched protocol shapes", () => {
  const success = { id: 1, type: "detail", ok: true, result: null };
  assert.equal(validateWorkerResponse(success), success);
  const failure = { id: 2, type: "init", ok: false, error: { code: "INVALID_SCHEMA", message: "Rejected." } };
  assert.equal(validateWorkerResponse(failure), failure);
  assert.throws(() => validateWorkerResponse({ ...success, debug: true }), /shape/);
  assert.throws(() => validateWorkerResponse({ id: 2, type: "init", ok: false, error: { code: "bad", message: "No" } }), /code/);
});

test("offer expiry follows registry semantics and fails closed for malformed dates", () => {
  const now = Date.parse("2026-08-21T12:00:00Z");
  assert.equal(isOfferActiveAt(null, now), true);
  assert.equal(isOfferActiveAt("", now), true);
  assert.equal(isOfferActiveAt("2026-08-21T12:00:01Z", now), true);
  assert.equal(isOfferActiveAt("2026-08-21T12:00:00Z", now), false);
  assert.equal(isOfferActiveAt("2026-08-21 13:00:00", now), false);
  assert.equal(isOfferActiveAt("not-a-date", now), false);
});

test("the shell boots from deep clean routes and app-owned code avoids HTML sinks", async () => {
  const [index, app, worker] = await Promise.all([
    readFile(new URL("./index.html", import.meta.url), "utf8"),
    readFile(new URL("./app.js", import.meta.url), "utf8"),
    readFile(new URL("./catalog-worker.js", import.meta.url), "utf8"),
  ]);
  assert.match(index, /href="\/app\.css"/);
  assert.match(index, /src="\/app\.js"/);
  assert.match(index, /href="#\/minerals"/);
  assert.doesNotMatch(app, /\b(?:innerHTML|outerHTML|insertAdjacentHTML)\b/);
  assert.match(app, /data-catalog-map-preview/);
  assert.match(app, /Forest context only — mineral locations are not included yet\./);
  assert.match(app, /mountConfiguredMap/);
  assert.match(worker, /import\("\.\/vendor\/sqlite\/index\.mjs"\)/);
  assert.match(worker, /sqlite3_deserialize/);
});

test("optional exported snapshot passes the real worker integrity, schema, and query pipeline", {
  skip: !process.env.WAAJACU_CATALOG_SMOKE_DIR,
}, async () => {
  const { join } = await import("node:path");
  const snapshotDirectory = process.env.WAAJACU_CATALOG_SMOKE_DIR;
  const manifestText = await readFile(join(snapshotDirectory, "catalog-manifest.json"), "utf8");
  const rawManifest = JSON.parse(manifestText);
  const databaseFile = await readFile(join(snapshotDirectory, rawManifest.database.path));
  const brotliFile = await readFile(join(snapshotDirectory, `${rawManifest.database.path}.br`));
  const gzipFile = await readFile(join(snapshotDirectory, `${rawManifest.database.path}.gz`));
  assert.equal(brotliFile.byteLength < databaseFile.byteLength, true);
  assert.equal(gzipFile.byteLength < databaseFile.byteLength, true);
  assert.deepEqual(brotliDecompressSync(brotliFile), databaseFile);
  assert.deepEqual(gunzipSync(gzipFile), databaseFile);
  const wasmBytes = new Uint8Array(await readFile(new URL("./vendor/sqlite/sqlite3.wasm", import.meta.url)));
  const server = createServer((request, response) => {
    const url = new URL(request.url, "http://127.0.0.1");
    const [, mode, ...segments] = url.pathname.split("/");
    const resource = segments.join("/");
    if (resource === "catalog-manifest.json") {
      response.writeHead(200, {
        "Content-Type": "application/json",
        "Content-Length": Buffer.byteLength(manifestText),
        "Cache-Control": "no-cache",
      });
      response.end(manifestText);
      return;
    }
    if (resource === rawManifest.database.path) {
      const representations = {
        br: { bytes: brotliFile, encoding: "br" },
        gzip: { bytes: gzipFile, encoding: "gzip" },
        identity: { bytes: databaseFile, encoding: null },
      };
      const representation = representations[mode];
      if (!representation) {
        response.writeHead(404).end();
        return;
      }
      const headers = {
        "Content-Type": "application/vnd.sqlite3",
        "Content-Length": representation.bytes.byteLength,
        "Cache-Control": "public, max-age=31536000, immutable",
        Vary: "Accept-Encoding",
      };
      if (representation.encoding) headers["Content-Encoding"] = representation.encoding;
      response.writeHead(200, headers);
      response.end(representation.bytes);
      return;
    }
    if (resource === "vendor/sqlite/sqlite3.wasm") {
      response.writeHead(200, {
        "Content-Type": "application/wasm",
        "Content-Length": wasmBytes.byteLength,
      });
      response.end(wasmBytes);
      return;
    }
    response.writeHead(404).end();
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  assert.notEqual(address, null);
  const origin = `http://127.0.0.1:${address.port}`;
  let onMessage;
  const responses = new Map();
  const originalSelf = globalThis.self;
  const originalLocation = globalThis.location;
  const originalFetch = globalThis.fetch;
  const workerLocation = new URL(`${origin}/br/catalog-worker.js`);
  globalThis.location = workerLocation;
  globalThis.self = {
    location: workerLocation,
    addEventListener(type, listener) {
      if (type === "message") onMessage = listener;
    },
    postMessage(message) {
      responses.get(message.id)?.(message);
    },
  };

  let nextId = 1;
  const request = (type, payload) => new Promise((resolve) => {
    const id = nextId++;
    responses.set(id, (message) => {
      responses.delete(id);
      resolve(validateWorkerResponse(message));
    });
    onMessage({ data: { id, type, payload } });
  });
  try {
    for (const [mode, encoding, encodedBytes] of [
      ["br", "br", brotliFile],
      ["gzip", "gzip", gzipFile],
      ["identity", null, databaseFile],
    ]) {
      const response = await fetch(`${origin}/${mode}/${rawManifest.database.path}`);
      assert.equal(response.ok, true);
      assert.equal(response.headers.get("content-encoding"), encoding);
      assert.equal(response.headers.get("content-length"), String(encodedBytes.byteLength));
      assert.equal(response.headers.get("vary"), "Accept-Encoding");
      assert.deepEqual(Buffer.from(await response.arrayBuffer()), databaseFile);
    }

    await import(`./catalog-worker.js?smoke=${Date.now()}`);
    assert.equal(typeof onMessage, "function");
    const initialized = await request("init", { manifestUrl: `${origin}/br/catalog-manifest.json` });
    assert.equal(initialized.ok, true, initialized.error?.message);
    assert.equal(initialized.result.manifest.mineral_count, rawManifest.mineral_count);

    const search = await request("search", { query: "quartz", page: 1, pageSize: 5 });
    assert.equal(search.ok, true, search.error?.message);
    assert.equal(search.result.items.length > 0, true);
    assert.equal(search.result.items.length <= 5, true);
    const slug = search.result.items[0].slug;
    const detail = await request("detail", { slug });
    const evidence = await request("evidence", { slug });
    const offers = await request("offers", { slug });
    assert.equal(detail.ok, true, detail.error?.message);
    assert.equal(detail.result.slug, slug);
    assert.equal(evidence.ok, true, evidence.error?.message);
    assert.equal(Array.isArray(evidence.result.items), true);
    assert.equal(offers.ok, true, offers.error?.message);
    assert.equal(Array.isArray(offers.result.items), true);
  } finally {
    globalThis.fetch = originalFetch;
    globalThis.self = originalSelf;
    globalThis.location = originalLocation;
    server.close();
    await once(server, "close");
  }
});
