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

function isPublicResearchItem(item) {
  const nonpublic = new Set(["draft", "private", "rejected", "withdrawn", "withheld"]);
  return isRecord(item) && Object.prototype.toString.call(item) === "[object Object]" && item.private_research !== true
    && !nonpublic.has(item.publication_status) && !nonpublic.has(item.review_status);
}

// Public research is an explicit release projection, separate from working notes.
export function publishedResearchProfile(rawProperties) {
  let properties = rawProperties;
  if (typeof properties === "string") {
    try { properties = JSON.parse(properties); } catch { properties = {}; }
  }
  if (!isRecord(properties)) properties = {};
  const { public_research: candidate, ...otherProperties } = properties;
  const publicItem = isPublicResearchItem;
  if (!publicItem(candidate) || candidate.publication_status !== "published"
    || !["reviewed", "verified"].includes(candidate.review_status)) {
    return { properties: otherProperties, research: null };
  }
  const collection = (key) => Array.isArray(candidate[key]) ? candidate[key].filter(publicItem) : [];
  return {
    properties: otherProperties,
    research: {
      description: publicItem(candidate.description) && typeof candidate.description.text === "string"
        ? { ...candidate.description, source_urls: Array.isArray(candidate.description.source_urls) ? candidate.description.source_urls.filter((url) => typeof url === "string") : [] }
        : null,
      observations: collection("observations"),
      cod_records: collection("cod_records"),
      structures: collection("structures"),
    },
  };
}

// Presentation categories describe the question, never the scientific scope of
// its answer. In particular, a specimen observation is not a species property.
const OBSERVATION_GROUPS = [
  ["physical", "Physical properties", ["physical", "phys", "appearance", "morphology", "mechanical", "density", "luminescence", "thermal"]],
  ["optical", "Optical properties", ["optical", "opt", "optics"]],
  ["chemistry", "Chemistry", ["chemistry", "chem", "chemical"]],
  ["crystallography", "Crystallography and structure", ["crystallography", "structure"]],
  ["geology", "Occurrence and formation", ["geology", "geo", "occurrence", "locality", "formation"]],
  ["history", "Discovery and history", ["history", "provenance", "type"]],
  ["identity", "Identity and classification", ["identity", "classification", "identifiers", "names", "relationships", "authority"]],
  ["diagnostic", "Diagnostic features", ["diagnostic", "diagnostics"]],
  ["spectroscopy", "Spectroscopy", ["spectroscopy"]],
  ["safety", "Safety observations", ["safety"]],
  ["uses", "Uses", ["uses", "use"]],
  ["significance", "Scientific significance", ["significance"]],
  ["editorial", "Qualifications and remaining gaps", ["editorial", "curation", "research", "evidence", "sources"]],
  ["other", "Further observations", []],
];
const OBSERVATION_PREFIXES = new Map(OBSERVATION_GROUPS.flatMap(([key, , aliases]) => aliases.map((alias) => [alias, key])));
const LEGACY_OBSERVATION_GROUPS = new Map([
  ["physical", ["hardness", "density", "physical_appearance"]],
  ["optical", ["optical_properties", "optical_source_conflict"]],
  ["chemistry", ["analytical_method", "chemical_analysis_wt_percent", "formula_basis", "optimized_empirical_site_formula", "iron_valence_limitations", "scattering_proxy_occupancies", "bond_valence_comparison"]],
  ["crystallography", ["unit_cell", "symmetry", "scxrd_method", "refinement_statistics", "refinement_constraints", "hydrogen_model", "coordination_and_bond_lengths", "powder_pattern_scope"]],
  ["geology", ["type_locality", "paragenesis", "sample_origin", "genetic_interpretation"]],
  ["history", ["discovery_and_name", "approved_redefinition"]],
  ["identity", ["identity_dominant_ree", "related_mineral_comparison"]],
  ["spectroscopy", ["moessbauer_method", "moessbauer_result", "raman_method", "raman_interpretation"]],
  ["editorial", ["specimen_scope", "cod_evidence_scope", "source_software_and_f000_conflicts"]],
].flatMap(([group, keys]) => keys.map((key) => [key, group])));
const OBSERVATION_LABELS = {
  hardness_mohs: "Mohs hardness", mohs_hardness: "Mohs hardness",
  hardness_vickers: "Vickers hardness", vickers_hardness: "Vickers hardness",
  density_g_cm3: "Density (g/cm³)", density_calculated_g_cm3: "Calculated density (g/cm³)",
  density_measured_g_cm3: "Measured density (g/cm³)", density_calculated: "Calculated density",
  density_measured: "Measured density", crystal_system: "Crystal system", unit_cell: "Unit cell",
  space_group: "Space group", ima_number: "IMA number", ima_identifier: "IMA identifier",
  ima_symbol: "IMA symbol", "2v": "Optic axial angle (2V)", two_v: "Optic axial angle (2V)",
  optic_axis_angle_2v: "Optic axial angle (2V)", refractive_indices: "Refractive indices",
  colour: "Color", lustre: "Luster",
};
const OBSERVATION_CONTEXT_FIELDS = [
  ["subject_scope", "Applies to"], ["source_scope", "Source scope"], ["unit", "Unit"], ["units", "Units"],
  ["production_kind", "Production kind"], ["conditions", "Conditions"], ["specimen", "Specimen"],
  ["temperature", "Temperature"], ["pressure", "Pressure"], ["method", "Method"],
  ["wavelength", "Wavelength"], ["uncertainty", "Uncertainty"],
];
const OBSERVATION_QUALIFICATION_FIELDS = [["qualification", "Qualification"], ["scientific_note", "Scientific note"], ["qualifiers", "Qualifiers"]];

