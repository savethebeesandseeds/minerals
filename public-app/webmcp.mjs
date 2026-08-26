import {
  MAX_QUERY_LENGTH,
  MAX_SLUG_LENGTH,
  normalizeSearchQuery,
  normalizeSlug,
  routeHref,
} from "./app-core.mjs";

export const WEBMCP_TOOL_NAMES = Object.freeze(["search_minerals", "get_mineral"]);
export const WEBMCP_RESULT_CHARACTER_BUDGET = 1_500;

const MAX_AGENT_PAGE_SIZE = 5;
const MAX_AGENT_PAGE = 1_000_000;

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function requireObjectWithKeys(value, allowedKeys, label) {
  if (!isRecord(value)) throw new TypeError(`${label} must be an object`);
  const unexpected = Object.keys(value).filter((key) => !allowedKeys.includes(key));
  if (unexpected.length) throw new TypeError(`${label} contains unsupported fields`);
  return value;
}

function codePointLength(value) {
  return [...value].length;
}

function compactText(value, maximum) {
  if (typeof value !== "string") return null;
  const normalized = value.normalize("NFC").trim().replace(/\s+/gu, " ");
  if (!normalized) return null;
  const points = [...normalized];
  return points.length <= maximum ? normalized : `${points.slice(0, maximum - 1).join("")}…`;
}

function nonNegativeInteger(value) {
  return Number.isSafeInteger(value) && value >= 0 ? value : 0;
}

function finiteUnitInterval(value) {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.max(0, Math.min(1, value))
    : null;
}

function assignText(target, key, value, maximum) {
  const compact = compactText(value, maximum);
  if (compact !== null) target[key] = compact;
}

function requireAgentSearchInput(input) {
  const value = requireObjectWithKeys(input, ["query", "page", "page_size"], "search input");
  if (typeof value.query !== "string" || codePointLength(value.query) > MAX_QUERY_LENGTH) {
    throw new TypeError(`query must be a string of at most ${MAX_QUERY_LENGTH} characters`);
  }
  const query = normalizeSearchQuery(value.query);
  if (!query) throw new TypeError("query must contain a searchable term");
  const page = value.page === undefined ? 1 : value.page;
  const pageSize = value.page_size === undefined ? MAX_AGENT_PAGE_SIZE : value.page_size;
  if (!Number.isSafeInteger(page) || page < 1 || page > MAX_AGENT_PAGE) {
    throw new TypeError(`page must be an integer from 1 to ${MAX_AGENT_PAGE}`);
  }
  if (!Number.isSafeInteger(pageSize) || pageSize < 1 || pageSize > MAX_AGENT_PAGE_SIZE) {
    throw new TypeError(`page_size must be an integer from 1 to ${MAX_AGENT_PAGE_SIZE}`);
  }
  return { query, page, pageSize };
}

function requireAgentSlugInput(input) {
  const value = requireObjectWithKeys(input, ["slug"], "mineral input");
  if (typeof value.slug !== "string" || codePointLength(value.slug) > MAX_SLUG_LENGTH) {
    throw new TypeError(`slug must be a string of at most ${MAX_SLUG_LENGTH} characters`);
  }
  const slug = normalizeSlug(value.slug);
  if (!slug || value.slug !== slug) {
    throw new TypeError("slug must exactly match a canonical catalog slug");
  }
  return slug;
}

function requireBaseUrl(value) {
  if (typeof value !== "string" || value.length > 512) {
    throw new TypeError("baseUrl must be a reasonably sized HTTP or HTTPS URL");
  }
  const url = new URL(value);
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) {
    throw new TypeError("baseUrl must be an uncredentialed HTTP or HTTPS URL");
  }
  return url;
}

function mineralUrl(baseUrl, slug) {
  return new URL(routeHref(`/minerals/${encodeURIComponent(slug)}`), baseUrl).href;
}

function compactSearchRecord(item, baseUrl) {
  if (!isRecord(item)) throw new TypeError("catalog search returned an invalid record");
  const slug = normalizeSlug(item.slug);
  if (!slug) throw new TypeError("catalog search returned an invalid slug");
  const record = {
    slug,
    name: compactText(item.canonical_name, 120) ?? slug,
    evidence_count: nonNegativeInteger(item.evidence_count),
    url: mineralUrl(baseUrl, slug),
  };
  assignText(record, "public_id", item.public_id, 80);
  assignText(record, "formula", item.formula, 120);
  assignText(record, "family", item.mineral_family, 100);
  assignText(record, "verification", item.verification_status, 60);
  const quality = finiteUnitInterval(item.data_quality_score);
  if (quality !== null) record.data_quality_score = quality;
  return record;
}

