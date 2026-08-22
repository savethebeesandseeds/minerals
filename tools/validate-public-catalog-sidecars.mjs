#!/usr/bin/env node

import { createHash } from "node:crypto";
import { lstat, readFile, readdir } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";
import { brotliDecompressSync, gunzipSync } from "node:zlib";

const MAX_DATABASE_BYTES = 512 * 1024 * 1024;
const MAX_MANIFEST_BYTES = 64 * 1024;

function fail(message) {
  throw new Error(message);
}

async function requireRegularFile(root, relative, maximumBytes) {
  const segments = relative.split("/");
  if (
    path.isAbsolute(relative) ||
    segments.length === 0 ||
    segments.some((segment) => !segment || segment === "." || segment === "..")
  ) {
    fail(`unsafe catalog path: ${relative}`);
  }
  const candidate = path.join(root, ...segments);
  const metadata = await lstat(candidate);
  if (metadata.isSymbolicLink() || !metadata.isFile()) {
    fail(`catalog artifact must be a regular non-symlink file: ${relative}`);
  }
  if (metadata.size <= 0 || metadata.size > maximumBytes) {
    fail(`catalog artifact has an invalid size: ${relative}`);
  }
  return candidate;
}

function exactNames(actual, expected, label) {
  const left = [...actual].sort();
  const right = [...expected].sort();
  if (left.length !== right.length || left.some((value, index) => value !== right[index])) {
    fail(`${label} contains an unexpected or missing entry`);
  }
}

export async function validateCatalogSidecars(catalogRoot) {
  const rootMetadata = await lstat(catalogRoot);
  if (rootMetadata.isSymbolicLink() || !rootMetadata.isDirectory()) {
    fail("public catalog root must be a real directory");
  }
  exactNames(
    (await readdir(catalogRoot)).map(String),
    ["catalog-manifest.json", "data"],
    "public catalog root",
  );

  const manifestPath = await requireRegularFile(
    catalogRoot,
    "catalog-manifest.json",
    MAX_MANIFEST_BYTES,
  );
  let manifest;
  try {
    manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  } catch {
    fail("public catalog manifest is not valid JSON");
  }
  const databasePath = manifest?.database?.path;
  const digest = manifest?.database?.sha256;
  const expectedBytes = manifest?.database?.bytes;
  const match =
    typeof databasePath === "string"
      ? /^data\/catalog-([0-9a-f]{64})\.sqlite3$/.exec(databasePath)
      : null;
  if (!match || digest !== `sha256:${match[1]}`) {
    fail("public catalog manifest database identity is invalid");
  }
  if (!Number.isSafeInteger(expectedBytes) || expectedBytes <= 0 || expectedBytes > MAX_DATABASE_BYTES) {
    fail("public catalog manifest database size is invalid");
  }

  const rawPath = await requireRegularFile(catalogRoot, databasePath, MAX_DATABASE_BYTES);
  const brotliPath = await requireRegularFile(
    catalogRoot,
    `${databasePath}.br`,
    MAX_DATABASE_BYTES,
  );
  const gzipPath = await requireRegularFile(
    catalogRoot,
    `${databasePath}.gz`,
    MAX_DATABASE_BYTES,
  );
  const dataDirectory = path.join(catalogRoot, "data");
  const dataMetadata = await lstat(dataDirectory);
  if (dataMetadata.isSymbolicLink() || !dataMetadata.isDirectory()) {
    fail("public catalog data must be a real directory");
  }
  exactNames(
    (await readdir(dataDirectory)).map(String),
    [path.basename(rawPath), path.basename(brotliPath), path.basename(gzipPath)],
    "public catalog data",
  );

  const raw = await readFile(rawPath);
  if (raw.length !== expectedBytes) {
    fail("raw public catalog size does not match the manifest");
  }
  if (createHash("sha256").update(raw).digest("hex") !== match[1]) {
    fail("raw public catalog SHA-256 does not match the manifest");
  }

  let brotli;
  let gzip;
  try {
    const encoded = await readFile(brotliPath);
    const decoded = brotliDecompressSync(encoded, {
      info: true,
      maxOutputLength: expectedBytes + 1,
    });
    if (decoded.engine.bytesWritten !== encoded.length) {
      fail("Brotli catalog sidecar has trailing compressed input");
    }
    brotli = decoded.buffer;
  } catch {
    fail("Brotli catalog sidecar cannot be decoded safely");
  }
  try {
    const encoded = await readFile(gzipPath);
    const decoded = gunzipSync(encoded, {
      info: true,
      maxOutputLength: expectedBytes + 1,
    });
    if (decoded.engine.bytesWritten !== encoded.length) {
      fail("gzip catalog sidecar has trailing compressed input");
    }
    gzip = decoded.buffer;
  } catch {
    fail("gzip catalog sidecar cannot be decoded safely");
  }
  if (!brotli.equals(raw)) {
    fail("Brotli catalog sidecar does not decode to the raw public database");
  }
  if (!gzip.equals(raw)) {
    fail("gzip catalog sidecar does not decode to the raw public database");
  }
  return {
    bytes: raw.length,
    sha256: match[1],
  };
}

async function main() {
  if (process.argv.length !== 3) {
    fail("usage: validate-public-catalog-sidecars.mjs PUBLIC_CATALOG_ROOT");
  }
  const result = await validateCatalogSidecars(path.resolve(process.argv[2]));
  process.stdout.write(
    `Validated public catalog sidecars: ${result.bytes} bytes sha256:${result.sha256}\n`,
  );
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    process.stderr.write(`public catalog sidecar validation failed: ${error.message}\n`);
    process.exitCode = 1;
  });
}