function observationLabel(value) {
  const text = value.replace(/[_-]+/gu, " ").replace(/\s+/gu, " ").trim();
  return text ? text[0].toUpperCase() + text.slice(1) : "Observation";
}

function observationGroup(questionKey) {
  const key = questionKey.toLowerCase();
  const prefix = key.split(".")[0];
  if (key.includes(".")) return OBSERVATION_PREFIXES.get(prefix) || "other";
  return LEGACY_OBSERVATION_GROUPS.get(key)
    || [...OBSERVATION_PREFIXES].find(([alias]) => key.startsWith(`${alias}_`))?.[1] || "other";
}

// This adapter consumes an already published profile. It adds display metadata
// without parsing strings as JSON, inventing absent fields, or altering records.
export function normalizeMineralObservation(item) {
  if (!isPublicResearchItem(item)) return null;
  const questionKey = typeof item.question_key === "string" ? item.question_key.normalize("NFC").trim() : "";
  const leaf = questionKey.split(".").at(-1);
  const explicitLabel = [item.question_label, item.display_label, item.label].find((value) => typeof value === "string" && value.trim());
  const group = observationGroup(questionKey);
  const groupLabel = OBSERVATION_GROUPS.find(([key]) => key === group)[1];
  const contextKeys = OBSERVATION_CONTEXT_FIELDS;
  const qualificationKeys = OBSERVATION_QUALIFICATION_FIELDS;
  const extract = (keys) => keys.filter(([key]) => Object.hasOwn(item, key)).map(([key, label]) => ({ label, value: item[key] }));
  const represented = new Set([
    "question_key", "question_label", "display_label", "label", "value", ...contextKeys.map(([key]) => key), ...qualificationKeys.map(([key]) => key),
    // These remain attached to the original item and its numbered reference.
    "source_url", "source_title", "publisher", "locator", "license_spdx", "attribution", "bibliography",
    "claim_id", "review_status", "publication_status", "private_research", "evidence_level",
  ]);
  return {
    raw: item, questionKey,
    label: explicitLabel?.trim() || (Object.hasOwn(OBSERVATION_LABELS, leaf.toLowerCase()) ? OBSERVATION_LABELS[leaf.toLowerCase()] : observationLabel(leaf)),
    group, groupLabel, hasValue: Object.hasOwn(item, "value") && item.value !== undefined, value: item.value,
    context: extract(contextKeys), qualifications: extract(qualificationKeys),
    extraFields: Object.fromEntries(Object.entries(item).filter(([key]) => !represented.has(key))),
  };
}

export function groupMineralObservations(observations) {
  if (!Array.isArray(observations)) return [];
  const groups = new Map();
  for (const item of observations) {
    const observation = normalizeMineralObservation(item);
    if (!observation) continue;
    if (!groups.has(observation.group)) groups.set(observation.group, []);
    groups.get(observation.group).push(observation);
  }
  return OBSERVATION_GROUPS.flatMap(([key, label]) => groups.has(key) ? [{ key, label, observations: groups.get(key) }] : []);
}