function serializedLength(value) {
  return JSON.stringify(value).length;
}

function compactSearchResult(raw, baseUrl, requested) {
  if (!isRecord(raw) || !Array.isArray(raw.items)) {
    throw new TypeError("catalog search returned an invalid result");
  }
  const result = {
    query: compactText(raw.query, MAX_QUERY_LENGTH) ?? requested.query,
    page: nonNegativeInteger(raw.page) || requested.page,
    page_size: nonNegativeInteger(raw.page_size) || requested.pageSize,
    total: nonNegativeInteger(raw.total),
    total_pages: nonNegativeInteger(raw.total_pages),
    records: raw.items.slice(0, requested.pageSize).map((item) => compactSearchRecord(item, baseUrl)),
  };
  assignText(result, "release_id", raw.release_id, 80);
  let omitted = Math.max(0, raw.items.length - result.records.length);
  if (omitted > 0) result.truncated = true;
  while (serializedLength(result) > WEBMCP_RESULT_CHARACTER_BUDGET && result.records.length > 0) {
    result.records.pop();
    omitted += 1;
    result.truncated = true;
  }
  if (serializedLength(result) > WEBMCP_RESULT_CHARACTER_BUDGET) {
    throw new RangeError("catalog search metadata exceeds the WebMCP result budget");
  }
  return result;
}

function compactEvidence(item) {
  if (!isRecord(item)) throw new TypeError("catalog detail returned invalid evidence");
  const evidence = { position: nonNegativeInteger(item.position) };
  assignText(evidence, "title", item.title || item.work_title, 140);
  assignText(evidence, "publisher", item.publisher || item.attribution_party, 100);
  assignText(evidence, "license_spdx", item.license_spdx, 40);
  assignText(evidence, "review_status", item.review_status, 60);
  assignText(evidence, "retrieved_at", item.retrieved_at, 40);
  const confidence = finiteUnitInterval(item.confidence);
  if (confidence !== null) evidence.confidence = confidence;
  return evidence;
}

function compactMineral(mineral) {
  if (!isRecord(mineral)) throw new TypeError("catalog detail returned an invalid mineral");
  const slug = normalizeSlug(mineral.slug);
  if (!slug) throw new TypeError("catalog detail returned an invalid slug");
  const result = {
    slug,
    name: compactText(mineral.canonical_name, 120) ?? slug,
    evidence_count: nonNegativeInteger(mineral.evidence_count),
    active_offer_count: nonNegativeInteger(mineral.active_offer_count),
  };
  assignText(result, "public_id", mineral.public_id, 80);
  assignText(result, "formula", mineral.formula, 120);
  assignText(result, "description", mineral.description, 260);
  assignText(result, "family", mineral.mineral_family, 100);
  assignText(result, "nomenclature_status", mineral.nomenclature_status, 60);
  assignText(result, "verification_status", mineral.verification_status, 60);
  assignText(result, "source_kind", mineral.source_kind, 60);
  assignText(result, "license_spdx", mineral.license_spdx, 40);
  assignText(result, "cas_number", mineral.cas_number, 40);
  assignText(result, "discovery_country", mineral.discovery_country, 80);
  assignText(result, "source_status", mineral.source_status, 60);
  const quality = finiteUnitInterval(mineral.data_quality_score);
  if (quality !== null) result.data_quality_score = quality;
  return result;
}

