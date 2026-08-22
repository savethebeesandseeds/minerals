export const CATALOG_FORMAT = "waajacu-public-catalog-v1";
export const CATALOG_SCHEMA_VERSION = 1;
export const DEFAULT_PAGE_SIZE = 24;
export const MAX_PAGE_SIZE = 50;
export const MAX_QUERY_LENGTH = 160;
export const MAX_SLUG_LENGTH = 120;

const REQUEST_TYPES = new Set(["init", "search", "detail", "evidence", "offers"]);
const ROUTE_NAMES = new Set(["home", "minerals", "mineral", "about", "map", "not-found"]);
const SHA256_PATTERN = /^sha256:([0-9a-f]{64})$/;
const SLUG_PATTERN = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function ownKeysAre(value, expected) {
  if (!isRecord(value)) return false;
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index]);
}

function requireExactKeys(value, expected, label) {
  if (!ownKeysAre(value, expected)) {
    throw new TypeError(`${label} has an invalid shape`);
  }
}

function requireString(value, label, maximum = 2_048) {
  if (typeof value !== "string" || value.length === 0 || value.length > maximum) {
    throw new TypeError(`${label} must be a non-empty string of at most ${maximum} characters`);
  }
  return value;
}

function codePointSlice(value, maximum) {
  return [...value].slice(0, maximum).join("");
}

export function normalizeSearchQuery(value) {
  if (typeof value !== "string") return "";
  return codePointSlice(value.normalize("NFC").trim().replace(/\s+/gu, " "), MAX_QUERY_LENGTH);
}

export function normalizePage(value, fallback = 1) {
  const parsed = typeof value === "number" ? value : Number.parseInt(String(value ?? ""), 10);
  return Number.isSafeInteger(parsed) && parsed >= 1 ? Math.min(parsed, 1_000_000) : fallback;
}

export function normalizePageSize(value, fallback = DEFAULT_PAGE_SIZE) {
  const parsed = typeof value === "number" ? value : Number.parseInt(String(value ?? ""), 10);
  return Number.isSafeInteger(parsed) && parsed >= 1 ? Math.min(parsed, MAX_PAGE_SIZE) : fallback;
}

export function normalizeSearchParams(input) {
  let parameters;
  if (input instanceof URLSearchParams) {
    parameters = input;
  } else if (typeof input === "string") {
    parameters = new URLSearchParams(input.startsWith("?") ? input.slice(1) : input);
  } else if (isRecord(input)) {
    parameters = new URLSearchParams();
    for (const [key, value] of Object.entries(input)) {
      if (value !== undefined && value !== null) parameters.set(key, String(value));
    }
  } else {
    parameters = new URLSearchParams();
  }

  return {
    query: normalizeSearchQuery(parameters.get("q") ?? parameters.get("query") ?? ""),
    page: normalizePage(parameters.get("page"), 1),
    pageSize: normalizePageSize(parameters.get("page_size") ?? parameters.get("pageSize"), DEFAULT_PAGE_SIZE),
  };
}

export function normalizeSlug(value) {
  if (typeof value !== "string") return null;
  let decoded;
  try {
    decoded = decodeURIComponent(value);
  } catch {
    return null;
  }
  const normalized = decoded.normalize("NFC").trim().toLowerCase();
  if (normalized.length === 0 || normalized.length > MAX_SLUG_LENGTH || !SLUG_PATTERN.test(normalized)) {
    return null;
  }
  return normalized;
}

export function isOfferActiveAt(expiresAt, now = Date.now()) {
  if (expiresAt === null || expiresAt === undefined || expiresAt === "") return true;
  if (typeof expiresAt !== "string"
    || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(expiresAt)
    || !Number.isFinite(now)) return false;
  const expires = Date.parse(expiresAt);
  return Number.isFinite(expires) && expires > now;
}

function routeSource(url) {
  if (url.hash.startsWith("#/")) {
    return { source: "hash", routeUrl: new URL(url.hash.slice(1), url.origin) };
  }

  const routeParameter = url.searchParams.get("route");
  if (routeParameter?.startsWith("/")) {
    try {
      const fallback = new URL(routeParameter, url.origin);
      if (fallback.origin === url.origin) return { source: "query", routeUrl: fallback };
    } catch {
      // Fall through to the clean path. Invalid fallback input never becomes markup.
    }
  }

  return { source: "history", routeUrl: url };
}

