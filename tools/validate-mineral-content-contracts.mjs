#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";

const REGISTRY_FILE = "schemas/mineral-record-questionnaire-v1.json";
const MATRIX_FILE = "schemas/mineral-source-admission-matrix-v1.json";
const DOCUMENT_FILE = "docs/MINERAL_RECORD_QUESTIONNAIRE_V1.md";
const QUESTION_KEY = /^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/;
const FIELD_KEY = /^[a-z][a-z0-9_]*$/;
const SHA256 = /^sha256:[0-9a-f]{64}$/;
const HTTPS = /^https:\/\/\S+$/;
const JSON_POINTER_PATTERN = /^\/(?:[^/~]|~[01]|\*)*(?:\/(?:[^/~]|~[01]|\*)*)*$/;
const SURFACES = ["worker", "detail", "card", "search", "facet", "comparison", "map", "safety_banner", "site_tool"];

function fail(message) {
  throw new Error(message);
}

function object(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    fail(label + " must be an object");
  }
  return value;
}

function array(value, label) {
  if (!Array.isArray(value)) fail(label + " must be an array");
  return value;
}

function string(value, label) {
  if (typeof value !== "string" || value.length === 0 || value.trim() !== value) {
    fail(label + " must be a non-empty trimmed string");
  }
  return value;
}

function integer(value, label) {
  if (!Number.isSafeInteger(value) || value <= 0) fail(label + " must be a positive integer");
  return value;
}

function boolean(value, label) {
  if (typeof value !== "boolean") fail(label + " must be a boolean");
  return value;
}

function oneOf(value, values, label) {
  if (!values.includes(value)) fail(label + " has unsupported value " + JSON.stringify(value));
  return value;
}

function exact(value, names, label) {
  object(value, label);
  const actual = Object.keys(value).sort();
  const expected = [...names].sort();
  if (actual.length !== expected.length || actual.some((name, index) => name !== expected[index])) {
    fail(label + " contains an unexpected or missing property");
  }
}

function allowed(value, names, required, label) {
  object(value, label);
  for (const name of Object.keys(value)) {
    if (!names.includes(name)) fail(label + " contains unknown property " + name);
  }
  for (const name of required) {
    if (!(name in value)) fail(label + " is missing required property " + name);
  }
}

function uniqueStrings(value, label, allowEmpty = false) {
  const items = array(value, label);
  if (!allowEmpty && items.length === 0) fail(label + " must not be empty");
  const seen = new Set();
  for (const [index, item] of items.entries()) {
    string(item, label + "[" + index + "]");
    if (seen.has(item)) fail(label + " repeats " + item);
    seen.add(item);
  }
  return items;
}

function exactStringValues(value, expected, label) {
  uniqueStrings(value, label, expected.length === 0);
  if (value.length !== expected.length || value.some((item, index) => item !== expected[index])) {
    fail(label + " does not match the versioned vocabulary");
  }
}

function canonicalJson(value) {
  if (Array.isArray(value)) return "[" + value.map(canonicalJson).join(",") + "]";
  if (value !== null && typeof value === "object") {
    const keys = Object.keys(value).sort((left, right) =>
      Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8")),
    );
    return "{" + keys.map((key) => JSON.stringify(key) + ":" + canonicalJson(value[key])).join(",") + "}";
  }
  return JSON.stringify(value);
}

function canonicalSha256(value) {
  return "sha256:" + createHash("sha256").update(canonicalJson(value), "utf8").digest("hex");
}

function exactValue(value, expected, label) {
  if (canonicalJson(value) !== canonicalJson(expected)) fail(label + " does not match the protected contract");
}

function sameStringSet(actual, expected, label) {
  uniqueStrings(actual, label, expected.length === 0);
  const left = [...actual].sort();
  const right = [...expected].sort();
  if (left.length !== right.length || left.some((item, index) => item !== right[index])) {
    fail(label + " does not match owned fields");
  }
}

function https(value, label) {
  string(value, label);
  if (!HTTPS.test(value)) fail(label + " must be an HTTPS URL");
}

function nfc(value, label) {
  if (typeof value === "string") {
    if (value !== value.normalize("NFC")) fail(label + " contains a non-NFC string");
  } else if (Array.isArray(value)) {
    value.forEach((item, index) => nfc(item, label + "[" + index + "]"));
  } else if (value !== null && typeof value === "object") {
    for (const [key, item] of Object.entries(value)) {
      nfc(key, label + " key");
      nfc(item, label + "." + key);
    }
  }
}

async function json(file, label) {
  let value;
  try {
    value = JSON.parse(await readFile(file, "utf8"));
  } catch (error) {
    fail(label + " is not valid JSON: " + error.message);
  }
  nfc(value, label);
  return value;
}

function cells(line) {
  return line.startsWith("|") && line.endsWith("|")
    ? line.slice(1, -1).split("|").map((cell) => cell.trim())
    : null;
}