function compactMineralResult(raw, baseUrl, requestedSlug) {
  if (!isRecord(raw)) throw new TypeError("catalog detail returned an invalid result");
  if (raw.mineral === null) {
    const missing = { found: false, slug: requestedSlug };
    assignText(missing, "release_id", raw.release_id, 80);
    return missing;
  }
  if (!Array.isArray(raw.evidence)) throw new TypeError("catalog detail returned invalid evidence");
  const mineral = compactMineral(raw.mineral);
  if (mineral.slug !== requestedSlug) throw new TypeError("catalog detail returned a different mineral");
  const result = {
    found: true,
    url: mineralUrl(baseUrl, requestedSlug),
    mineral,
    evidence: raw.evidence.slice(0, 3).map(compactEvidence),
  };
  assignText(result, "release_id", raw.release_id, 80);
  let omittedEvidence = Math.max(0, raw.evidence.length - result.evidence.length);
  if (omittedEvidence > 0) result.evidence_truncated = true;
  while (serializedLength(result) > WEBMCP_RESULT_CHARACTER_BUDGET && result.evidence.length > 0) {
    result.evidence.pop();
    omittedEvidence += 1;
    result.evidence_truncated = true;
  }
  if (serializedLength(result) > WEBMCP_RESULT_CHARACTER_BUDGET && "description" in result.mineral) {
    delete result.mineral.description;
    result.description_truncated = true;
  }
  const optionalFields = [
    "source_status",
    "source_kind",
    "discovery_country",
    "cas_number",
    "license_spdx",
    "nomenclature_status",
    "verification_status",
    "family",
    "formula",
    "public_id",
    "data_quality_score",
    "active_offer_count",
  ];
  while (serializedLength(result) > WEBMCP_RESULT_CHARACTER_BUDGET && optionalFields.length > 0) {
    const field = optionalFields.shift();
    if (field in result.mineral) {
      delete result.mineral[field];
      result.metadata_truncated = true;
    }
  }
  if (serializedLength(result) > WEBMCP_RESULT_CHARACTER_BUDGET && "release_id" in result) {
    delete result.release_id;
    result.metadata_truncated = true;
  }
  if (serializedLength(result) > WEBMCP_RESULT_CHARACTER_BUDGET) {
    throw new RangeError("mineral detail exceeds the WebMCP result budget");
  }
  return result;
}

function requireAdapter(adapter) {
  if (!isRecord(adapter)
    || typeof adapter.searchMinerals !== "function"
    || typeof adapter.getMineral !== "function") {
    throw new TypeError("WebMCP catalog adapter is incomplete");
  }
  return { ...adapter, baseUrl: requireBaseUrl(adapter.baseUrl) };
}

export function createMineralsWebMcpTools(adapterInput) {
  const adapter = requireAdapter(adapterInput);
  return [
    {
      name: "search_minerals",
      title: "Search Minerals",
      description: "Search Waajacu’s integrity-checked, read-only public mineral catalog by name, formula, family, identifier, or keyword. Use a returned slug with get_mineral for its published record and evidence summary.",
      inputSchema: {
        type: "object",
        additionalProperties: false,
        required: ["query"],
        properties: {
          query: { type: "string", minLength: 1, maxLength: MAX_QUERY_LENGTH, description: "Mineral name, formula, family, identifier, or keyword." },
          page: { type: "integer", minimum: 1, maximum: MAX_AGENT_PAGE, default: 1, description: "One-based result page." },
          page_size: { type: "integer", minimum: 1, maximum: MAX_AGENT_PAGE_SIZE, default: MAX_AGENT_PAGE_SIZE, description: "Records to return, from 1 to 5." },
        },
      },
      annotations: { readOnlyHint: true, untrustedContentHint: true },
      execute: async (input, context = {}) => {
        const requested = requireAgentSearchInput(input);
        const raw = await adapter.searchMinerals(requested, context.signal);
        return compactSearchResult(raw, adapter.baseUrl, requested);
      },
    },
    {
      name: "get_mineral",
      title: "Get Mineral",
      description: "Read one published mineral catalog record and a bounded evidence summary from Waajacu’s integrity-checked local release. Supply the exact slug returned by search_minerals.",
      inputSchema: {
        type: "object",
        additionalProperties: false,
        required: ["slug"],
        properties: {
          slug: { type: "string", minLength: 1, maxLength: MAX_SLUG_LENGTH, pattern: "^[a-z0-9]+(?:[._-][a-z0-9]+)*$", description: "Exact mineral slug returned by search_minerals." },
        },
      },
      annotations: { readOnlyHint: true, untrustedContentHint: true },
      execute: async (input, context = {}) => {
        const slug = requireAgentSlugInput(input);
        const raw = await adapter.getMineral(slug, context.signal);
        return compactMineralResult(raw, adapter.baseUrl, slug);
      },
    },
  ];
}

export async function registerMineralsWebMcp({ modelContext, signal, ...adapterInput }) {
  if (typeof modelContext?.registerTool !== "function") {
    return { supported: false, toolNames: [], dispose() {} };
  }
  const registrationController = new AbortController();
  const abortRegistration = () => registrationController.abort(signal?.reason);
  if (signal?.aborted) abortRegistration();
  else signal?.addEventListener("abort", abortRegistration, { once: true });
  const dispose = () => {
    signal?.removeEventListener("abort", abortRegistration);
    registrationController.abort();
  };
  try {
    const tools = createMineralsWebMcpTools(adapterInput);
    for (const tool of tools) {
      await modelContext.registerTool(tool, { signal: registrationController.signal });
    }
    return { supported: true, toolNames: tools.map((tool) => tool.name), dispose };
  } catch (error) {
    dispose();
    throw error;
  }
}