function relativeRoutePath(pathname, applicationBasePath) {
  if (typeof applicationBasePath !== "string" || !applicationBasePath.startsWith("/")) {
    throw new TypeError("application base path must be an absolute pathname");
  }
  const parsedBase = new URL(applicationBasePath, "https://catalog.invalid/");
  if (
    parsedBase.origin !== "https://catalog.invalid"
    || parsedBase.search
    || parsedBase.hash
  ) {
    throw new TypeError("application base path must be an absolute pathname");
  }
  const basePath = parsedBase.pathname.endsWith("/")
    ? parsedBase.pathname
    : `${parsedBase.pathname}/`;
  if (basePath === "/") return pathname;
  if (pathname === basePath.slice(0, -1)) return "/";
  if (!pathname.startsWith(basePath)) return null;
  return `/${pathname.slice(basePath.length)}`;
}

export function parseRoute(locationLike, applicationBasePath = "/") {
  const base = typeof locationLike === "string"
    ? locationLike
    : locationLike?.href ?? `https://catalog.invalid${locationLike?.pathname ?? "/"}${locationLike?.search ?? ""}${locationLike?.hash ?? ""}`;
  const url = new URL(base, "https://catalog.invalid/");
  const { source, routeUrl } = routeSource(url);
  const routePath = source === "history"
    ? relativeRoutePath(routeUrl.pathname, applicationBasePath)
    : routeUrl.pathname;
  const rawSegments = (routePath ?? "/__outside_application_base__").split("/").filter(Boolean);
  let segments;
  try {
    segments = rawSegments.map((part) => decodeURIComponent(part));
  } catch {
    segments = ["__invalid__"];
  }
  const search = normalizeSearchParams(routeUrl.searchParams);

  let route;
  if (segments.length === 0 || (segments.length === 1 && segments[0] === "index.html")) {
    route = { name: "home", path: "/" };
  } else if (segments.length === 1 && segments[0] === "minerals") {
    route = { name: "minerals", path: "/minerals" };
  } else if (segments.length === 2 && segments[0] === "minerals") {
    const slug = normalizeSlug(segments[1]);
    route = slug
      ? { name: "mineral", path: `/minerals/${encodeURIComponent(slug)}`, slug }
      : { name: "not-found", path: routePath ?? routeUrl.pathname };
  } else if (segments.length === 1 && segments[0] === "about") {
    route = { name: "about", path: "/about" };
  } else if (segments.length === 1 && segments[0] === "map") {
    route = { name: "map", path: "/map" };
  } else {
    route = { name: "not-found", path: routePath ?? routeUrl.pathname };
  }

  return { ...route, source, search };
}

export function routeHref(path, parameters = undefined) {
  if (typeof path !== "string" || !path.startsWith("/")) {
    throw new TypeError("route path must begin with /");
  }
  const route = new URL(path, "https://catalog.invalid/");
  if (parameters) {
    const search = parameters instanceof URLSearchParams ? parameters : new URLSearchParams(parameters);
    route.search = search.toString();
  }
  return `#${route.pathname}${route.search}`;
}