// A compact rendering plan, not a lossy value conversion. Callers must render
// `remaining` too; unknown or ambiguous compound values use the generic view.
export function compactStructuredValue(value) {
  const scalar = (item) => typeof item === "string" || typeof item === "boolean"
    || (typeof item === "number" && Number.isFinite(item));
  if (Array.isArray(value)) {
    if (!value.length || value.length > 24 || !value.every(scalar)) return null;
    const text = value.map(String).join(", ");
    return text.length <= 240 && value.every((item) => typeof item !== "string" || item.trim())
      ? { kind: "list", text, remaining: {} } : null;
  }
  if (!isRecord(value)) return null;
  const numeric = (item) => (typeof item === "number" && Number.isFinite(item))
    || (typeof item === "string" && /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/iu.test(item) && Number.isFinite(Number(item)));
  const ranges = [["min", "max"], ["minimum", "maximum"], ["lower", "upper"]].filter((pair) => pair.every((key) => Object.hasOwn(value, key)));
  const consumed = new Set();
  const unitKey = ["unit", "units"].filter((key) => Object.hasOwn(value, key));
  const unit = unitKey.length === 1 && typeof value[unitKey[0]] === "string" && value[unitKey[0]].trim() ? value[unitKey[0]] : null;
  let text;
  let kind;
  if (ranges.length === 1) {
    const [minimum, maximum] = ranges[0];
    if (!numeric(value[minimum]) || !numeric(value[maximum]) || Number(value[minimum]) > Number(value[maximum])) return null;
    text = `${value[minimum]}–${value[maximum]}`;
    consumed.add(minimum).add(maximum);
    kind = "range";
  } else if (ranges.length === 0 && Object.hasOwn(value, "value") && scalar(value.value)
    && (Object.hasOwn(value, "unit") || Object.hasOwn(value, "units") || Object.hasOwn(value, "raw_value") || Object.hasOwn(value, "standard_uncertainty"))) {
    const key = typeof value.raw_value === "string" && value.raw_value.trim() ? "raw_value" : "value";
    text = String(value[key]);
    consumed.add(key);
    kind = "measurement";
  } else return null;
  if (unit !== null) {
    const trimmedText = text.trimEnd();
    const unitStart = trimmedText.length - unit.trim().length;
    const exactUnitToken = unitStart >= 0 && trimmedText.slice(unitStart) === unit.trim()
      && (unitStart === 0 || /\s/u.test(trimmedText[unitStart - 1]));
    const uncertainNumber = text.match(/^([+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?)\(\d+\)$/iu);
    const bareNumber = numeric(text) || (uncertainNumber !== null && Number.isFinite(Number(uncertainNumber[1])));
    if (exactUnitToken) consumed.add(unitKey[0]);
    else if (kind === "range" || bareNumber) {
      text += ` ${unit}`;
      consumed.add(unitKey[0]);
    }
    // A suffix such as "m" in "10 mm" is not the same unit token. Keep
    // conflicting or ambiguous units visible instead of guessing a conversion.
  }
  return { kind, text, remaining: Object.fromEntries(Object.entries(value).filter(([key]) => !consumed.has(key))) };
}

// A source-scoped presentation plan. This never feeds the mineral fact selector.
export function sourceModelSummary(item) {
  const present = (value) => value !== null && value !== undefined && value !== "";
  const scalar = (value) => typeof value === "string" ? value.trim() : typeof value === "number" && Number.isFinite(value) ? String(value) : null;
  const units = { angstrom: "Å", degree: "°", kelvin: "K", kilopascal: "kPa", angstrom_cubed: "Å³", cubic_angstrom: "Å³" };
  const measurement = (value, fallbackUnit = "") => {
    const direct = scalar(value);
    if (direct !== null) {
      if (/^(?:[?.]|not reported|unknown)$/iu.test(direct)) return null;
      return direct && fallbackUnit && /^[+-]?[\d.]+(?:\(\d+\))?$/u.test(direct) ? `${direct} ${fallbackUnit}` : direct;
    }
    if (!isRecord(value)) return null;
    const raw = scalar(value.raw_value) ?? scalar(value.source_value) ?? scalar(value.value);
    if (!raw || /^(?:[?.]|not reported|unknown)$/iu.test(raw)) return null;
    const unit = units[value.unit] || value.unit || fallbackUnit;
    let text = `${raw}${unit ? ` ${unit}` : ""}`;
    const uncertainty = scalar(value.raw_uncertainty) ?? scalar(value.standard_uncertainty);
    const inline = /\(\d+\)/u.test(raw);
    if (uncertainty !== null && (!inline || (present(value.inline_standard_uncertainty) && Number(value.inline_standard_uncertainty) !== Number(value.standard_uncertainty)))) {
      text += ` (σ ${uncertainty}${unit ? ` ${unit}` : ""})`;
    }
    return text;
  };
  const rows = [];
  const add = (label, value) => { const text = measurement(value); if (text) rows.push({ label, value: text }); };
  add("Deposited formula", item.source_formula);
  const cell = isRecord(item.cell) ? item.cell : isRecord(item.unit_cell) ? item.unit_cell : {};
  const cellValues = ["a", "b", "c", "alpha", "beta", "gamma"].flatMap((key, index) => {
    const value = cell[key] ?? cell[`${key}_${index < 3 ? "angstrom" : "deg"}`] ?? cell[`${key}_A`];
    const text = measurement(value, index < 3 ? "Å" : "°");
    return text ? [{ label: ["a", "b", "c", "α", "β", "γ"][index], value: text }] : [];
  });
  const symmetry = isRecord(item.symmetry) ? item.symmetry : isRecord(item.space_group) ? item.space_group : {};
  add("Space group", symmetry.sg ?? symmetry.space_group ?? symmetry.space_group_hm ?? item.space_group);
  add("Hall symbol", symmetry.sgHall ?? symmetry.hall_symbol ?? symmetry.Hall);
  add("Space group number", symmetry.sgNumber ?? symmetry.space_group_number ?? symmetry.IT_number);
  add("Cell volume", item.volume ?? cell.volume ?? cell.V_A3);
  add("Formula units (Z)", item.formula_units_Z ?? cell.Z);
  const conditions = isRecord(item.conditions) ? item.conditions : {};
  for (const [key, label] of [["cell_temperature", "Cell temperature"], ["diffraction_temperature", "Diffraction temperature"], ["cell_pressure", "Cell pressure"], ["diffraction_pressure", "Diffraction pressure"], ["temperature", "Temperature"], ["pressure", "Pressure"], ["thermal_history", "Thermal history"], ["pressure_history", "Pressure history"]]) add(label, conditions[key]);
  const origin = item.sample_origin ?? item.origin ?? item.specimen_origin ?? conditions.sample_origin ?? conditions.origin;
  add("Specimen origin", isRecord(origin) ? origin.raw_value ?? origin.value : origin);
  add("Source mineral name", item.source_mineral_name);
  add("Source sample description", item.source_sample_description);
  // Keep source status separate from species facts; the complete source record
  // remains available to inspect unknown fields and measurement provenance.
  const qualification = [item.qualification, item.scientific_note, item.qualifiers,
    ...(Array.isArray(item.source_cautions) ? item.source_cautions : [])].filter(present);
  return { rows, cell: cellValues, qualification, record: item };
}

// Summarize only explicit mineral properties. Research observations and deposited
// models retain their own scope, conditions and provenance elsewhere on the page.
export function compactMineralFacts(properties) {
  if (!isRecord(properties)) return [];
  for (const key of ["subject_scope", "source_scope"]) {
    if (Object.hasOwn(properties, key) && properties[key] !== "species") return [];
  }
  const fields = [
    { label: "Crystal system", keys: ["crystal_system"] },
    { label: "Mohs hardness", keys: ["hardness_mohs"] },
    { label: "Color", keys: ["color", "colour"] },
    { label: "Streak", keys: ["streak"] },
    { label: "Luster", keys: ["luster", "lustre"] },
    { label: "Density", keys: ["density_g_cm3"], unit: "g/cm³" },
    { label: "Cleavage", keys: ["cleavage"] },
    { label: "Fracture", keys: ["fracture"] },
    { label: "Transparency", keys: ["transparency"] },
  ];
  const placeholder = /^(?:[-—–?]|n\/?a|unknown|not (?:reported|available)|null|nan|[+-]?infinity)$/iu;
  const numeric = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/iu;
  const numericRange = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)\s*(?:[-–—]|to)\s*[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/iu;
  const scalar = (value) => {
    if (typeof value === "number") return Number.isFinite(value) ? String(value) : null;
    if (typeof value !== "string") return null;
    const text = value.trim();
    return text && !placeholder.test(text) && (!numeric.test(text) || Number.isFinite(Number(text))) ? text : null;
  };
  const withUnit = (text, unit) => unit && (numeric.test(text) || numericRange.test(text)) ? `${text} ${unit}` : text;
  const displayValue = (value, unit) => {
    const text = scalar(value);
    if (text !== null) return withUnit(text, unit);
    if (Array.isArray(value)) {
      if (!value.length) return null;
      const items = value.map(scalar);
      if (items.some((item) => item === null)) return null;
      return items.map((item) => withUnit(item, unit)).join(", ");
    }
    // Extra fields may carry scope or qualifications, so do not reduce a
    // measurement object to its numeric part or discard those extra fields.
    if (!ownKeysAre(value, ["min", "max"])) return null;
    const minimum = scalar(value.min);
    const maximum = scalar(value.max);
    if (minimum === null || maximum === null || !numeric.test(minimum) || !numeric.test(maximum)
      || !Number.isFinite(Number(minimum)) || !Number.isFinite(Number(maximum))
      || Number(minimum) > Number(maximum)) return null;
    return `${minimum}–${maximum}${unit ? ` ${unit}` : ""}`;
  };
  return fields.flatMap(({ label, keys, unit }) => {
    const key = keys.find((candidate) => Object.hasOwn(properties, candidate));
    if (key === undefined) return [];
    const value = displayValue(properties[key], unit);
    return value === null ? [] : [{ label, value }];
  });
}

const SUBSCRIPT_DIGITS = "₀₁₂₃₄₅₆₇₈₉";
const SUPERSCRIPT_DIGITS = "⁰¹²³⁴⁵⁶⁷⁸⁹";
const PLAIN_CRYSTAL_SYSTEMS = new Map([
  ["triclinic", "Triclinic"], ["monoclinic", "Monoclinic"], ["orthorhombic", "Orthorhombic"],
  ["tetragonal", "Tetragonal"], ["trigonal", "Trigonal"], ["hexagonal", "Hexagonal"],
  ["cubic", "Cubic"], ["rhombohedral", "Trigonal"], ["isometric", "Cubic"],
]);

function translatedDigits(text, alphabet) {
  return [...text].map((character) => alphabet[character] ?? character).join("");
}

// Display-only mineral formula typography. Counts become subscripts and
// charges become superscripts. Coefficients, decimals, and uncertainty marks
// stay on the baseline. The stored formula is not rewritten.
export function formatFormula(formula) {
  const source = typeof formula === "string" ? formula : "";
  const parts = [];
  const push = (kind, text) => {
    if (!text) return;
    const last = parts.at(-1);
    if (last?.kind === kind) last.text += text;
    else parts.push({ kind, text });
  };
  const digit = (character) => character >= "0" && character <= "9";
  let index = 0;
  while (index < source.length) {
    const character = source[index];
    if (digit(character)) {
      let end = index;
      while (end < source.length && digit(source[end])) end += 1;
      if (source[end] === "." && digit(source[end + 1] ?? "")) {
        end += 2;
        while (end < source.length && digit(source[end])) end += 1;
      }
      const number = source.slice(index, end);
      const next = source[end] ?? "";
      const previous = source[index - 1] ?? "";
      const last = parts.at(-1);
      const before = parts.at(-2);
      const followsAtom = /[A-Za-z)\]☐]/u.test(previous) || last?.kind === "sup";
      const range = !number.includes(".") && last?.kind === "base" && last.text.endsWith("-")
        && before?.kind === "sub";
      if (!number.includes(".") && (next === "+" || next === "-")) {
        push("sup", `${translatedDigits(number, SUPERSCRIPT_DIGITS)}${next === "+" ? "⁺" : "⁻"}`);
        index = end + 1;
        continue;
      }
      if (!number.includes(".") && (followsAtom || range)) {
        push("sub", translatedDigits(number, SUBSCRIPT_DIGITS));
        index = end;
        continue;
      }
      push("base", number);
      index = end;
      continue;
    }
    if ((character === "+" || character === "-") && /[A-Za-z)\]☐]/u.test(source[index - 1] ?? "")) {
      push("sup", character === "+" ? "⁺" : "⁻");
      index += 1;
      continue;
    }
    push("base", character);
    index += 1;
  }
  return parts;
}

