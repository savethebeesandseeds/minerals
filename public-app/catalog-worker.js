import {
  CATALOG_FORMAT,
  CATALOG_SCHEMA_VERSION,
  validateManifest,
  validateWorkerRequest,
} from "./app-core.mjs";

const MAX_MANIFEST_BYTES = 64 * 1024;
const MAX_DATABASE_BYTES = 512 * 1024 * 1024;
const REQUIRED_COLUMNS = Object.freeze({
  catalog_meta: ["key", "value"],
  minerals: [
    "slug", "public_id", "canonical_name", "formula", "description", "mineral_family",
    "nomenclature_status", "verification_status", "data_quality_score", "source_kind",
    "license_spdx", "cas_number", "identifiers_json", "properties_json", "safety_json",
    "discovery_country", "first_reference", "second_reference", "source_status",
    "evidence_count", "active_offer_count",
  ],
  evidence: [
    "mineral_slug", "position", "title", "publisher", "canonical_url", "license_spdx",
    "claim_scope", "claim_json", "confidence", "review_status", "retrieved_at", "content_hash",
    "attribution_party", "work_title", "work_url", "license_url", "changes_notice",
    "no_endorsement_notice", "derived_output_license_spdx",
  ],
  offers: [
    "mineral_slug", "position", "provider_name", "provider_slug", "provider_verification_status",
    "provider_trust_score", "title", "product_url", "currency_code", "price_minor",
    "currency_exponent", "pricing_basis", "minimum_order_quantity", "minimum_order_unit",
    "stock_status", "purity_text", "grade", "origin_country_code", "verification_status",
    "last_checked_at", "expires_at",
  ],
  mineral_search: ["slug", "canonical_name", "formula", "mineral_family", "search_text"],
});

const NULLABLE_COLUMNS = Object.freeze({
  catalog_meta: new Set(),
  minerals: new Set(["cas_number"]),
  evidence: new Set([
    "attribution_party", "work_title", "work_url", "license_url", "changes_notice",
    "no_endorsement_notice", "derived_output_license_spdx",
  ]),
  offers: new Set(["price_minor", "minimum_order_quantity", "expires_at"]),
  mineral_search: new Set(REQUIRED_COLUMNS.mineral_search),
});

const INTEGER_COLUMNS = new Set([
  "minerals.evidence_count", "minerals.active_offer_count", "evidence.position",
  "offers.position", "offers.price_minor", "offers.currency_exponent",
]);
const REAL_COLUMNS = new Set([
  "minerals.data_quality_score", "evidence.confidence", "offers.provider_trust_score",
  "offers.minimum_order_quantity",
]);
const PRIMARY_KEY_POSITIONS = Object.freeze({
  catalog_meta: { key: 1 },
  minerals: { slug: 1 },
  evidence: { mineral_slug: 1, position: 2 },
  offers: { mineral_slug: 1, position: 2 },
  mineral_search: {},
});

const MINERAL_LIST_COLUMNS = `
  m.slug, m.public_id, m.canonical_name, m.formula,
  CASE WHEN length(m.description) > 360 THEN substr(m.description, 1, 357) || '…'
       ELSE m.description END AS description_excerpt,
  m.mineral_family, m.nomenclature_status, m.verification_status,
  m.data_quality_score, m.license_spdx, m.evidence_count, m.active_offer_count`;

const MINERAL_DETAIL_COLUMNS = `
  slug, public_id, canonical_name, formula, description, mineral_family,
  nomenclature_status, verification_status, data_quality_score, source_kind,
  license_spdx, cas_number, identifiers_json, properties_json, safety_json,
  discovery_country, first_reference, second_reference, source_status,
  evidence_count, active_offer_count`;

const EVIDENCE_COLUMNS = `
  mineral_slug, position, title, publisher, canonical_url, license_spdx,
  claim_scope, claim_json, confidence, review_status, retrieved_at, content_hash,
  attribution_party, work_title, work_url, license_url, changes_notice,
  no_endorsement_notice, derived_output_license_spdx`;