function parseDocument(markdown) {
  let section = "";
  const questions = new Map();
  const occurrenceFields = new Map();
  for (const line of markdown.split(/\r?\n/)) {
    if (line.startsWith("## ")) section = line.slice(3);
    const row = cells(line);
    if (!row) continue;
    if (row.length === 5 && /^\x60[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*\x60$/.test(row[0])) {
      const key = row[0].slice(1, -1);
      if (questions.has(key)) fail("questionnaire Markdown repeats question " + key);
      questions.set(key, {
        section,
        question: row[1],
        tier: row[2],
        shape: row[3],
        sources: [...row[4].matchAll(/\x60([A-Z][A-Z0-9]+)\x60/g)].map((match) => match[1]),
      });
    }
    if (
      section === "Occurrence and locality collection" &&
      row.length === 2 &&
      /^\x60[a-z][a-z0-9_]*\x60$/.test(row[0])
    ) {
      const key = row[0].slice(1, -1);
      if (occurrenceFields.has(key)) fail("questionnaire Markdown repeats occurrence field " + key);
      occurrenceFields.set(key, row[1]);
    }
  }
  if (questions.size !== 91) fail("questionnaire Markdown must contain exactly 91 questions");
  if (occurrenceFields.size !== 21) fail("questionnaire Markdown must contain exactly 21 occurrence fields");
  return { questions, occurrenceFields };
}

function valueSchema(schema, key, registry) {
  allowed(
    schema,
    [
      "version",
      "status",
      "value_kind",
      "cardinality",
      "shape",
      "canonical_unit",
      "accepted_units",
      "bounds",
      "vocabulary",
      "json_schema",
      "precision_policy",
      "ordering_policy",
      "maximum_items",
      "affirmative_discriminator",
      "projection_selectors",
      "gate_selectors",
    ],
    ["version", "status", "value_kind", "cardinality", "shape"],
    key + ".value_schema",
  );
  integer(schema.version, key + ".value_schema.version");
  oneOf(schema.status, ["draft", "approved"], key + ".value_schema.status");
  oneOf(schema.value_kind, registry.axes.value_kind, key + ".value_schema.value_kind");
  oneOf(schema.cardinality, ["single", "repeatable"], key + ".value_schema.cardinality");
  string(schema.shape, key + ".value_schema.shape");
  if (["measurement", "range"].includes(schema.value_kind) || "canonical_unit" in schema || "accepted_units" in schema) {
    string(schema.canonical_unit, key + ".value_schema.canonical_unit");
    uniqueStrings(schema.accepted_units, key + ".value_schema.accepted_units");
    if (!schema.accepted_units.includes(schema.canonical_unit)) {
      fail(key + " canonical unit is absent from accepted_units");
    }
  }
  if (schema.value_kind === "controlled_term") {
    exact(schema.vocabulary, ["id", "status"], key + ".value_schema.vocabulary");
    if (!/_v1$/.test(string(schema.vocabulary.id, key + ".value_schema.vocabulary.id"))) {
      fail(key + " controlled vocabulary is not versioned");
    }
    oneOf(schema.vocabulary.status, ["draft", "approved"], key + ".value_schema.vocabulary.status");
    if (schema.status === "approved" && schema.vocabulary.status !== "approved") {
      fail(key + " cannot approve a controlled value schema with a draft vocabulary");
    }
  }
  if ("json_schema" in schema) {
    object(schema.json_schema, key + ".value_schema.json_schema");
    https(schema.json_schema.$schema, key + ".value_schema.json_schema.$schema");
    if (!("type" in schema.json_schema) && !("oneOf" in schema.json_schema) && !("anyOf" in schema.json_schema) && !("$ref" in schema.json_schema)) {
      fail(key + ".value_schema.json_schema must declare type, oneOf, anyOf, or $ref");
    }
    if ("$ref" in schema.json_schema) {
      const reference = string(schema.json_schema.$ref, key + ".value_schema.json_schema.$ref");
      if (!/^schemas\/[a-z0-9_./-]+\.schema\.json$/.test(reference) || reference.includes("..")) {
        fail(key + ".value_schema.json_schema.$ref must be a safe repository-local schema path");
      }
    }
  }
  if (schema.status === "approved" && !("json_schema" in schema)) {
    fail(key + " cannot approve a value schema without an executable json_schema");
  }
  if ("precision_policy" in schema) string(schema.precision_policy, key + ".value_schema.precision_policy");
  if ("ordering_policy" in schema) string(schema.ordering_policy, key + ".value_schema.ordering_policy");
  if ("maximum_items" in schema) integer(schema.maximum_items, key + ".value_schema.maximum_items");
  if ("bounds" in schema) {
    exact(schema.bounds, ["minimum", "maximum"], key + ".value_schema.bounds");
    if (
      typeof schema.bounds.minimum !== "number" ||
      typeof schema.bounds.maximum !== "number" ||
      schema.bounds.minimum > schema.bounds.maximum
    ) {
      fail(key + ".value_schema.bounds are invalid");
    }
  }
  if ("affirmative_discriminator" in schema) {
    const discriminator = schema.affirmative_discriminator;
    exact(
      discriminator,
      ["path", "value_kind", "vocabulary_id", "affirmative_values", "negative_values", "indeterminate_values", "status"],
      key + ".value_schema.affirmative_discriminator",
    );
    string(discriminator.path, key + ".value_schema.affirmative_discriminator.path");
    if (!JSON_POINTER_PATTERN.test(discriminator.path)) fail(key + " has an invalid affirmative discriminator path");
    if (discriminator.value_kind !== "controlled_term") fail(key + " affirmative discriminator must be a controlled term");
    if (!/_v1$/.test(string(discriminator.vocabulary_id, key + ".value_schema.affirmative_discriminator.vocabulary_id"))) {
      fail(key + " affirmative discriminator vocabulary is not versioned");
    }
    oneOf(discriminator.status, ["draft", "approved"], key + ".value_schema.affirmative_discriminator.status");
    const groups = ["affirmative_values", "negative_values", "indeterminate_values"];
    const seen = new Set();
    for (const group of groups) {
      uniqueStrings(discriminator[group], key + ".value_schema.affirmative_discriminator." + group);
      for (const value of discriminator[group]) {
        if (seen.has(value)) fail(key + " affirmative discriminator value sets overlap at " + value);
        seen.add(value);
      }
    }
    if (schema.status === "approved" && discriminator.status !== "approved") {
      fail(key + " has an unapproved affirmative discriminator");
    }
  }
  if ("projection_selectors" in schema) {
    const selectors = object(schema.projection_selectors, key + ".value_schema.projection_selectors");
    if (Object.keys(selectors).length === 0) fail(key + ".value_schema.projection_selectors must not be empty");
    for (const [surface, selector] of Object.entries(selectors)) {
      oneOf(surface, ["search", "facet", "comparison"], key + ".value_schema.projection_selectors surface");
      exact(selector, ["paths", "index_key"], key + ".value_schema.projection_selectors." + surface);
      uniqueStrings(selector.paths, key + ".value_schema.projection_selectors." + surface + ".paths");
      selector.paths.forEach((pointer) => {
        string(pointer, key + ".value_schema.projection_selectors." + surface + ".path");
        if (!JSON_POINTER_PATTERN.test(pointer)) fail(key + " has an invalid " + surface + " projection path " + pointer);
      });
      if (!/^[a-z][a-z0-9_.]*$/.test(string(selector.index_key, key + ".value_schema.projection_selectors." + surface + ".index_key"))) {
        fail(key + " has an invalid " + surface + " projection index key");
      }
    }
  }
  if ("gate_selectors" in schema) {
    const selectors = object(schema.gate_selectors, key + ".value_schema.gate_selectors");
    if (Object.keys(selectors).length === 0) fail(key + ".value_schema.gate_selectors must not be empty");
    for (const [name, selector] of Object.entries(selectors)) {
      if (!FIELD_KEY.test(name)) fail(key + " has an invalid gate selector name " + name);
      exact(selector, ["path"], key + ".value_schema.gate_selectors." + name);
      if (!JSON_POINTER_PATTERN.test(string(selector.path, key + ".value_schema.gate_selectors." + name + ".path"))) {
        fail(key + " has an invalid gate selector path " + selector.path);
      }
    }
  }
}

function requirements(value, productionKinds, key, registry) {
  exact(value, productionKinds, key + ".requirements");
  const compatible = { sourced: "external", derived: "derivation", editorial: "editorial" };
  for (const production of productionKinds) {
    const requirement = value[production];
    exact(requirement, ["mode", "codes"], key + ".requirements." + production);
    oneOf(requirement.mode, ["all", "any"], key + ".requirements." + production + ".mode");
    uniqueStrings(requirement.codes, key + ".requirements." + production + ".codes");
    for (const code of requirement.codes) {
      if (!(code in registry.source_requirements)) fail(key + " references unknown source requirement " + code);
      if (registry.source_requirements[code].kind !== compatible[production]) {
        fail(key + " uses incompatible " + code + " for " + production);
      }
    }
  }
}

function sourceRequirements(registry) {
  const requirementsRegistry = object(registry.source_requirements, "source_requirements");
  const requirementCodes = ["AUTH1", "SCI1", "SCI2", "SYN2", "SAFE1", "LOC1", "DER1", "INT1"];
  exact(requirementsRegistry, requirementCodes, "source_requirements");
  const externalBase = ["kind", "minimum_distinct_sources", "minimum_review_state", "allowed_source_classes", "locator"];
  for (const code of ["AUTH1", "SCI1", "SAFE1"]) {
    const requirement = requirementsRegistry[code];
    exact(requirement, externalBase, "source requirement " + code);
    if (requirement.kind !== "external") fail("source requirement " + code + " must be external");
    integer(requirement.minimum_distinct_sources, "source requirement " + code + ".minimum_distinct_sources");
    if (requirement.minimum_review_state !== "reviewed") fail("source requirement " + code + " must require reviewed claims");
    uniqueStrings(requirement.allowed_source_classes, "source requirement " + code + ".allowed_source_classes");
    requirement.allowed_source_classes.forEach((sourceClass) => {
      if (!/^[a-z][a-z0-9_]*$/.test(sourceClass)) fail("source requirement " + code + " has invalid source class " + sourceClass);
    });
    if (requirement.locator !== "required") fail("source requirement " + code + " must require a locator");
  }
  exact(requirementsRegistry.SCI2, [...externalBase, "minimum_verified_claims", "requires_independence"], "source requirement SCI2");
  if (requirementsRegistry.SCI2.kind !== "external") fail("source requirement SCI2 must be external");
  integer(requirementsRegistry.SCI2.minimum_distinct_sources, "source requirement SCI2.minimum_distinct_sources");
  integer(requirementsRegistry.SCI2.minimum_verified_claims, "source requirement SCI2.minimum_verified_claims");
  if (requirementsRegistry.SCI2.minimum_distinct_sources < 2 || requirementsRegistry.SCI2.requires_independence !== true) {
    fail("source requirement SCI2 must retain two independent sources");
  }
  if (requirementsRegistry.SCI2.minimum_review_state !== "reviewed" || requirementsRegistry.SCI2.locator !== "required") {
    fail("source requirement SCI2 must retain reviewed claims and locators");
  }
  uniqueStrings(requirementsRegistry.SCI2.allowed_source_classes, "source requirement SCI2.allowed_source_classes");
  exact(requirementsRegistry.LOC1, [...externalBase, "coordinate_provenance"], "source requirement LOC1");
  if (requirementsRegistry.LOC1.kind !== "external") fail("source requirement LOC1 must be external");
  integer(requirementsRegistry.LOC1.minimum_distinct_sources, "source requirement LOC1.minimum_distinct_sources");
  uniqueStrings(requirementsRegistry.LOC1.allowed_source_classes, "source requirement LOC1.allowed_source_classes");
  if (
    requirementsRegistry.LOC1.minimum_review_state !== "reviewed" ||
    requirementsRegistry.LOC1.locator !== "required" ||
    requirementsRegistry.LOC1.coordinate_provenance !== "required_when_geometry_present"
  ) {
    fail("source requirement LOC1 has weakened locality evidence rules");
  }
  exact(requirementsRegistry.SYN2, ["kind", "minimum_supporting_claims", "minimum_review_state", "new_scientific_assertions"], "source requirement SYN2");
  if (
    requirementsRegistry.SYN2.kind !== "editorial" ||
    requirementsRegistry.SYN2.minimum_supporting_claims !== 2 ||
    requirementsRegistry.SYN2.minimum_review_state !== "reviewed" ||
    requirementsRegistry.SYN2.new_scientific_assertions !== "forbidden"
  ) {
    fail("source requirement SYN2 has weakened synthesis rules");
  }
  exact(requirementsRegistry.DER1, ["kind", "minimum_supporting_claims", "minimum_review_state", "method_and_inputs"], "source requirement DER1");
  if (
    requirementsRegistry.DER1.kind !== "derivation" ||
    requirementsRegistry.DER1.minimum_supporting_claims !== 1 ||
    requirementsRegistry.DER1.minimum_review_state !== "reviewed" ||
    requirementsRegistry.DER1.method_and_inputs !== "required"
  ) {
    fail("source requirement DER1 has weakened derivation rules");
  }
  exact(requirementsRegistry.INT1, ["kind", "minimum_supporting_states", "minimum_review_state", "new_scientific_assertions"], "source requirement INT1");
  if (
    requirementsRegistry.INT1.kind !== "editorial" ||
    requirementsRegistry.INT1.minimum_supporting_states !== 1 ||
    requirementsRegistry.INT1.minimum_review_state !== "reviewed" ||
    requirementsRegistry.INT1.new_scientific_assertions !== "forbidden"
  ) {
    fail("source requirement INT1 has weakened limitations rules");
  }
}

function consumer(value, label, contract) {
  object(value, label + ".consumer");
  for (const [surface, profile] of Object.entries(value)) {
    if (!(surface in contract.defaults)) fail(label + ".consumer has unknown surface " + surface);
    string(profile, label + ".consumer." + surface);
    if (!(profile in contract.profiles[surface])) {
      fail(label + ".consumer." + surface + " references unknown profile " + profile);
    }
  }
}

function resolved(question, surface, registry, defaults = registry.consumer_contract.defaults) {
  const name = question.consumer[surface] ?? defaults[surface];
  return { name, profile: registry.consumer_contract.profiles[surface][name] };
}

function flatten(registry) {
  const result = new Map();
  const moduleOrders = new Set();
  for (const [moduleId, module] of Object.entries(registry.modules)) {
    if (!FIELD_KEY.test(moduleId)) fail("invalid module key " + moduleId);
    allowed(module, ["order", "label", "lifecycle", "technical_pilot", "questions"], ["order", "label", "lifecycle", "questions"], "module " + moduleId);
    integer(module.order, "module " + moduleId + ".order");
    if (moduleOrders.has(module.order)) fail("duplicate module order " + module.order);
    moduleOrders.add(module.order);
    string(module.label, "module " + moduleId + ".label");
    oneOf(module.lifecycle, registry.axes.module_lifecycle, "module " + moduleId + ".lifecycle");
    object(module.questions, "module " + moduleId + ".questions");
    if ("technical_pilot" in module) {
      const pilot = module.technical_pilot;
      exact(
        pilot,
        ["pilot_id", "kind", "status", "question_keys", "source_matrix_entry", "contract", "coverage_rows", "public_projection"],
        "module " + moduleId + ".technical_pilot",
      );
      if (!/^[a-z][a-z0-9-]+-v[1-9][0-9]*$/.test(string(pilot.pilot_id, "module " + moduleId + ".technical_pilot.pilot_id"))) {
        fail("module " + moduleId + " has an invalid technical pilot id");
      }
      if (pilot.kind !== "adapter_contract_without_record_coverage") fail("module " + moduleId + " technical pilot may create record coverage");
      if (pilot.status !== "contract_ready_candidate_discovery_not_run") fail("module " + moduleId + " has an unsupported technical pilot status");
      uniqueStrings(pilot.question_keys, "module " + moduleId + ".technical_pilot.question_keys");
      string(pilot.source_matrix_entry, "module " + moduleId + ".technical_pilot.source_matrix_entry");
      if (!/^schemas\/pilots\/[a-z0-9-]+\.json$/.test(string(pilot.contract, "module " + moduleId + ".technical_pilot.contract"))) {
        fail("module " + moduleId + " technical pilot has an unsafe contract path");
      }
      if (pilot.coverage_rows !== "forbidden" || pilot.public_projection !== "forbidden") {
        fail("module " + moduleId + " technical pilot may create coverage or public projection");
      }
      if (module.lifecycle !== "not_launched") {
        fail("module " + moduleId + " technical adapter contract requires not_launched lifecycle");
      }
    }
    const orders = new Set();
    for (const [key, question] of Object.entries(module.questions)) {
      if (!QUESTION_KEY.test(key)) fail("invalid question key " + key);
      if (result.has(key)) fail("question occurs in more than one module: " + key);
      allowed(
        question,
        ["order", "label", "question", "definition_version", "definition_status", "scientific_definition", "allowed_coverage_states", "lifecycle", "replaced_by", "priority_tier", "subject_scopes", "production_kinds", "applicability_rule", "value_schema", "normalization_policy", "requirements", "conflict_policy", "visibility", "consumer"],
        ["order", "label", "question", "definition_status", "lifecycle", "replaced_by", "priority_tier", "subject_scopes", "production_kinds", "applicability_rule", "value_schema", "normalization_policy", "requirements", "conflict_policy", "visibility", "consumer"],
        "question " + key,
      );
      integer(question.order, key + ".order");
      if (orders.has(question.order)) fail("duplicate question order in " + moduleId);
      orders.add(question.order);
      string(question.label, key + ".label");
      string(question.question, key + ".question");
      oneOf(question.definition_status, ["draft", "approved"], key + ".definition_status");
      if ("definition_version" in question) integer(question.definition_version, key + ".definition_version");
      if ("scientific_definition" in question) string(question.scientific_definition, key + ".scientific_definition");
      if ("allowed_coverage_states" in question) {
        uniqueStrings(question.allowed_coverage_states, key + ".allowed_coverage_states");
        question.allowed_coverage_states.forEach((state) => oneOf(state, registry.axes.coverage_state, key + ".allowed_coverage_states"));
      }
      if (
        question.definition_status === "approved" &&
        (!("definition_version" in question) || !("scientific_definition" in question) || !("allowed_coverage_states" in question))
      ) {
        fail(key + " cannot be approved without a versioned scientific definition and allowed coverage states");
      }
      oneOf(question.lifecycle, registry.axes.question_lifecycle, key + ".lifecycle");
      if (question.lifecycle === "superseded") string(question.replaced_by, key + ".replaced_by");
      else if (question.replaced_by !== null) fail(key + ".replaced_by must be null");
      oneOf(question.priority_tier, registry.axes.priority_tier, key + ".priority_tier");
      uniqueStrings(question.subject_scopes, key + ".subject_scopes");
      question.subject_scopes.forEach((scope) => oneOf(scope, registry.axes.subject_scope, key + ".subject_scopes"));
      uniqueStrings(question.production_kinds, key + ".production_kinds");
      question.production_kinds.forEach((kind) => oneOf(kind, registry.axes.production_kind, key + ".production_kinds"));
      if (!(question.applicability_rule in registry.applicability_rules)) fail(key + " has unknown applicability rule");
      if (!(question.normalization_policy in registry.normalization_policies)) fail(key + " has unknown normalization policy");
      valueSchema(question.value_schema, key, registry);
      requirements(question.requirements, question.production_kinds, key, registry);
      oneOf(question.conflict_policy, ["authority_owned_no_profile_overwrite", "preserve_all_no_automatic_preference"], key + ".conflict_policy");
      exact(question.visibility, ["public", "sensitivity"], key + ".visibility");
      oneOf(question.visibility.public, ["when_resolved", "never"], key + ".visibility.public");
      string(question.visibility.sensitivity, key + ".visibility.sensitivity");
      consumer(question.consumer, key, registry.consumer_contract);
      if (["active", "complete"].includes(module.lifecycle) && question.lifecycle === "active") {
        if (question.definition_status !== "approved" || question.value_schema.status !== "approved") {
          fail(key + " cannot enter a public module with draft definitions or value schemas");
        }
      }
      result.set(key, { module, question });
    }
    if ("technical_pilot" in module) {
      for (const key of module.technical_pilot.question_keys) {
        const question = module.questions[key];
        if (!question) fail("module " + moduleId + " technical pilot references a question outside the module: " + key);
        if (question.definition_status !== "approved" || question.value_schema.status !== "approved") {
          fail("module " + moduleId + " technical pilot references a draft question contract: " + key);
        }
      }
    }
  }
  return result;
}

function consumerContract(registry, questions) {
  const contract = registry.consumer_contract;
  exact(
    contract,
    ["version", "resolution_rule", "defaults", "occurrence_defaults", "public_projection_gate", "profiles"],
    "consumer_contract",
  );
  integer(contract.version, "consumer_contract.version");
  string(contract.resolution_rule, "consumer_contract.resolution_rule");
  const expectedDefaults = {
    worker: "public_answer",
    detail: "section_fact",
    card: "off",
    search: "off",
    facet: "off",
    comparison: "off",
    map: "off",
    safety_banner: "off",
    site_tool: "on_demand",
  };
  const expectedOccurrenceDefaults = {
    worker: "off",
    detail: "off",
    card: "off",
    search: "off",
    facet: "off",
    comparison: "off",
    map: "off",
    safety_banner: "off",
    site_tool: "omit",
  };
  const expectedPublicGate = {
    allowed_module_lifecycle: ["active", "complete"],
    surface_target_scopes: {
      worker: "species",
      detail: "species",
      card: "species",
      search: "species",
      facet: "species",
      comparison: "species",
      map: "occurrence",
      safety_banner: "species",
      site_tool: "species",
    },
    required_question_visibility: "when_resolved",
    withheld_value_projection: "forbidden",
    withheld_state_projection: "public_safe_reason_only",
    public_redaction_before_every_surface: true,
    unresolved_conflict_projection: "state_and_public_claims_without_silent_preference",
    specimen_to_species_projection: "requires_reviewed_species_resolution",
    relationship_to_species_projection: "requires_reviewed_species_endpoint_resolution",
    occurrence: {
      required_collection_definition_status: "approved",
      required_lifecycle_state: "active",
      minimum_review_state: "reviewed",
      excluded_sensitivity_policies: ["withheld"],
      publication_transform_before_every_surface: true,
      untransformed_geometry_projection: "forbidden",
    },
  };
  exactValue(contract.defaults, expectedDefaults, "consumer_contract.defaults");
  exactValue(contract.occurrence_defaults, expectedOccurrenceDefaults, "consumer_contract.occurrence_defaults");
  exactValue(contract.public_projection_gate, expectedPublicGate, "consumer_contract.public_projection_gate");
  exact(contract.profiles, SURFACES, "consumer_contract.profiles");
  for (const surface of SURFACES) {
    object(contract.profiles[surface], "consumer profiles " + surface);
    const profile = string(contract.defaults[surface], "consumer default " + surface);
    if (!(profile in contract.profiles[surface])) fail("consumer default profile does not exist: " + surface);
    const occurrenceProfile = string(contract.occurrence_defaults[surface], "occurrence consumer default " + surface);
    if (!(occurrenceProfile in contract.profiles[surface])) fail("occurrence consumer default profile does not exist: " + surface);
    Object.entries(contract.profiles[surface]).forEach(([name, value]) => object(value, "consumer profile " + surface + "." + name));
  }
  for (const surface of SURFACES.filter((surface) => surface !== "site_tool")) {
    exactValue(contract.profiles[surface].off, { enabled: false }, "consumer profile " + surface + ".off");
  }
  exactValue(contract.profiles.site_tool.omit, { enabled: false }, "consumer profile site_tool.omit");
  exactValue(
    contract.profiles.worker.public_answer,
    {
      answer_source: "public_resolution",
      include_state: true,
      include_conflict: true,
      evidence: "summary",
      public_redaction: "required",
    },
    "consumer profile worker.public_answer",
  );
  exactValue(contract.profiles.detail.off, { enabled: false }, "consumer profile detail.off");
  exactValue(
    contract.profiles.facet.reviewed_positive,
    {
      enabled: true,
      kind: "reviewed_positive",
      count_basis: "mineral",
      minimum_review_state: "reviewed",
      coverage_denominator: "visible",
      missing_is_negative: false,
    },
    "consumer profile facet.reviewed_positive",
  );
  exactValue(
    contract.profiles.map.geometry,
    { enabled: true, reviewed_public_geometry_only: true },
    "consumer profile map.geometry",
  );
  exactValue(
    contract.profiles.map.geometry_kind,
    { enabled: true, role: "geometry_discriminator_only" },
    "consumer profile map.geometry_kind",
  );
  exactValue(
    contract.profiles.map.sensitivity,
    { enabled: true, enforce_redaction: true },
    "consumer profile map.sensitivity",
  );
  exactValue(
    contract.profiles.search.occurrence_name,
    {
      enabled: true,
      modes: ["exact", "prefix", "normalized_fulltext"],
      boost: 30,
      answer_source: "reviewed_public_occurrence",
    },
    "consumer profile search.occurrence_name",
  );
  const expectedHazardTrigger = {
    enabled: true,
    namespace: "safety",
    minimum_review_state: "reviewed",
    affirmative_resolved_only: true,
    missing_is_safe: false,
  };
  exactValue(contract.profiles.safety_banner.hazard_trigger, expectedHazardTrigger, "consumer profile safety_banner.hazard_trigger");
  exactValue(contract.profiles.safety_banner.restriction_trigger, expectedHazardTrigger, "consumer profile safety_banner.restriction_trigger");
  exactValue(
    contract.profiles.safety_banner.control_contributor,
    { enabled: true, trigger: false, minimum_review_state: "reviewed" },
    "consumer profile safety_banner.control_contributor",
  );
  if (contract.profiles.site_tool.critical_when_affirmative.activation !== "affirmative_resolved_only") {
    fail("consumer profile site_tool.critical_when_affirmative has an unsafe activation rule");
  }
  const cardSlots = new Map();
  for (const [key, { question }] of questions) {
    for (const surface of SURFACES) {
      if (!resolved(question, surface, registry).profile) fail(key + " cannot resolve " + surface);
      const projection = resolved(question, surface, registry);
      if (projection.profile.enabled === false) continue;
      const targetScope = contract.public_projection_gate.surface_target_scopes[surface];
      if (
        targetScope === "species" &&
        !question.subject_scopes.includes("species") &&
        !(question.subject_scopes.includes("relationship") && question.value_schema.value_kind === "material_relation")
      ) {
        fail(key + " projects to a mineral surface without species scope");
      }
      if (
        targetScope === "occurrence" &&
        !question.subject_scopes.includes("occurrence") &&
        question.value_schema.value_kind !== "occurrence_relation"
      ) {
        fail(key + " projects to an occurrence surface without occurrence scope");
      }
    }
    const card = resolved(question, "card", registry).profile;
    if (card.enabled) {
      string(card.slot, key + " resolved card slot");
      if (cardSlots.has(card.slot)) fail("card slot collision: " + card.slot);
      cardSlots.set(card.slot, key);
      if (question.value_schema.cardinality === "repeatable" && !Number.isSafeInteger(card.maximum_items)) {
        fail("repeatable card answer lacks maximum_items: " + key);
      }
    }
    const facet = resolved(question, "facet", registry).name;
    if (
      facet !== "off" &&
      !["controlled_term", "measurement", "range", "year_or_date"].includes(question.value_schema.value_kind) &&
      facet !== "reviewed_positive"
    ) {
      fail(key + " facets a non-controlled/non-canonical value");
    }
    if (question.value_schema.value_kind === "structured") {
      for (const surface of ["search", "facet", "comparison"]) {
        const projection = resolved(question, surface, registry);
        if (projection.profile.enabled === false) continue;
        if (surface === "facet" && projection.name === "reviewed_positive") {
          if (!("affirmative_discriminator" in question.value_schema)) {
            fail(key + " needs an affirmative discriminator for reviewed_positive");
          }
        } else if (!(surface in (question.value_schema.projection_selectors ?? {}))) {
          fail(key + " needs a " + surface + " projection selector for its structured value");
        }
      }
    }
    const map = resolved(question, "map", registry).name;
    if (map !== "off" && !question.subject_scopes.includes("occurrence") && question.value_schema.value_kind !== "occurrence_relation") {
      fail(key + " enables a map outside occurrence scope");
    }
  }
  if (resolved(questions.get("authority.discovery_country").question, "map", registry).name !== "off") {
    fail("authority.discovery_country must not drive the map");
  }
  if (resolved(questions.get("physical.radioactivity").question, "safety_banner", registry).name !== "off") {
    fail("physical.radioactivity must not drive a safety banner");
  }
  for (const [key, { question }] of questions) {
    const banner = resolved(question, "safety_banner", registry);
    if (banner.name !== "off" && !key.startsWith("safety.")) {
      fail("non-safety question drives a safety banner: " + key);
    }
    if (
      banner.profile.enabled !== false &&
      banner.profile.trigger !== false &&
      !("affirmative_discriminator" in question.value_schema)
    ) {
      fail(key + " needs an affirmative discriminator for its safety trigger");
    }
    if (
      resolved(question, "site_tool", registry).name === "critical_when_affirmative" &&
      !("affirmative_discriminator" in question.value_schema)
    ) {
      fail(key + " needs an affirmative discriminator for conditional tool activation");
    }
  }
}

function occurrenceCollection(registry, documentedFields) {
  exact(registry.collections, ["material_occurrences"], "collections");
  const collection = registry.collections.material_occurrences;
  exact(
    collection,
    ["module", "subject_scope", "source_requirement", "identity", "cardinality", "definition_version", "definition_status", "fields"],
    "material_occurrences",
  );
  if (!(collection.module in registry.modules)) fail("occurrence collection has unknown module");
  if (collection.subject_scope !== "occurrence") fail("occurrence collection must be occurrence-scoped");
  if (!(collection.source_requirement in registry.source_requirements)) fail("occurrence collection has unknown source requirement");
  if (collection.cardinality !== "repeatable") fail("occurrence collection must be repeatable");
  integer(collection.definition_version, "material_occurrences.definition_version");
  oneOf(collection.definition_status, ["draft", "approved"], "material_occurrences.definition_status");
  const fieldNames = Object.keys(object(collection.fields, "occurrence fields")).sort();
  const documentNames = [...documentedFields.keys()].sort();
  if (fieldNames.length !== documentNames.length || fieldNames.some((name, index) => name !== documentNames[index])) {
    fail("occurrence registry fields drift from questionnaire Markdown");
  }
  const orders = new Set();
  const geometryProducers = [];
  for (const [key, field] of Object.entries(collection.fields)) {
    if (!FIELD_KEY.test(key)) fail("invalid occurrence field key " + key);
    allowed(
      field,
      ["order", "value_kind", "requirement", "canonical_unit", "description", "consumer", "value_schema"],
      ["order", "value_kind", "requirement", "description", "consumer"],
      "occurrence field " + key,
    );
    integer(field.order, "occurrence field " + key + ".order");
    if (orders.has(field.order)) fail("duplicate occurrence field order " + field.order);
    orders.add(field.order);
    oneOf(field.value_kind, registry.axes.value_kind, "occurrence field " + key + ".value_kind");
    oneOf(field.requirement, ["required", "optional", "conditional"], "occurrence field " + key + ".requirement");
    string(field.description, "occurrence field " + key + ".description");
    consumer(field.consumer, "occurrence field " + key, registry.consumer_contract);
    if ("value_schema" in field) {
      valueSchema(field.value_schema, "material_occurrences." + key, registry);
      if (field.value_schema.value_kind !== field.value_kind) fail("occurrence field " + key + " value kind drifts from its schema");
    }
    for (const surface of SURFACES) {
      if (!resolved(field, surface, registry, registry.consumer_contract.occurrence_defaults).profile) {
        fail("occurrence field " + key + " cannot resolve " + surface);
      }
    }
    if (resolved(field, "map", registry, registry.consumer_contract.occurrence_defaults).name === "geometry") {
      geometryProducers.push(key);
    }
  }
  exactStringValues(geometryProducers, ["geometry"], "occurrence geometry producers");
  if (resolved(collection.fields.geometry_kind, "map", registry, registry.consumer_contract.occurrence_defaults).name !== "geometry_kind") {
    fail("occurrence geometry_kind must remain a discriminator, not a geometry producer");
  }
  for (const fieldName of ["publication_transform", "sensitivity_policy"]) {
    if (resolved(collection.fields[fieldName], "map", registry, registry.consumer_contract.occurrence_defaults).name !== "sensitivity") {
      fail("occurrence " + fieldName + " must enforce map sensitivity");
    }
  }
  const moduleLifecycle = registry.modules[collection.module].lifecycle;
  if (["active", "complete"].includes(moduleLifecycle) && collection.definition_status !== "approved") {
    fail("material_occurrences cannot enter a public module with a draft collection definition");
  }
  if (collection.definition_status === "approved") {
    for (const [key, field] of Object.entries(collection.fields)) {
      if (!("value_schema" in field) || field.value_schema.status !== "approved") {
        fail("approved material_occurrences requires an approved value schema for " + key);
      }
    }
    const requiredVocabularies = {
      lifecycle_state: "occurrence_lifecycle_state_v1",
      sensitivity_policy: "occurrence_sensitivity_policy_v1",
    };
    for (const [fieldName, vocabularyId] of Object.entries(requiredVocabularies)) {
      const schema = collection.fields[fieldName].value_schema;
      if (schema.value_kind !== "controlled_term" || schema.vocabulary.id !== vocabularyId || schema.vocabulary.status !== "approved") {
        fail("approved material_occurrences has an unsafe " + fieldName + " vocabulary");
      }
    }
    const selectorRequirements = {
      evidence: { review_state: { path: "/review_state" } },
      publication_transform: {
        action: { path: "/action" },
        public_geometry: { path: "/public_geometry" },
        reason: { path: "/reason" },
      },
    };
    for (const [fieldName, expectedSelectors] of Object.entries(selectorRequirements)) {
      exactValue(
        collection.fields[fieldName].value_schema.gate_selectors,
        expectedSelectors,
        "material_occurrences." + fieldName + ".value_schema.gate_selectors",
      );
    }
  }
}

function validateRegistry(registry, documented) {
  exact(
    registry,
    ["format", "schema_version", "questionnaire_version", "registry_revision", "registry_id", "status", "human_contract", "population", "canonicalization", "axes", "tier_policies", "source_requirements", "applicability_rules", "normalization_policies", "consumer_contract", "modules", "collections", "legacy_mappings", "validation_invariants"],
    "questionnaire registry",
  );
  if (registry.format !== "waajacu-mineral-record-questionnaire-registry") fail("invalid registry format");
  if (registry.schema_version !== 1 || registry.questionnaire_version !== 1) fail("unsupported registry version");
  integer(registry.registry_revision, "registry_revision");
  if (registry.registry_id !== "waajacu-mineral-record-questionnaire-v1") fail("invalid registry_id");
  oneOf(registry.status, ["draft_for_pilot_validation", "approved"], "registry.status");
  if (registry.human_contract !== DOCUMENT_FILE) fail("invalid human_contract path");
  exact(registry.population, ["kind", "release_id", "mineral_count", "identity_policy"], "population");
  if (!SHA256.test(registry.population.release_id)) fail("invalid population release_id");
  if (registry.population.mineral_count !== 6226 || registry.population.identity_policy !== "fixed") {
    fail("population must remain the fixed 6,226-mineral release");
  }
  exact(registry.canonicalization, ["algorithm", "object_keys", "arrays", "unicode", "digest", "self_digest"], "canonicalization");
  if (
    registry.canonicalization.algorithm !== "waajacu-canonical-json-v1" ||
    registry.canonicalization.object_keys !== "utf8_lexicographic" ||
    registry.canonicalization.arrays !== "preserve" ||
    registry.canonicalization.unicode !== "must_be_nfc" ||
    registry.canonicalization.digest !== "sha256" ||
    registry.canonicalization.self_digest !== "forbidden"
  ) {
    fail("canonicalization policy is invalid");
  }
  exact(registry.axes, ["subject_scope", "value_kind", "production_kind", "priority_tier", "module_lifecycle", "question_lifecycle", "coverage_state", "claim_review_state", "record_verification_state", "resolution_state", "source_decision"], "axes");
  const expectedAxes = {
    subject_scope: ["species", "occurrence", "specimen", "relationship"],
    value_kind: ["text", "controlled_term", "boolean", "integer", "year_or_date", "decimal", "range", "measurement", "formula", "identifier", "structured", "material_relation", "occurrence_relation"],
    production_kind: ["sourced", "derived", "editorial"],
    priority_tier: ["core", "conditional", "extended", "deferred"],
    module_lifecycle: ["not_launched", "pilot", "active", "complete", "superseded"],
    question_lifecycle: ["active", "deprecated", "superseded"],
    coverage_state: ["value_present", "not_yet_researched", "not_reported_by_reviewed_sources", "not_applicable", "withheld"],
    claim_review_state: ["unreviewed", "reviewed", "verified", "disputed"],
    record_verification_state: ["draft", "generated", "sourced", "reviewed", "verified", "disputed"],
    resolution_state: ["unresolved", "single_supported", "multiple_consistent", "conflicting", "preferred_with_conflict"],
    source_decision: ["admitted", "pilot_only", "private_only", "deferred", "rejected"],
  };
  Object.entries(expectedAxes).forEach(([name, values]) => exactStringValues(registry.axes[name], values, "axis " + name));
  exact(registry.tier_policies, registry.axes.priority_tier, "tier_policies");
  for (const [tier, policy] of Object.entries(registry.tier_policies)) {
    exact(policy, ["applicability_evaluation", "coverage_required_when_module"], "tier policy " + tier);
    string(policy.applicability_evaluation, "tier policy " + tier + ".applicability_evaluation");
    uniqueStrings(policy.coverage_required_when_module, "tier policy " + tier + ".coverage_required_when_module", true);
    policy.coverage_required_when_module.forEach((state) => oneOf(state, registry.axes.module_lifecycle, "tier policy " + tier));
  }
  sourceRequirements(registry);
  object(registry.applicability_rules, "applicability_rules");
  exact(registry.applicability_rules, ["default_question_applicability_v1", "conditional_by_reviewed_evidence_v1"], "applicability_rules");
  for (const [id, rule] of Object.entries(registry.applicability_rules)) {
    exact(rule, ["version", "status", "kind", "description"], "applicability rule " + id);
    integer(rule.version, "applicability rule " + id + ".version");
    oneOf(rule.status, ["draft", "approved"], "applicability rule " + id + ".status");
    string(rule.kind, "applicability rule " + id + ".kind");
    string(rule.description, "applicability rule " + id + ".description");
  }
  object(registry.normalization_policies, "normalization_policies");
  exact(registry.normalization_policies, ["preserve_raw_and_normalize_v1"], "normalization_policies");
  const normalization = registry.normalization_policies.preserve_raw_and_normalize_v1;
  exact(
    normalization,
    ["version", "status", "preserve_raw_value", "preserve_conditions", "preserve_uncertainty", "transformations_versioned", "empty_placeholder_values_forbidden"],
    "normalization policy preserve_raw_and_normalize_v1",
  );
  integer(normalization.version, "normalization policy version");
  oneOf(normalization.status, ["draft", "approved"], "normalization policy status");
  for (const name of ["preserve_raw_value", "preserve_conditions", "preserve_uncertainty", "transformations_versioned", "empty_placeholder_values_forbidden"]) {
    if (normalization[name] !== true) fail("normalization policy " + name + " must remain true");
  }
  if (Object.keys(registry.modules).length !== 11) fail("registry must contain exactly 11 modules");
  const questions = flatten(registry);
  if (questions.size !== 91) fail("registry must contain exactly 91 questions");
  for (const [key, row] of documented.questions) {
    const registered = questions.get(key);
    if (!registered) fail("registry is missing question " + key);
    if (registered.module.label !== row.section) fail(key + " is assigned to the wrong module");
    if (registered.question.question !== row.question) fail(key + " question text drifts from Markdown");
    if (registered.question.priority_tier !== row.tier) fail(key + " tier drifts from Markdown");
    if (registered.question.value_schema.shape !== row.shape) fail(key + " shape drifts from Markdown");
    const codes = Object.values(registered.question.requirements).flatMap((requirement) => requirement.codes).sort();
    const expected = [...row.sources].sort();
    if (codes.length !== expected.length || codes.some((code, index) => code !== expected[index])) {
      fail(key + " source requirements drift from Markdown");
    }
  }
  for (const [key, { question }] of questions) {
    if (
      (key.startsWith("identity.") || key.startsWith("authority.") || ["identifiers.ima_number", "identifiers.ima_symbol"].includes(key)) &&
      !Object.values(question.requirements).some((requirement) => requirement.codes.includes("AUTH1"))
    ) {
      fail("protected question lacks AUTH1: " + key);
    }
  }
  consumerContract(registry, questions);
  occurrenceCollection(registry, documented.occurrenceFields);
  if (registry.status === "approved") {
    for (const [id, rule] of Object.entries(registry.applicability_rules)) {
      if (rule.status !== "approved") fail("approved registry has draft applicability rule " + id);
    }
    if (normalization.status !== "approved") fail("approved registry has a draft normalization policy");
    for (const [key, { question }] of questions) {
      if (question.definition_status !== "approved" || question.value_schema.status !== "approved") {
        fail("approved registry has a draft question contract: " + key);
      }
    }
    if (registry.collections.material_occurrences.definition_status !== "approved") {
      fail("approved registry has a draft occurrence collection");
    }
  }
  if (Object.keys(object(registry.legacy_mappings, "legacy_mappings")).length !== 16) {
    fail("registry must contain all 16 legacy dispositions");
  }
  for (const [id, mapping] of Object.entries(registry.legacy_mappings)) {
    exact(mapping, ["sources", "action", "targets", "guard", "preserve_legacy", "scientific_status_effect"], "legacy mapping " + id);
    uniqueStrings(mapping.sources, "legacy mapping " + id + ".sources");
    uniqueStrings(mapping.targets, "legacy mapping " + id + ".targets", true);
    mapping.targets.forEach((target) => {
      if (!questions.has(target)) fail("legacy mapping " + id + " has unknown target " + target);
    });
    oneOf(mapping.action, ["direct_projection", "reviewed_migration", "split_reviewed_migration", "retain_legacy", "deprecate"], "legacy mapping " + id + ".action");
    if (mapping.preserve_legacy !== true || mapping.scientific_status_effect !== "preserve") {
      fail("legacy mapping may discard or upgrade data: " + id);
    }
  }
  exactStringValues(
    registry.validation_invariants,
    [
      "human_registry_question_parity_v1",
      "closed_objects_and_enums_v1",
      "nfc_strings_v1",
      "global_question_key_uniqueness_v1",
      "unique_module_and_question_order_v1",
      "all_policy_requirement_profile_and_legacy_targets_resolve_v1",
      "production_requirement_kind_compatibility_v1",
      "controlled_terms_declare_versioned_vocabularies_v1",
      "measurements_and_ranges_declare_canonical_units_v1",
      "public_facets_use_controlled_or_canonical_values_v1",
      "protected_identity_requires_AUTH1_v1",
      "discovery_country_map_forbidden_v1",
      "physical_radioactivity_banner_forbidden_v1",
      "safety_banner_requires_reviewed_affirmative_resolution_v1",
      "not_launched_creates_no_per_record_missing_rows_v1",
      "withheld_never_enters_public_projection_v1",
      "legacy_migration_never_upgrades_scientific_status_v1",
      "public_module_lifecycle_gate_v1",
      "consumer_profile_security_constants_v1",
      "occurrence_public_projection_gate_v1",
      "structured_consumer_selectors_v1",
      "specimen_species_projection_requires_review_v1",
      "source_claim_contract_matches_ownership_v1",
      "source_requirement_class_compatibility_v1",
      "source_matrix_registry_digest_binding_v1",
      "pilot_adapter_prerequisite_gates_v1",
      "technical_adapter_contract_scope_v1",
    ],
    "validation_invariants",
  );
  return questions;
}

function matrixEntry(id, entry, matrix, registry, questions, occurrenceFields) {
  if (!FIELD_KEY.test(id)) fail("invalid source entry key " + id);
  exact(
    entry,
    ["order", "source_id", "question_group", "publisher", "work", "release", "intended_ownership", "authority_fit", "claim_contract", "identity_crosswalk", "coverage_expectation", "format_access", "locator_quality", "units_definitions", "update_semantics", "rights", "reproducibility", "conflict_policy", "review_burden", "gate_status", "decision", "decision_reason"],
    "source entry " + id,
  );
  integer(entry.order, id + ".order");
  string(entry.source_id, id + ".source_id");
  string(entry.question_group, id + ".question_group");
  exact(entry.publisher, ["name", "canonical_url"], id + ".publisher");
  string(entry.publisher.name, id + ".publisher.name");
  https(entry.publisher.canonical_url, id + ".publisher.canonical_url");
  exact(entry.work, ["title", "canonical_url"], id + ".work");
  string(entry.work.title, id + ".work.title");
  https(entry.work.canonical_url, id + ".work.canonical_url");
  exact(entry.release, ["version", "released_at", "snapshot_kind"], id + ".release");
  Object.entries(entry.release).forEach(([name, value]) => string(value, id + ".release." + name));
  exact(entry.intended_ownership, ["question_keys", "collection_fields", "excluded_uses"], id + ".intended_ownership");
  uniqueStrings(entry.intended_ownership.question_keys, id + ".question_keys", true);
  uniqueStrings(entry.intended_ownership.collection_fields, id + ".collection_fields", true);
  uniqueStrings(entry.intended_ownership.excluded_uses, id + ".excluded_uses");
  entry.intended_ownership.question_keys.forEach((key) => {
    if (!questions.has(key)) fail(id + " references unknown question " + key);
  });
  entry.intended_ownership.collection_fields.forEach((key) => {
    if (!(key in occurrenceFields)) fail(id + " references unknown occurrence field " + key);
  });
  uniqueStrings(entry.authority_fit, id + ".authority_fit");
  exact(entry.claim_contract, ["subject_scopes", "value_kinds", "cardinality"], id + ".claim_contract");
  uniqueStrings(entry.claim_contract.subject_scopes, id + ".subject_scopes", true);
  entry.claim_contract.subject_scopes.forEach((scope) => oneOf(scope, registry.axes.subject_scope, id + ".subject_scopes"));
  uniqueStrings(entry.claim_contract.value_kinds, id + ".value_kinds", true);
  entry.claim_contract.value_kinds.forEach((kind) => oneOf(kind, registry.axes.value_kind, id + ".value_kinds"));
  oneOf(entry.claim_contract.cardinality, ["single", "repeatable", "not_applicable"], id + ".cardinality");
  const expectedScopes = new Set();
  const expectedKinds = new Set();
  let expectedCardinality = "not_applicable";
  const sourceRequirementMatches = (requirement, label) => {
    const matchesCode = (code) => {
      const allowedClasses = registry.source_requirements[code].allowed_source_classes ?? [];
      return allowedClasses.some((sourceClass) => entry.authority_fit.includes(sourceClass));
    };
    const matches = requirement.mode === "all" ? requirement.codes.every(matchesCode) : requirement.codes.some(matchesCode);
    if (!matches) fail(id + " authority_fit cannot satisfy " + label + " source requirements");
  };
  for (const key of entry.intended_ownership.question_keys) {
    const question = questions.get(key).question;
    question.subject_scopes.forEach((scope) => expectedScopes.add(scope));
    expectedKinds.add(question.value_schema.value_kind);
    if (expectedCardinality === "not_applicable") expectedCardinality = "single";
    if (question.value_schema.cardinality === "repeatable") expectedCardinality = "repeatable";
    if (!("sourced" in question.requirements)) fail(id + " claims source ownership of non-sourced question " + key);
    sourceRequirementMatches(question.requirements.sourced, key);
  }
  if (entry.intended_ownership.collection_fields.length > 0) {
    expectedScopes.add("occurrence");
    expectedCardinality = "repeatable";
    entry.intended_ownership.collection_fields.forEach((key) => expectedKinds.add(occurrenceFields[key].value_kind));
    sourceRequirementMatches(
      { mode: "all", codes: [registry.collections.material_occurrences.source_requirement] },
      "material_occurrences",
    );
  }
  sameStringSet(entry.claim_contract.subject_scopes, [...expectedScopes], id + ".claim_contract.subject_scopes");
  sameStringSet(entry.claim_contract.value_kinds, [...expectedKinds], id + ".claim_contract.value_kinds");
  if (entry.claim_contract.cardinality !== expectedCardinality) {
    fail(id + ".claim_contract.cardinality does not match owned fields");
  }
  exact(entry.identity_crosswalk, ["source_record_key", "method", "status", "fuzzy_matching"], id + ".identity_crosswalk");
  if (entry.identity_crosswalk.fuzzy_matching !== "forbidden") fail(id + " permits fuzzy matching");
  ["source_record_key", "method", "status"].forEach((name) => string(entry.identity_crosswalk[name], id + ".identity_crosswalk." + name));
  exact(entry.coverage_expectation, ["denominator_rule", "useful_yield_gate", "minimum_useful_yield_percent"], id + ".coverage_expectation");
  string(entry.coverage_expectation.denominator_rule, id + ".denominator_rule");
  string(entry.coverage_expectation.useful_yield_gate, id + ".useful_yield_gate");
  const yieldPercent = entry.coverage_expectation.minimum_useful_yield_percent;
  if (typeof yieldPercent !== "number" || yieldPercent < 0 || yieldPercent > 100) fail(id + " has invalid useful-yield percentage");
  exact(entry.format_access, ["formats", "access", "machine_readable", "extraction_risks"], id + ".format_access");
  uniqueStrings(entry.format_access.formats, id + ".formats");
  string(entry.format_access.access, id + ".format_access.access");
  if (typeof entry.format_access.machine_readable !== "boolean") fail(id + ".machine_readable must be boolean");
  uniqueStrings(entry.format_access.extraction_risks, id + ".extraction_risks");
  exact(entry.locator_quality, ["level", "locators"], id + ".locator_quality");
  string(entry.locator_quality.level, id + ".locator_quality.level");
  uniqueStrings(entry.locator_quality.locators, id + ".locator_quality.locators");
  exact(entry.units_definitions, ["status", "policy"], id + ".units_definitions");
  string(entry.units_definitions.status, id + ".units_definitions.status");
  string(entry.units_definitions.policy, id + ".units_definitions.policy");
  exact(entry.update_semantics, ["cadence", "correction_behavior", "deletion_behavior", "required_action"], id + ".update_semantics");
  Object.entries(entry.update_semantics).forEach(([name, value]) => string(value, id + ".update_semantics." + name));
  exact(entry.rights, ["status", "license_expression", "license_url", "public_redistribution", "attribution", "changes_notice", "restrictions", "checked_on", "evidence_urls"], id + ".rights");
  ["status", "license_expression", "public_redistribution", "attribution", "changes_notice", "checked_on"].forEach((name) => string(entry.rights[name], id + ".rights." + name));
  https(entry.rights.license_url, id + ".rights.license_url");
  uniqueStrings(entry.rights.restrictions, id + ".rights.restrictions");
  uniqueStrings(entry.rights.evidence_urls, id + ".rights.evidence_urls");
  entry.rights.evidence_urls.forEach((url, index) => https(url, id + ".rights.evidence_urls[" + index + "]"));
  allowed(entry.reproducibility, ["status", "retrieved_at", "artifact_sha256", "records_sha256", "parser", "parser_code_revision", "parser_configuration_sha256", "approved_batch_id", "report_sha256", "required_snapshot"], ["status", "retrieved_at", "artifact_sha256", "parser"], id + ".reproducibility");
  Object.entries(entry.reproducibility).forEach(([name, value]) => string(value, id + ".reproducibility." + name));
  string(entry.conflict_policy, id + ".conflict_policy");
  exact(entry.review_burden, ["extraction", "scientific", "required_review"], id + ".review_burden");
  Object.entries(entry.review_burden).forEach(([name, value]) => string(value, id + ".review_burden." + name));
  exact(entry.gate_status, matrix.hard_gates, id + ".gate_status");
  Object.entries(entry.gate_status).forEach(([gate, status]) => oneOf(status, ["pass", "conditional", "not_yet_met", "fail"], id + ".gate_status." + gate));
  oneOf(entry.decision, registry.axes.source_decision, id + ".decision");
  string(entry.decision_reason, id + ".decision_reason");
  const gates = Object.values(entry.gate_status);
  const ownsFields = entry.intended_ownership.question_keys.length + entry.intended_ownership.collection_fields.length > 0;
  if (entry.decision === "admitted") {
    if (gates.some((status) => status !== "pass")) fail("admitted source has an unpassed gate: " + id);
    if (!ownsFields) fail("admitted source has no field ownership: " + id);
    if (entry.reproducibility.status !== "captured_and_approved" || !SHA256.test(entry.reproducibility.artifact_sha256)) {
      fail("admitted source lacks an approved frozen artifact: " + id);
    }
  }
  if (entry.decision === "pilot_only" && gates.includes("fail")) fail("pilot-only source has a failed gate: " + id);
  if (entry.decision === "pilot_only" && !ownsFields) fail("pilot-only source has no bounded field ownership: " + id);
  if (entry.decision === "private_only" && entry.gate_status.rights !== "fail") fail("private-only source lacks failed rights gate: " + id);
  if (entry.decision === "rejected" && !gates.includes("fail")) fail("rejected source lacks a failed gate: " + id);
}

function validateMatrix(matrix, registry, questions) {
  exact(
    matrix,
    ["format", "schema_version", "matrix_revision", "status", "reviewed_on", "questionnaire_registry", "questionnaire_id", "questionnaire_registry_revision", "questionnaire_registry_sha256", "population_release_id", "decision_definitions", "hard_gates", "policy", "entries"],
    "source matrix",
  );
  if (matrix.format !== "waajacu-mineral-source-admission-matrix" || matrix.schema_version !== 1) fail("invalid source matrix format/version");
  integer(matrix.matrix_revision, "matrix_revision");
  oneOf(matrix.status, ["draft_for_pilot_validation", "approved"], "matrix.status");
  string(matrix.reviewed_on, "matrix.reviewed_on");
  if (
    matrix.questionnaire_registry !== REGISTRY_FILE ||
    matrix.questionnaire_id !== registry.registry_id ||
    matrix.questionnaire_registry_revision !== registry.registry_revision ||
    matrix.questionnaire_registry_sha256 !== canonicalSha256(registry) ||
    matrix.population_release_id !== registry.population.release_id
  ) {
    fail("source matrix does not bind the questionnaire registry/population");
  }
  integer(matrix.questionnaire_registry_revision, "questionnaire_registry_revision");
  if (!SHA256.test(matrix.questionnaire_registry_sha256)) fail("invalid questionnaire_registry_sha256");
  if (matrix.status === "approved" && registry.status !== "approved") fail("approved source matrix requires an approved questionnaire registry");
  exact(matrix.decision_definitions, registry.axes.source_decision, "decision_definitions");
  Object.entries(matrix.decision_definitions).forEach(([decision, definition]) => string(definition, "decision definition " + decision));
  exactStringValues(matrix.hard_gates, ["publisher_work_release", "rights", "stable_identity", "locators", "units_and_semantics", "update_behavior", "reproducibility", "field_ownership"], "hard_gates");
  exact(
    matrix.policy,
    ["public_claim_source_decisions", "private_pilot_source_decisions", "adapter_work_forbidden_decisions", "pilot_adapter_prerequisite_gates", "non_pass_gate_blocks_public_ingestion", "weighted_scores_cannot_override_hard_gates", "public_download_is_not_a_reuse_grant"],
    "matrix.policy",
  );
  ["public_claim_source_decisions", "private_pilot_source_decisions", "adapter_work_forbidden_decisions"].forEach((name) => {
    uniqueStrings(matrix.policy[name], "matrix.policy." + name);
    matrix.policy[name].forEach((decision) => oneOf(decision, registry.axes.source_decision, "matrix.policy." + name));
  });
  exactStringValues(matrix.policy.public_claim_source_decisions, ["admitted"], "matrix.policy.public_claim_source_decisions");
  exactStringValues(matrix.policy.private_pilot_source_decisions, ["admitted", "pilot_only"], "matrix.policy.private_pilot_source_decisions");
  exactStringValues(matrix.policy.adapter_work_forbidden_decisions, ["private_only", "deferred", "rejected"], "matrix.policy.adapter_work_forbidden_decisions");
  exactStringValues(
    matrix.policy.pilot_adapter_prerequisite_gates,
    ["publisher_work_release", "rights", "stable_identity", "field_ownership"],
    "matrix.policy.pilot_adapter_prerequisite_gates",
  );
  ["non_pass_gate_blocks_public_ingestion", "weighted_scores_cannot_override_hard_gates", "public_download_is_not_a_reuse_grant"].forEach((name) => {
    if (matrix.policy[name] !== true) fail("matrix.policy." + name + " must remain true");
  });
  object(matrix.entries, "matrix.entries");
  const orders = new Set();
  const fields = registry.collections.material_occurrences.fields;
  for (const [id, entry] of Object.entries(matrix.entries)) {
    matrixEntry(id, entry, matrix, registry, questions, fields);
    if (orders.has(entry.order)) fail("duplicate source matrix order " + entry.order);
    orders.add(entry.order);
  }
  for (const [moduleId, module] of Object.entries(registry.modules)) {
    if (!("technical_pilot" in module)) continue;
    const pilot = module.technical_pilot;
    const entry = matrix.entries[pilot.source_matrix_entry];
    if (!entry) fail("module " + moduleId + " technical pilot references an unknown source matrix entry");
    sameStringSet(pilot.question_keys, entry.intended_ownership.question_keys, "module " + moduleId + ".technical_pilot.question_keys");
    if (!matrix.policy.private_pilot_source_decisions.includes(entry.decision)) {
      fail("module " + moduleId + " technical pilot source is not eligible for private pilot work");
    }
  }
}

export async function validateMineralContentContracts(rootDirectory) {
  const root = path.resolve(rootDirectory);
  const [registry, matrix, markdown] = await Promise.all([
    json(path.join(root, ...REGISTRY_FILE.split("/")), "questionnaire registry"),
    json(path.join(root, ...MATRIX_FILE.split("/")), "source admission matrix"),
    readFile(path.join(root, ...DOCUMENT_FILE.split("/")), "utf8"),
  ]);
  const documented = parseDocument(markdown);
  const questions = validateRegistry(registry, documented);
  validateMatrix(matrix, registry, questions);
  return {
    modules: Object.keys(registry.modules).length,
    questions: questions.size,
    occurrenceFields: Object.keys(registry.collections.material_occurrences.fields).length,
    sourceEntries: Object.keys(matrix.entries).length,
    admittedSources: Object.values(matrix.entries).filter((entry) => entry.decision === "admitted").length,
    pilotOnlySources: Object.values(matrix.entries).filter((entry) => entry.decision === "pilot_only").length,
  };
}

async function main() {
  if (process.argv.length !== 3) fail("usage: validate-mineral-content-contracts.mjs REPOSITORY_ROOT");
  const result = await validateMineralContentContracts(process.argv[2]);
  process.stdout.write(
    "Validated mineral content contracts: " +
      result.questions +
      " questions, " +
      result.occurrenceFields +
      " occurrence fields, " +
      result.sourceEntries +
      " source decisions (" +
      result.admittedSources +
      " admitted, " +
      result.pilotOnlySources +
      " pilot-only).\n",
  );
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    process.stderr.write("mineral content contract validation failed: " + error.message + "\n");
    process.exitCode = 1;
  });
}