export function plainCrystalSystemName(value) {
  if (typeof value !== "string") return null;
  let key = value.trim().replace(/\s+/gu, " ");
  if (key.endsWith(".")) key = key.slice(0, -1).trim();
  return PLAIN_CRYSTAL_SYSTEMS.get(key.toLowerCase()) ?? null;
}

// One species-level system can sit in the facts box. A qualified sentence or
// two different plain systems stay with the full reports.
export function plainCrystalSystem(facts) {
  const group = (Array.isArray(facts) ? facts : []).find((item) => item?.key === "crystal_system");
  if (!group) return null;
  const names = [];
  for (const item of [...(group.general ?? []), ...(group.reports ?? [])]) {
    const name = plainCrystalSystemName(typeof item.value === "string" ? item.value : item.value?.crystal_system ?? item.value?.system);
    if (!name) return null;
    names.push(name);
  }
  if (!names.length || new Set(names).size !== 1) return null;
  return names[0];
}

const RESEARCHED_FACT_GROUPS = [
  ["crystal_system", "Crystal system"], ["hardness", "Hardness"], ["color", "Color"],
  ["streak", "Streak"], ["luster", "Luster"], ["density", "Density"],
  ["cleavage", "Cleavage"], ["fracture", "Fracture"], ["transparency", "Transparency"],
  ["crystal_habit", "Crystal habit"], ["tenacity", "Tenacity"], ["appearance", "Appearance"],
];