const OFFER_COLUMNS = `
  mineral_slug, position, provider_name, provider_slug, provider_verification_status,
  provider_trust_score, title, product_url, currency_code, price_minor,
  currency_exponent, pricing_basis, minimum_order_quantity, minimum_order_unit,
  stock_status, purity_text, grade, origin_country_code, verification_status,
  last_checked_at, expires_at`;

class CatalogError extends Error {
  constructor(code, message, options) {
    super(message, options);
    this.name = "CatalogError";
    this.code = code;
  }
}

let sqlite3;
let database;
let loadedManifest;
let loadedManifestUrl;
let requestQueue = Promise.resolve();

function catalogError(code, message, cause) {
  return new CatalogError(code, message, cause ? { cause } : undefined);
}

function safeError(error) {
  if (error instanceof CatalogError) {
    return { code: error.code, message: error.message };
  }
  console.error("Unexpected catalog worker failure:", error);
  return { code: "INTERNAL_ERROR", message: "The catalog worker could not complete the request." };
}

function databaseRows(sql, bind = []) {
  const rows = database.exec({ sql, bind, rowMode: "object", returnValue: "resultRows" });
  return rows.map((row) => {
    const result = {};
    for (const [key, value] of Object.entries(row)) {
      if (value === null || typeof value === "string" || (typeof value === "number" && Number.isFinite(value))) {
        result[key] = value;
      } else if (typeof value === "bigint") {
        result[key] = value >= Number.MIN_SAFE_INTEGER && value <= Number.MAX_SAFE_INTEGER ? Number(value) : String(value);
      } else {
        throw catalogError("INVALID_DATABASE", `Column ${key} has an unsupported value type.`);
      }
    }
    return result;
  });
}

function singleValue(sql, bind) {
  const value = database.selectValue(sql, bind);
  if (typeof value === "bigint") {
    if (value < Number.MIN_SAFE_INTEGER || value > Number.MAX_SAFE_INTEGER) {
      throw catalogError("INVALID_DATABASE", "A SQLite integer exceeds the browser's safe numeric range.");
    }
    return Number(value);
  }
  return value;
}

function assertExactColumns(table, expected) {
  const rows = databaseRows("SELECT name, type, \"notnull\" AS not_null, pk FROM pragma_table_info(?) ORDER BY cid", [table]);
  const actual = rows.map((row) => row.name);
  if (actual.length !== expected.length || actual.some((name, index) => name !== expected[index])) {
    throw catalogError("INVALID_SCHEMA", `${table} does not match the public catalog v1 column contract.`);
  }
  for (const row of rows) {
    const qualified = `${table}.${row.name}`;
    const expectedType = table === "mineral_search" ? "" : INTEGER_COLUMNS.has(qualified) ? "INTEGER" : REAL_COLUMNS.has(qualified) ? "REAL" : "TEXT";
    const expectedNotNull = NULLABLE_COLUMNS[table].has(row.name) || table === "mineral_search" ? 0 : 1;
    const expectedPrimaryKey = PRIMARY_KEY_POSITIONS[table][row.name] ?? 0;
    if (row.type !== expectedType || row.not_null !== expectedNotNull || row.pk !== expectedPrimaryKey) {
      throw catalogError("INVALID_SCHEMA", `${table}.${row.name} does not match the public catalog v1 type or constraint contract.`);
    }
  }
}

function assertUniquePublicId() {
  const indexes = databaseRows("SELECT name FROM pragma_index_list(?) WHERE \"unique\" = 1", ["minerals"]);
  const hasPublicIdIndex = indexes.some((index) => {
    if (typeof index.name !== "string") return false;
    const columns = databaseRows("SELECT name FROM pragma_index_info(?) ORDER BY seqno", [index.name]);
    return columns.length === 1 && columns[0].name === "public_id";
  });
  if (!hasPublicIdIndex) throw catalogError("INVALID_SCHEMA", "minerals.public_id must have a unique constraint.");
}

