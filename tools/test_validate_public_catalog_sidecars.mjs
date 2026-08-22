import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { brotliCompressSync, gzipSync } from "node:zlib";

import { validateCatalogSidecars } from "./validate-public-catalog-sidecars.mjs";

async function fixture() {
  const root = await mkdtemp(path.join(os.tmpdir(), "waajacu-catalog-sidecars-"));
  const data = path.join(root, "data");
  await mkdir(data);
  const raw = Buffer.from("sanitized public sqlite test fixture");
  const digest = createHash("sha256").update(raw).digest("hex");
  const relative = `data/catalog-${digest}.sqlite3`;
  await writeFile(path.join(root, relative), raw);
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