// These are explicit questionnaire and released legacy aliases, not keyword
// matching. A future question containing "density" is not automatically a fact.
const RESEARCHED_FACT_QUESTIONS = new Map([
  ["crystal_system", "Crystal system", ["crystallography.crystal_system", "structure.crystal_system", "crystal_system"]],
  ["hardness", "Mohs hardness", ["physical.hardness_mohs", "physical.mohs_hardness", "hardness_mohs", "mohs_hardness"]],
  ["hardness", "Vickers hardness", ["physical.hardness_vickers", "physical.vickers_hardness", "hardness_vickers", "vickers_hardness"]],
  ["hardness", "Microhardness", ["physical.microhardness"]],
  ["hardness", "Hardness", ["physical.hardness", "phys.hardness", "hardness"]],
  ["hardness", "Estimated hardness", ["mechanical.hardness_estimate", "physical.hardness_estimated"]],
  ["color", "Color", ["physical.color", "physical.colour", "appearance.colour", "phys.color", "color", "colour"]],
  ["streak", "Streak", ["physical.streak", "streak"]],
  ["luster", "Luster", ["physical.luster", "physical.lustre", "luster", "lustre"]],
  ["density", "Density (g/cm³)", ["physical.density_g_cm3", "density_g_cm3"]],
  ["density", "Measured density (g/cm³)", ["physical.density_measured_g_cm3", "density_measured_g_cm3"]],
  ["density", "Calculated density (g/cm³)", ["physical.density_calculated_g_cm3", "density_calculated_g_cm3"]],
  ["density", "Measured density", ["physical.density_measured", "density_measured"]],
  ["density", "Calculated density", ["physical.density_calculated", "density_calculated"]],
  ["density", "Calculated density (empirical)", ["density.calculated_empirical", "physical.density_calculated_emp"]],
  ["density", "Calculated density (structure)", ["density.calculated_structure"]],
  ["density", "Density (refined proxy)", ["physical.density_refined_proxy"]],
  ["density", "Density", ["physical.density", "phys.density", "density"]],
  ["density", "Specific gravity", ["physical.specific_gravity", "specific_gravity"]],
  ["cleavage", "Cleavage", ["physical.cleavage", "cleavage"]],
  ["fracture", "Fracture", ["physical.fracture", "fracture"]],
  ["transparency", "Transparency", ["physical.transparency", "transparency"]],
  ["crystal_habit", "Crystal habit", ["physical.crystal_habit", "crystal_habit"]],
  ["tenacity", "Tenacity", ["physical.tenacity", "tenacity"]],
  ["appearance", "Color, streak and luster", ["physical.color_streak_lustre"]],
].flatMap(([key, label, aliases]) => aliases.map((alias) => [alias, { key, label }])));