function assertForeignKey(table) {
  const keys = databaseRows("SELECT \"table\" AS target_table, \"from\" AS source_column, \"to\" AS target_column FROM pragma_foreign_key_list(?)", [table]);
  if (keys.length !== 1 || keys[0].target_table !== "minerals" || keys[0].source_column !== "mineral_slug" || keys[0].target_column !== "slug") {
    throw catalogError("INVALID_SCHEMA", `${table}.mineral_slug must reference minerals.slug.`);
  }
}

function validateDatabaseSchema(manifest) {
  database.exec("PRAGMA query_only = ON; PRAGMA trusted_schema = OFF;");

  const quickCheck = databaseRows("PRAGMA quick_check(1)");
  if (quickCheck.length !== 1 || Object.values(quickCheck[0])[0] !== "ok") {
    throw catalogError("INVALID_DATABASE", "SQLite quick_check did not accept the catalog database.");
  }

  const objectNames = Object.keys(REQUIRED_COLUMNS);
  const placeholders = objectNames.map(() => "?").join(", ");
  const objects = databaseRows(
    `SELECT name, type, sql FROM sqlite_schema WHERE name IN (${placeholders}) ORDER BY name`,
    objectNames,
  );
  if (objects.length !== objectNames.length || objects.some((item) => item.type !== "table")) {
    throw catalogError("INVALID_SCHEMA", "The catalog database is missing a required table.");
  }
  const searchDefinition = objects.find((item) => item.name === "mineral_search")?.sql;
  if (typeof searchDefinition !== "string"
    || !/\bUSING\s+fts5\s*\(/iu.test(searchDefinition)
    || !/\bslug\s+UNINDEXED\b/iu.test(searchDefinition)
    || !/tokenize\s*=\s*['"]unicode61\s+remove_diacritics\s+2['"]/iu.test(searchDefinition)) {
    throw catalogError("INVALID_SCHEMA", "mineral_search must be an FTS5 table using the v1 Unicode tokenizer.");
  }
  for (const [table, columns] of Object.entries(REQUIRED_COLUMNS)) assertExactColumns(table, columns);
  for (const table of ["catalog_meta", "minerals", "evidence", "offers"]) {
    const definition = objects.find((item) => item.name === table)?.sql;
    if (typeof definition !== "string" || !/\bWITHOUT\s+ROWID\s*;?\s*$/iu.test(definition)) {
      throw catalogError("INVALID_SCHEMA", `${table} must use WITHOUT ROWID.`);
    }
  }
  assertUniquePublicId();
  assertForeignKey("evidence");
  assertForeignKey("offers");

  const metadataRows = databaseRows("SELECT key, value FROM catalog_meta ORDER BY key");
  const expectedMetadata = {
    format: CATALOG_FORMAT,
    generated_at: manifest.generated_at,
    mineral_count: String(manifest.mineral_count),
    release_id: manifest.release_id,
    schema_version: String(CATALOG_SCHEMA_VERSION),
  };
  if (metadataRows.length !== Object.keys(expectedMetadata).length) {
    throw catalogError("INVALID_SCHEMA", "catalog_meta must contain exactly the five v1 metadata records.");
  }
  for (const row of metadataRows) {
    if (!Object.hasOwn(expectedMetadata, row.key) || row.value !== expectedMetadata[row.key]) {
      throw catalogError("MANIFEST_MISMATCH", `catalog_meta value ${String(row.key)} does not match the manifest.`);
    }
  }

  const mineralCount = singleValue("SELECT count(*) FROM minerals");
  const searchCount = singleValue("SELECT count(*) FROM mineral_search");
  if (mineralCount !== manifest.mineral_count || searchCount !== manifest.mineral_count) {
    throw catalogError("MANIFEST_MISMATCH", "Manifest, mineral table, and search index counts do not match.");
  }
  if (singleValue("SELECT count(*) FROM minerals WHERE json_valid(identifiers_json) <> 1 OR json_valid(properties_json) <> 1 OR json_valid(safety_json) <> 1") !== 0
    || singleValue("SELECT count(*) FROM evidence WHERE json_valid(claim_json) <> 1") !== 0) {
    throw catalogError("INVALID_DATABASE", "A catalog JSON column contains invalid JSON.");
  }
  if (singleValue("SELECT count(*) FROM minerals AS m LEFT JOIN mineral_search AS s ON s.slug = m.slug WHERE s.slug IS NULL") !== 0
    || singleValue("SELECT count(*) FROM mineral_search AS s LEFT JOIN minerals AS m ON m.slug = s.slug WHERE m.slug IS NULL") !== 0) {
    throw catalogError("INVALID_SCHEMA", "The mineral search index does not match the mineral table.");
  }
  if (singleValue("SELECT count(*) FROM evidence AS e LEFT JOIN minerals AS m ON m.slug = e.mineral_slug WHERE m.slug IS NULL") !== 0
    || singleValue("SELECT count(*) FROM offers AS o LEFT JOIN minerals AS m ON m.slug = o.mineral_slug WHERE m.slug IS NULL") !== 0) {
    throw catalogError("INVALID_SCHEMA", "Evidence or offers refer to a mineral not present in the catalog.");
  }
}

async function sha256Hex(bytes) {
  if (!globalThis.crypto?.subtle) {
    throw catalogError("CRYPTO_UNAVAILABLE", "Web Crypto SHA-256 is unavailable; serve the app from HTTPS or literal localhost.");
  }
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

function sameOriginUrl(rawUrl, baseUrl, label) {
  let url;
  try {
    url = new URL(rawUrl, baseUrl);
  } catch (error) {
    throw catalogError("INVALID_URL", `${label} is not a valid URL.`, error);
  }
  const base = new URL(baseUrl);
  if (url.origin !== base.origin || url.username || url.password || url.hash || !["http:", "https:"].includes(url.protocol)) {
    throw catalogError("INVALID_URL", `${label} must be an uncredentialed same-origin HTTP(S) URL without a fragment.`);
  }
  return url;
}

async function fetchManifest(manifestUrl) {
  const response = await fetch(manifestUrl, {
    cache: "no-store",
    credentials: "same-origin",
    redirect: "error",
    headers: { Accept: "application/json" },
  }).catch((error) => {
    throw catalogError("MANIFEST_FETCH_FAILED", "The catalog manifest could not be fetched.", error);
  });
  if (!response.ok) throw catalogError("MANIFEST_FETCH_FAILED", `The catalog manifest returned HTTP ${response.status}.`);
  const declaredLength = Number(response.headers.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > MAX_MANIFEST_BYTES) {
    throw catalogError("INVALID_MANIFEST", "The catalog manifest is too large.");
  }
  const text = await response.text();
  if (new TextEncoder().encode(text).byteLength > MAX_MANIFEST_BYTES) {
    throw catalogError("INVALID_MANIFEST", "The catalog manifest is too large.");
  }
  let raw;
  try {
    raw = JSON.parse(text);
  } catch (error) {
    throw catalogError("INVALID_MANIFEST", "The catalog manifest is not valid JSON.", error);
  }
  try {
    return validateManifest(raw, manifestUrl.href);
  } catch (error) {
    throw catalogError("INVALID_MANIFEST", error instanceof Error ? error.message : "The catalog manifest is invalid.", error);
  }
}

async function fetchDatabase(manifest) {
  if (manifest.database.bytes > MAX_DATABASE_BYTES) {
    throw catalogError("DATABASE_TOO_LARGE", `This app accepts catalog databases up to ${MAX_DATABASE_BYTES} bytes.`);
  }
  const response = await fetch(manifest.database.url, {
    cache: "force-cache",
    credentials: "same-origin",
    redirect: "error",
    headers: { Accept: "application/vnd.sqlite3, application/octet-stream;q=0.9" },
  }).catch((error) => {
    throw catalogError("DATABASE_FETCH_FAILED", "The catalog database could not be fetched.", error);
  });
  if (!response.ok) throw catalogError("DATABASE_FETCH_FAILED", `The catalog database returned HTTP ${response.status}.`);
  const declaredLength = Number(response.headers.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > MAX_DATABASE_BYTES) {
    throw catalogError("DATABASE_TOO_LARGE", "The catalog database response is too large.");
  }
  const bytes = await response.arrayBuffer();
  if (bytes.byteLength !== manifest.database.bytes) {
    throw catalogError("DATABASE_SIZE_MISMATCH", "The downloaded catalog database size does not match the signed manifest value.");
  }
  const digest = await sha256Hex(bytes);
  if (digest !== manifest.database.digest) {
    throw catalogError("DATABASE_HASH_MISMATCH", "The downloaded catalog database failed SHA-256 verification.");
  }
  return new Uint8Array(bytes);
}

async function initializeSqlite() {
  if (sqlite3) return sqlite3;
  let initializer;
  try {
    ({ default: initializer } = await import("./vendor/sqlite/index.mjs"));
  } catch (error) {
    throw catalogError("SQLITE_LOAD_FAILED", "The vendored SQLite WASM module could not be loaded.", error);
  }
  if (typeof initializer !== "function") {
    throw catalogError("SQLITE_LOAD_FAILED", "The vendored SQLite module has no initializer.");
  }
  const previousApiConfig = globalThis.sqlite3ApiConfig;
  globalThis.sqlite3ApiConfig = {
    disable: {
      vfs: { opfs: true, "opfs-vfs": true, "opfs-wl": true, "opfs-sahpool": true },
    },
  };
  try {
    sqlite3 = await initializer({
      locateFile: (filename) => new URL(`./vendor/sqlite/${filename}`, self.location.href).href,
      print: () => {},
      printErr: (message) => console.warn("SQLite WASM:", message),
    });
  } catch (error) {
    throw catalogError("SQLITE_LOAD_FAILED", "SQLite WASM could not be initialized.", error);
  } finally {
    if (previousApiConfig === undefined) delete globalThis.sqlite3ApiConfig;
    else globalThis.sqlite3ApiConfig = previousApiConfig;
  }
  return sqlite3;
}

async function deserializeDatabase(bytes) {
  const api = await initializeSqlite();
  const db = new api.oo1.DB(":memory:", "c");
  const pointer = api.wasm.allocFromTypedArray(bytes);
  let transferred = false;
  try {
    const freeOnClose = api.capi.SQLITE_DESERIALIZE_FREEONCLOSE ?? 1;
    const readOnly = api.capi.SQLITE_DESERIALIZE_READONLY ?? 4;
    const result = api.capi.sqlite3_deserialize(
      db.pointer,
      "main",
      pointer,
      bytes.byteLength,
      bytes.byteLength,
      freeOnClose | readOnly,
    );
    db.checkRc(result);
    transferred = true;
    return db;
  } catch (error) {
    db.close();
    throw catalogError("DATABASE_OPEN_FAILED", "SQLite could not deserialize the verified catalog in read-only mode.", error);
  } finally {
    if (!transferred) api.wasm.dealloc(pointer);
  }
}

async function initializeCatalog(manifestUrlText) {
  const manifestUrl = sameOriginUrl(manifestUrlText, self.location.href, "manifestUrl");
  if (loadedManifest) {
    if (manifestUrl.href !== loadedManifestUrl) throw catalogError("ALREADY_INITIALIZED", "The worker is already initialized with another manifest.");
    return publicManifestResult();
  }

  const manifest = await fetchManifest(manifestUrl);
  const bytes = await fetchDatabase(manifest);
  const candidate = await deserializeDatabase(bytes);
  database = candidate;
  try {
    validateDatabaseSchema(manifest);
  } catch (error) {
    database.close();
    database = undefined;
    throw error;
  }
  loadedManifest = manifest;
  loadedManifestUrl = manifestUrl.href;
  return publicManifestResult();
}

function publicManifestResult() {
  return {
    manifest: {
      format: loadedManifest.format,
      schema_version: loadedManifest.schema_version,
      generated_at: loadedManifest.generated_at,
      release_id: loadedManifest.release_id,
      mineral_count: loadedManifest.mineral_count,
      database: {
        path: loadedManifest.database.path,
        sha256: loadedManifest.database.sha256,
        bytes: loadedManifest.database.bytes,
      },
    },
  };
}

function requireDatabase() {
  if (!database || !loadedManifest) throw catalogError("NOT_INITIALIZED", "Initialize the catalog before querying it.");
}

function ftsExpression(query) {
  const tokens = query.match(/[\p{L}\p{N}][\p{L}\p{N}\p{M}._-]*/gu)?.slice(0, 12) ?? [];
  return tokens.length ? tokens.map((token) => `"${token.replaceAll('"', '""')}"*`).join(" AND ") : null;
}

function searchCatalog({ query, page, pageSize }) {
  requireDatabase();
  const offset = (page - 1) * pageSize;
  const expression = ftsExpression(query);
  let total;
  let items;
  if (expression) {
    total = singleValue("SELECT count(*) FROM mineral_search WHERE mineral_search MATCH ?", [expression]);
    items = databaseRows(
      `SELECT ${MINERAL_LIST_COLUMNS}
         FROM mineral_search
         JOIN minerals AS m ON m.slug = mineral_search.slug
        WHERE mineral_search MATCH ?
        ORDER BY bm25(mineral_search, 0.0, 10.0, 8.0, 3.0, 1.0),
                 m.canonical_name COLLATE NOCASE, m.slug
        LIMIT ? OFFSET ?`,
      [expression, pageSize, offset],
    );
  } else {
    total = singleValue("SELECT count(*) FROM minerals");
    items = databaseRows(
      `SELECT ${MINERAL_LIST_COLUMNS}
         FROM minerals AS m
        ORDER BY m.canonical_name COLLATE NOCASE, m.slug
        LIMIT ? OFFSET ?`,
      [pageSize, offset],
    );
  }
  if (!Number.isSafeInteger(total) || total < 0) throw catalogError("INVALID_DATABASE", "The search count is invalid.");
  return { items, total, page, page_size: pageSize, total_pages: total === 0 ? 0 : Math.ceil(total / pageSize), query };
}

function mineralDetail(slug) {
  requireDatabase();
  return databaseRows(`SELECT ${MINERAL_DETAIL_COLUMNS} FROM minerals WHERE slug = ? LIMIT 1`, [slug])[0] ?? null;
}

function mineralEvidence(slug) {
  requireDatabase();
  return {
    items: databaseRows(
      `SELECT ${EVIDENCE_COLUMNS} FROM evidence WHERE mineral_slug = ? ORDER BY position, canonical_url`,
      [slug],
    ),
  };
}

function mineralOffers(slug) {
  requireDatabase();
  return {
    items: databaseRows(
      `SELECT ${OFFER_COLUMNS} FROM offers WHERE mineral_slug = ? ORDER BY position, provider_name, product_url`,
      [slug],
    ),
  };
}

async function executeRequest(request) {
  switch (request.type) {
    case "init": return initializeCatalog(request.payload.manifestUrl);
    case "search": return searchCatalog(request.payload);
    case "detail": return mineralDetail(request.payload.slug);
    case "evidence": return mineralEvidence(request.payload.slug);
    case "offers": return mineralOffers(request.payload.slug);
    default: throw catalogError("INVALID_REQUEST", "Unsupported catalog operation.");
  }
}

self.addEventListener("message", (event) => {
  const raw = event.data;
  requestQueue = requestQueue.then(async () => {
    let request;
    try {
      request = validateWorkerRequest(raw);
      const result = await executeRequest(request);
      self.postMessage({ id: request.id, type: request.type, ok: true, result });
    } catch (error) {
      const id = Number.isSafeInteger(raw?.id) && raw.id > 0 ? raw.id : 1;
      const type = typeof raw?.type === "string" && ["init", "search", "detail", "evidence", "offers"].includes(raw.type)
        ? raw.type
        : "init";
      const failure = error instanceof TypeError
        ? { code: "INVALID_REQUEST", message: error.message }
        : safeError(error);
      self.postMessage({ id, type, ok: false, error: failure });
    }
  });
});
