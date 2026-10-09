import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { brotliCompressSync, gzipSync } from "node:zlib";

import { validateCatalogSidecars } from "./validate-public-catalog-sidecars.mjs";

async function fixture({ includeRaw = true } = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "waajacu-catalog-sidecars-"));
  const data = path.join(root, "data");
  await mkdir(data);
  const raw = Buffer.from("sanitized public sqlite test fixture");
  const digest = createHash("sha256").update(raw).digest("hex");
  const relative = `data/catalog-${digest}.sqlite3`;
  if (includeRaw) await writeFile(path.join(root, relative), raw);
  await writeFile(path.join(root, `${relative}.br`), brotliCompressSync(raw));
  await writeFile(path.join(root, `${relative}.gz`), gzipSync(raw));
  await writeFile(
    path.join(root, "catalog-manifest.json"),
    JSON.stringify({
      database: {
        path: relative,
        sha256: `sha256:${digest}`,
        bytes: raw.length,
      },
    }),
  );
  return { root, raw, relative };
}

test("accepts only sidecars that decode to the manifest-addressed raw database", async (context) => {
  const current = await fixture();
  context.after(() => rm(current.root, { recursive: true, force: true }));
  const result = await validateCatalogSidecars(current.root);
  assert.equal(result.bytes, current.raw.length);
});

test("accepts a compressed-only source when both sidecars reconstruct the exact manifest database", async (context) => {
  const current = await fixture({ includeRaw: false });
  context.after(() => rm(current.root, { recursive: true, force: true }));
  const result = await validateCatalogSidecars(current.root);
  assert.equal(result.bytes, current.raw.length);
  assert.equal(result.sha256, createHash("sha256").update(current.raw).digest("hex"));
});

test("compressed-only sources require both sidecars and reject unexpected files", async (context) => {
  for (const missing of [".br", ".gz"]) {
    const current = await fixture({ includeRaw: false });
    context.after(() => rm(current.root, { recursive: true, force: true }));
    await rm(path.join(current.root, `${current.relative}${missing}`));
    await assert.rejects(validateCatalogSidecars(current.root), /unexpected or missing entry/);
  }
  const extra = await fixture({ includeRaw: false });
  context.after(() => rm(extra.root, { recursive: true, force: true }));
  await writeFile(path.join(extra.root, "data", "private-backup.sqlite3"), "private");
  await assert.rejects(validateCatalogSidecars(extra.root), /unexpected or missing entry/);
});

test("matching compressed streams cannot substitute another database under the manifest hash", async (context) => {
  const current = await fixture({ includeRaw: false });
  context.after(() => rm(current.root, { recursive: true, force: true }));
  const different = Buffer.alloc(current.raw.length, 97);
  await writeFile(path.join(current.root, `${current.relative}.br`), brotliCompressSync(different));
  await writeFile(path.join(current.root, `${current.relative}.gz`), gzipSync(different));
  await assert.rejects(validateCatalogSidecars(current.root), /SHA-256 does not match/);
});

test("compressed-only sources reject different Brotli and gzip database bytes", async (context) => {
  const current = await fixture({ includeRaw: false });
  context.after(() => rm(current.root, { recursive: true, force: true }));
  await writeFile(path.join(current.root, `${current.relative}.br`), brotliCompressSync(Buffer.from("other data")));
  await assert.rejects(validateCatalogSidecars(current.root), /does not decode to the raw public database/);
});

test("decoded output is bounded by the declared database size", async (context) => {
  const current = await fixture({ includeRaw: false });
  context.after(() => rm(current.root, { recursive: true, force: true }));
  await writeFile(path.join(current.root, `${current.relative}.br`), brotliCompressSync(Buffer.alloc(current.raw.length + 1024)));
  await assert.rejects(validateCatalogSidecars(current.root), /cannot be decoded safely/);
});

test("rejects private trailing bytes after compressed-only gzip input", async (context) => {
  const current = await fixture({ includeRaw: false });
  context.after(() => rm(current.root, { recursive: true, force: true }));
  await writeFile(path.join(current.root, `${current.relative}.gz`), Buffer.concat([gzipSync(current.raw), Buffer.from("PRIVATE-TRAILER")]));
  await assert.rejects(validateCatalogSidecars(current.root), /gzip catalog sidecar cannot be decoded safely/);
});

test("rejects an opaque compressed file that is not the sanitized database", async (context) => {
  const current = await fixture();
  context.after(() => rm(current.root, { recursive: true, force: true }));
  await writeFile(
    path.join(current.root, `${current.relative}.br`),
    brotliCompressSync(Buffer.from("private backup contents")),
  );
  await assert.rejects(
    validateCatalogSidecars(current.root),
    /does not decode to the raw public database/,
  );
});

test("rejects private bytes appended after a valid Brotli stream", async (context) => {
  const current = await fixture();
  context.after(() => rm(current.root, { recursive: true, force: true }));
  const sidecar = path.join(current.root, `${current.relative}.br`);
  await writeFile(
    sidecar,
    Buffer.concat([brotliCompressSync(current.raw), Buffer.from("PRIVATE-TRAILER")]),
  );
  await assert.rejects(
    validateCatalogSidecars(current.root),
    /Brotli catalog sidecar cannot be decoded safely/,
  );
});