const COMPOUND_APPEARANCE_FIELDS = new Map([
  ["color", "color"], ["colour", "color"], ["streak", "streak"],
  ["luster", "luster"], ["lustre", "luster"], ["cleavage", "cleavage"],
  ["fracture", "fracture"], ["transparency", "transparency"],
  ["crystal_habit", "crystal_habit"], ["tenacity", "tenacity"],
]);
const COMPOUND_CRYSTAL_SYSTEM_QUESTIONS = new Set([
  "symmetry", "crystallography.space_group", "crystallography.unit_cell",
  "crystallography.single_crystal_cell", "crystallography.serpierite_1968_symmetry_cell",
  "crystallography.structure_references",
]);

// This consumes the already published profile returned by
// publishedResearchProfile. General properties remain visibly separate from
// source-scoped reports. A report retains its entire value and observation;
// conflicting measurements, model qualifications and units are never resolved
// by this presentation selector. Deposited models and description prose are not
// substitutes for explicit researched property observations.
export function researchedMineralFacts(properties, research) {
  const groups = new Map(RESEARCHED_FACT_GROUPS.map(([key, label]) => [key, { key, label, general: [], reports: [] }]));
  const generalKeys = new Map([
    ["Crystal system", "crystal_system"], ["Mohs hardness", "hardness"], ["Color", "color"],
    ["Streak", "streak"], ["Luster", "luster"], ["Density", "density"], ["Cleavage", "cleavage"],
    ["Fracture", "fracture"], ["Transparency", "transparency"],
  ]);
  if (isPublicResearchItem(properties)) {
    for (const fact of compactMineralFacts(properties)) groups.get(generalKeys.get(fact.label)).general.push(fact);
  }
  const hasValue = (value) => {
    if (typeof value === "string") return value.trim().length > 0;
    if (typeof value === "number") return Number.isFinite(value);
    if (typeof value === "boolean") return true;
    if (Array.isArray(value)) return value.some(hasValue);
    return isRecord(value) && Object.values(value).some(hasValue);
  };
  const addReport = (observation, key, label, value = observation.value, sourcePath = ["value"], related = {}) => {
    if (!hasValue(value)) return;
    const nestedContext = OBSERVATION_CONTEXT_FIELDS.filter(([field]) => Object.hasOwn(related, field))
      .map(([field, fieldLabel]) => ({
        label: ["subject_scope", "source_scope"].includes(field) ? fieldLabel : `Source record ${fieldLabel.toLowerCase()}`,
        value: related[field],
      }));
    const nestedQualifications = OBSERVATION_QUALIFICATION_FIELDS.filter(([field]) => Object.hasOwn(related, field))
      .map(([field, fieldLabel]) => ({ label: fieldLabel, value: related[field] }));
    const metadataFields = new Set([...OBSERVATION_CONTEXT_FIELDS, ...OBSERVATION_QUALIFICATION_FIELDS].map(([field]) => field));
    const otherRelated = Object.fromEntries(Object.entries(related).filter(([field]) => !metadataFields.has(field)));
    groups.get(key).reports.push({
      ...observation, label, value, sourcePath,
      // Scope and qualifications from the parent value must remain visible
      // when callers disclose the unrelated numerical/unknown sibling fields.
      context: [...observation.context, ...nestedContext, ...(Object.keys(otherRelated).length ? [{ label: "Related reported fields", value: otherRelated }] : [])],
      qualifications: [...observation.qualifications, ...nestedQualifications],
    });
  };
  const observations = isPublicResearchItem(research) && Array.isArray(research.observations) ? research.observations : [];
  for (const raw of observations) {
    const observation = normalizeMineralObservation(raw);
    if (!observation?.hasValue) continue;
    const questionKey = observation.questionKey.toLowerCase();
    const appearance = ["physical_appearance", "physical.color_streak_lustre"].includes(questionKey);
    if (appearance && isRecord(observation.value)) {
      const fields = Object.entries(observation.value);
      const related = Object.fromEntries(fields.filter(([field]) => !COMPOUND_APPEARANCE_FIELDS.has(field)));
      for (const [field, value] of fields) {
        const key = COMPOUND_APPEARANCE_FIELDS.get(field);
        if (key) addReport(observation, key, groups.get(key).label, value, ["value", field], related);
      }
      // An unrecognized compound appearance shape stays in the full article.
      continue;
    }
    if (COMPOUND_CRYSTAL_SYSTEM_QUESTIONS.has(questionKey) && isRecord(observation.value)) {
      // Some released crystallography records explicitly name the system next
      // to a cell or space group. Select only that named field, never infer it
      // from cell dimensions or space-group identifiers. Keep all siblings.
      const fields = Object.entries(observation.value);
      const related = Object.fromEntries(fields.filter(([field]) => !["system", "crystal_system"].includes(field)));
      for (const [field, value] of fields) {
        if (["system", "crystal_system"].includes(field)) addReport(observation, "crystal_system", "Crystal system", value, ["value", field], related);
      }
      continue;
    }
    const definition = RESEARCHED_FACT_QUESTIONS.get(questionKey);
    if (!definition) continue;
    const explicitLabel = [raw.question_label, raw.display_label, raw.label].some((value) => typeof value === "string" && value.trim());
    const protectedLabel = ["hardness", "density"].includes(definition.key) && definition.label !== groups.get(definition.key).label;
    const sourceLabel = explicitLabel && protectedLabel && observation.label !== definition.label
      ? [{ label: "Source property label", value: observation.label }] : [];
    addReport({ ...observation, context: [...observation.context, ...sourceLabel] }, definition.key,
      explicitLabel && !protectedLabel ? observation.label : definition.label);
  }
  return [...groups.values()].filter(({ general, reports }) => general.length || reports.length);
}