export function validateManifest(value, baseUrl = "https://catalog.invalid/") {
  requireExactKeys(
    value,
    ["format", "schema_version", "generated_at", "release_id", "mineral_count", "database"],
    "catalog manifest",
  );
  if (value.format !== CATALOG_FORMAT || value.schema_version !== CATALOG_SCHEMA_VERSION) {
    throw new TypeError("catalog manifest format or schema version is unsupported");
  }
  requireString(value.generated_at, "generated_at", 80);
  const generatedAt = new Date(value.generated_at);
  if (!Number.isFinite(generatedAt.valueOf())
    || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(value.generated_at)) {
    throw new TypeError("generated_at must be an RFC 3339 timestamp with an explicit offset");
  }
  if (typeof value.release_id !== "string" || !SHA256_PATTERN.test(value.release_id)) {
    throw new TypeError("release_id has an invalid format");
  }
  if (!Number.isSafeInteger(value.mineral_count) || value.mineral_count < 0) {
    throw new TypeError("mineral_count must be a non-negative safe integer");
  }
  requireExactKeys(value.database, ["path", "sha256", "bytes"], "manifest database");
  if (typeof value.database.sha256 !== "string" || !SHA256_PATTERN.test(value.database.sha256)) {
    throw new TypeError("database.sha256 must be sha256: followed by 64 lowercase hexadecimal characters");
  }
  if (!Number.isSafeInteger(value.database.bytes) || value.database.bytes <= 0) {
    throw new TypeError("database.bytes must be a positive safe integer");
  }
  requireString(value.database.path, "database.path", 240);
  const digest = SHA256_PATTERN.exec(value.database.sha256)[1];
  if (value.database.path !== `data/catalog-${digest}.sqlite3`) {
    throw new TypeError("database.path must be the content-addressed catalog filename");
  }
  const manifestUrl = new URL(baseUrl, "https://catalog.invalid/");
  const databaseUrl = new URL(value.database.path, manifestUrl);
  if (databaseUrl.origin !== manifestUrl.origin || databaseUrl.username || databaseUrl.password || databaseUrl.search || databaseUrl.hash) {
    throw new TypeError("database.path must resolve to an uncredentialed same-origin URL without query or fragment");
  }
  return {
    format: value.format,
    schema_version: value.schema_version,
    generated_at: value.generated_at,
    release_id: value.release_id,
    mineral_count: value.mineral_count,
    database: {
      path: value.database.path,
      sha256: value.database.sha256,
      digest,
      bytes: value.database.bytes,
      url: databaseUrl.href,
    },
  };
}

function validateRequestId(value) {
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new TypeError("worker request id must be a positive safe integer");
  }
  return value;
}

export function validateWorkerRequest(message) {
  requireExactKeys(message, ["id", "type", "payload"], "worker request");
  const id = validateRequestId(message.id);
  if (!REQUEST_TYPES.has(message.type)) throw new TypeError("worker request type is unsupported");

  if (message.type === "init") {
    requireExactKeys(message.payload, ["manifestUrl"], "init payload");
    return { id, type: message.type, payload: { manifestUrl: requireString(message.payload.manifestUrl, "manifestUrl") } };
  }
  if (message.type === "search") {
    requireExactKeys(message.payload, ["query", "page", "pageSize"], "search payload");
    if (typeof message.payload.query !== "string" || message.payload.query.length > MAX_QUERY_LENGTH * 2) {
      throw new TypeError("search query is invalid");
    }
    return {
      id,
      type: message.type,
      payload: {
        query: normalizeSearchQuery(message.payload.query),
        page: normalizePage(message.payload.page),
        pageSize: normalizePageSize(message.payload.pageSize),
      },
    };
  }
  requireExactKeys(message.payload, ["slug"], `${message.type} payload`);
  const slug = normalizeSlug(message.payload.slug);
  if (!slug) throw new TypeError("mineral slug is invalid");
  return { id, type: message.type, payload: { slug } };
}

export function validateWorkerResponse(message) {
  if (!isRecord(message)) throw new TypeError("worker response must be an object");
  validateRequestId(message.id);
  if (!REQUEST_TYPES.has(message.type)) throw new TypeError("worker response type is unsupported");
  if (typeof message.ok !== "boolean") throw new TypeError("worker response ok must be boolean");
  if (message.ok) {
    requireExactKeys(message, ["id", "type", "ok", "result"], "successful worker response");
    if (!isRecord(message.result) && !Array.isArray(message.result) && message.result !== null) {
      throw new TypeError("worker result has an invalid type");
    }
  } else {
    requireExactKeys(message, ["id", "type", "ok", "error"], "failed worker response");
    requireExactKeys(message.error, ["code", "message"], "worker error");
    if (!/^[A-Z][A-Z0-9_]{1,63}$/.test(message.error.code)) throw new TypeError("worker error code is invalid");
    requireString(message.error.message, "worker error message", 1_000);
  }
  return message;
}

export function isKnownRouteName(value) {
  return ROUTE_NAMES.has(value);
}