// The card is an index into the complete observations, not a second report.
// Only bounded, fully understood values are summarized. Unknown caveats and
// compound comparisons lead to the original reports instead of being dropped.
export function summarizeMineralFact(group) {
  const reports = Array.isArray(group?.reports) ? group.reports : [];
  const general = Array.isArray(group?.general) ? group.general : [];
  const pointer = (text = "See full report") => ({ text, qualifiers: [], reports, mode: "report" });
  const present = (value) => value !== undefined && value !== null && value !== "";
  const shortText = (value, maximum = 90) => {
    if (typeof value === "number") return Number.isFinite(value) ? String(value) : null;
    if (typeof value !== "string") return null;
    const text = value.trim();
    return text && text.length <= maximum && !/[\r\n]/u.test(text)
      && !/^(?:unknown|n\/?a|not (?:reported|available)|null|nan|[+-]?infinity)$/iu.test(text) ? text : null;
  };
  const numeric = "[+-]?(?:\\d+(?:\\.\\d*)?|\\.\\d+)(?:\\(\\d+\\))?";
  const numberOrRange = new RegExp(`^${numeric}(?:\\s*(?:[-–—]|to)\\s*${numeric})?$`, "u");
  const unitIdentity = (unit) => unit === "g/cm³" ? "g/cm3" : unit;
  const cellFields = new Set(["a", "b", "c", "alpha", "beta", "gamma", "a_A", "b_A", "c_A", "alpha_deg", "beta_deg", "gamma_deg", "V_A3", "volume", "Z", "space_group", "Hall", "IT_number"]);
  const makeCandidate = (report) => {
    let text = shortText(report.value);
    const qualifiers = [];
    let analogy = false;
    // An entire-value adapter for explicit estimates. Never extract the first
    // number from arbitrary prose or manufacture a measured hardness.
    if (group.key === "hardness" && typeof report.value === "string") {
      const match = report.value.match(new RegExp(`^(${numeric}(?:\\s*[–-]\\s*${numeric})?) by analogy with other [\\p{L}-]+-group members$`, "u"));
      if (match) { text = match[1]; analogy = true; qualifiers.push("estimated by analogy"); }
    }
    if (text === null) {
      const plan = compactStructuredValue(report.value);
      // Unconsumed members can be qualifiers, competing measurements or units.
      if (!plan || Object.keys(plan.remaining).length || plan.text.length > 90) return null;
      if (plan.kind === "list" && ["density", "hardness"].includes(group.key)) return null;
      text = plan.text;
    }
    const label = typeof report.label === "string" ? report.label : "";
    if (/calculated/iu.test(label)) qualifiers.push("calculated");
    else if (/estimated/iu.test(label) && !analogy) qualifiers.push("estimated");
    else if (/measured/iu.test(label)) qualifiers.push("measured");
    if (/vickers/iu.test(label)) qualifiers.push("Vickers");
    if (/microhardness/iu.test(label)) qualifiers.push("microhardness");
    if (/refined proxy/iu.test(label)) qualifiers.push("refined proxy");
    const inferredUnit = /mohs/iu.test(label) ? "Mohs" : /g\/cm³/iu.test(label) ? "g/cm³" : null;
    const units = [];
    for (const field of Array.isArray(report.context) ? report.context : []) {
      if (!present(field.value)) continue;
      if (field.label === "Related reported fields") {
        if (group.key !== "crystal_system" || !isRecord(field.value)
          || Object.entries(field.value).some(([key, value]) => !cellFields.has(key) || shortText(value, 64) === null)) return null;
        continue;
      }
      if (["Unit", "Units"].includes(field.label)) {
        const unit = shortText(field.value, 20);
        if (!unit) return null;
        units.push(unit);
        continue;
      }
      if (["Applies to", "Source scope"].includes(field.label) && field.value === "species") continue;
      // Keep even a short source scope verbatim: natural, synthetic, historical
      // and specimen observations must not become an unqualified species fact.
      if (["Applies to", "Source scope", "Specimen"].includes(field.label)) {
        const scope = shortText(field.value, 48);
        if (!scope) return null;
        qualifiers.push(scope.replaceAll("_", " "));
        continue;
      }
      // Methods, conditions and unexpected metadata may change interpretation.
      return null;
    }
    for (const field of Array.isArray(report.qualifications) ? report.qualifications : []) {
      if (!present(field.value)) continue;
      if (analogy && field.value === "Explicitly NOT measured.") continue;
      // This qualification explicitly confines a crystallographic report to a
      // studied crystal; its identity and experiment remain in the article.
      if (group.key === "crystal_system" && typeof field.value === "string"
        && /^[^;\n]{1,64} single crystal; room-temperature Mo-Kα measurement\.$/u.test(field.value)) {
        qualifiers.push("studied crystal");
        continue;
      }
      const qualification = shortText(field.value, 72);
      if (!qualification) return null;
      qualifiers.push(qualification.replace(/\.$/u, ""));
    }
    if (isRecord(report.extraFields) && Object.keys(report.extraFields).length) return null;
    const uniqueUnits = [...new Set(units)];
    if (new Set(uniqueUnits.map(unitIdentity)).size > 1
      || (inferredUnit && uniqueUnits.length && unitIdentity(uniqueUnits[0]) !== unitIdentity(inferredUnit))) return null;
    const unit = uniqueUnits[0] || inferredUnit;
    if (unit && numberOrRange.test(text)) text += ` ${unit}`;
    else if (unit && !text.endsWith(` ${unit}`)
      && !(unitIdentity(unit) === "g/cm3" && / g\/cm[3³]$/u.test(text))) return null;
    const uniqueQualifiers = [...new Set(qualifiers)];
    if (text.length > 90 || uniqueQualifiers.join("; ").length > 100
      || text.length + uniqueQualifiers.join("; ").length > 140) return null;
    return { text, qualifiers: uniqueQualifiers };
  };
  const candidates = reports.map(makeCandidate);
  // Compare complete report shapes, too: identical short values with different
  // scopes, methods or historical qualifications are not interchangeable.
  if (reports.length > 1) {
    const identity = (report) => JSON.stringify([report.value, report.label, report.context, report.qualifications, report.extraFields]);
    if (new Set(reports.map(identity)).size > 1) return pointer("See source reports");
  }
  const generalValues = general.map((item) => {
    const text = shortText(item.value);
    return text !== null && item.label === "Mohs hardness" && numberOrRange.test(text) ? `${text} Mohs` : text;
  });
  if (generalValues.some((value) => value === null)) return pointer();
  if (candidates.some((value) => value === null)) return pointer();
  const candidate = candidates[0] || (generalValues.length ? { text: generalValues[0], qualifiers: [] } : null);
  if (!candidate) return pointer();
  if (new Set(generalValues).size > 1 || generalValues.some((value) => value !== candidate.text)) return pointer("See source reports");
  return { ...candidate, reports, mode: "summary" };
}

export function codRelationshipLabel(status) {
  const labels = {
    name_matched: "Name lead · scientific comparison pending",
    identified: "Identified phase relationship",
    counterpart: "Counterpart relationship",
    related: "Related structure or historical model",
    candidate: "Candidate · scientific comparison pending",
    unresolved: "Unresolved relationship",
    name_conflict: "Name conflict · relationship not confirmed",
  };
  return labels[status] ?? "Relationship not classified";
}

export function groupEvidenceBySource(items) {
  const groups = new Map();
  for (const [index, item] of items.entries()) {
    const source = item.canonical_url || item.work_url;
    const key = source
      ? JSON.stringify([source, item.publisher, item.attribution_party, item.license_spdx])
      : `unlinked:${index}`;
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(item);
  }
  return [...groups.values()];
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
