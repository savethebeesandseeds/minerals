#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";

const PILOT_FILE = "schemas/pilots/cod-crystallography-pilot-v1.json";
const REGISTRY_FILE = "schemas/mineral-record-questionnaire-v1.json";
const MATRIX_FILE = "schemas/mineral-source-admission-matrix-v1.json";
const DRAFT_2020_12 = "https://json-schema.org/draft/2020-12/schema";
const SHA256 = /^sha256:[0-9a-f]{64}$/;
const QUESTION_KEYS = [
  "crystallography.space_group",
  "crystallography.unit_cell",
  "crystallography.structure_references",
];

function fail(message) {
  throw new Error(message);
}

function object(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) fail(label + " must be an object");
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

function positiveInteger(value, label) {
  if (!Number.isSafeInteger(value) || value <= 0) fail(label + " must be a positive integer");
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

function exactStrings(value, expected, label) {
  const actual = array(value, label);
  if (
    actual.length !== expected.length ||
    actual.some((item, index) => typeof item !== "string" || item !== expected[index])
  ) {
    fail(label + " does not match the protected contract");
  }
  if (new Set(actual).size !== actual.length) fail(label + " contains duplicates");
}

function sameStringSet(value, expected, label) {
  const actual = [...array(value, label)].sort();
  const wanted = [...expected].sort();
  if (
    actual.length !== wanted.length ||
    actual.some((item, index) => typeof item !== "string" || item !== wanted[index])
  ) {
    fail(label + " does not match the protected set");
  }
}

function canonicalJson(value) {
  if (Array.isArray(value)) return "[" + value.map(canonicalJson).join(",") + "]";
  if (value !== null && typeof value === "object") {
    return (
      "{" +
      Object.keys(value)
        .sort()
        .map((key) => JSON.stringify(key) + ":" + canonicalJson(value[key]))
        .join(",") +
      "}"
    );
  }
  return JSON.stringify(value);
}

function canonicalSha256(value) {
  return "sha256:" + createHash("sha256").update(canonicalJson(value), "utf8").digest("hex");
}

function bytesSha256(bytes) {
  return "sha256:" + createHash("sha256").update(bytes).digest("hex");
}

function assertNfc(value, label = "value") {
  if (typeof value === "string") {
    if (value.normalize("NFC") !== value) fail(label + " is not NFC-normalized");
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((item, index) => assertNfc(item, label + "[" + index + "]"));
    return;
  }
  if (value !== null && typeof value === "object") {
    for (const [key, child] of Object.entries(value)) {
      if (key.normalize("NFC") !== key) fail(label + " has a non-NFC property name");
      assertNfc(child, label + "." + key);
    }
  }
}

function repositoryPath(root, relative, label) {
  string(relative, label);
  if (relative.includes("\\") || path.posix.isAbsolute(relative)) fail(label + " must be a repository-relative POSIX path");
  const normalized = path.posix.normalize(relative);
  if (normalized === ".." || normalized.startsWith("../")) fail(label + " escapes the repository");
  const resolved = path.resolve(root, ...normalized.split("/"));
  if (resolved !== root && !resolved.startsWith(root + path.sep)) fail(label + " escapes the repository");
  return { normalized, resolved };
}

async function jsonFile(root, relative, label) {
  const { resolved } = repositoryPath(root, relative, label + " path");
  let bytes;
  try {
    bytes = await readFile(resolved);
  } catch (error) {
    fail(label + " cannot be read: " + error.message);
  }
  let value;
  try {
    value = JSON.parse(bytes.toString("utf8"));
  } catch (error) {
    fail(label + " is not valid JSON: " + error.message);
  }
  assertNfc(value, label);
  return { bytes, value };
}

function resolveJsonPointer(document, fragment, label) {
  if (fragment === "" || fragment === "#") return document;
  if (!fragment.startsWith("#/")) fail(label + " has an unsupported JSON Pointer fragment");
  let current = document;
  for (const encoded of fragment.slice(2).split("/")) {
    const key = encoded.replace(/~1/g, "/").replace(/~0/g, "~");
    if (current === null || typeof current !== "object" || !(key in current)) {
      fail(label + " does not resolve");
    }
    current = current[key];
  }
  return current;
}

async function lintJsonSchema(root, relative, cache = new Map()) {
  const normalized = repositoryPath(root, relative, "JSON Schema").normalized;
  if (cache.has(normalized)) return cache.get(normalized);
  const pending = (async () => {
    const loaded = await jsonFile(root, normalized, "JSON Schema " + normalized);
    const schema = object(loaded.value, "JSON Schema " + normalized);
    if (schema.$schema !== DRAFT_2020_12) fail(normalized + " must declare Draft 2020-12");
    if (typeof schema.$id !== "string" || !schema.$id.startsWith("https://")) fail(normalized + " must declare an HTTPS $id");
    if (schema.type !== "object" && schema.type !== "array") fail(normalized + " must declare an object or array root");
    if (schema.type === "object" && schema.additionalProperties !== false) fail(normalized + " root object must be closed");

    const references = [];
    const visit = (node, location) => {
      if (Array.isArray(node)) {
        node.forEach((child, index) => visit(child, location + "/" + index));
        return;
      }
      if (node === null || typeof node !== "object") return;
      if (node.type === "object" && node.additionalProperties !== false) {
        fail(normalized + location + " declares an open object");
      }
      if (Array.isArray(node.required) && node.properties && typeof node.properties === "object") {
        for (const name of node.required) {
          if (!(name in node.properties)) fail(normalized + location + " requires unknown property " + name);
        }
      }
      if (Array.isArray(node.enum) && new Set(node.enum.map(canonicalJson)).size !== node.enum.length) {
        fail(normalized + location + " repeats an enum value");
      }
      if ("$ref" in node) {
        if (typeof node.$ref !== "string" || node.$ref.length === 0) fail(normalized + location + " has an invalid $ref");
        references.push({ reference: node.$ref, location });
      }
      for (const [key, child] of Object.entries(node)) visit(child, location + "/" + key.replace(/~/g, "~0").replace(/\//g, "~1"));
    };
    visit(schema, "");

    for (const { reference, location } of references) {
      if (/^[a-z][a-z0-9+.-]*:/i.test(reference)) fail(normalized + location + " uses a remote $ref");
      const hashIndex = reference.indexOf("#");
      const targetText = hashIndex === -1 ? reference : reference.slice(0, hashIndex);
      const fragment = hashIndex === -1 ? "" : reference.slice(hashIndex);
      if (targetText === "") {
        resolveJsonPointer(schema, fragment, normalized + location + " $ref");
        continue;
      }
      const target = path.posix.normalize(path.posix.join(path.posix.dirname(normalized), targetText));
      if (!target.startsWith("schemas/") || !target.endsWith(".schema.json")) {
        fail(normalized + location + " $ref escapes the versioned schema tree");
      }
      const targetSchema = await lintJsonSchema(root, target, cache);
      resolveJsonPointer(targetSchema.schema, fragment, normalized + location + " $ref");
    }
    return { bytes: loaded.bytes, schema };
  })();
  cache.set(normalized, pending);
  return pending;
}

function challengeContract(sampleDesign) {
  const counts = sampleDesign.counts;
  exact(
    counts,
    ["total", "development", "holdout", "baseline", "baseline_development", "baseline_holdout", "challenge", "challenge_development", "challenge_holdout"],
    "sample_design.counts",
  );
  const expectedCounts = {
    total: 96,
    development: 72,
    holdout: 24,
    baseline: 60,
    baseline_development: 45,
    baseline_holdout: 15,
    challenge: 36,
    challenge_development: 27,
    challenge_holdout: 9,
  };
  for (const [name, expected] of Object.entries(expectedCounts)) {
    if (counts[name] !== expected) fail("sample_design.counts." + name + " must remain " + expected);
  }
  if (counts.development + counts.holdout !== counts.total) fail("sample split does not sum to total");
  if (counts.baseline + counts.challenge !== counts.total) fail("sample kinds do not sum to total");
  if (sampleDesign.selection_manifest_status !== "pending_candidate_discovery_not_frozen") {
    fail("pilot contract must not claim that an uncreated selection manifest is frozen");
  }
  if (sampleDesign.replacement_policy !== "forbidden_after_selection_manifest_freeze") fail("sample replacement policy is unsafe");
  exactStrings(sampleDesign.query_outcomes_required_for_every_selected_mineral, ["matched", "no_candidate", "ambiguous", "excluded"], "sample query outcomes");

  const baseline = sampleDesign.baseline;
  if (
    baseline.salt !== "questionnaire-v1-baseline" ||
    baseline.hash_input !== "UTF8(salt) || 0x00 || UTF8(public_id)" ||
    baseline.ranking !== "ascending_sha256_bytes_then_ascending_utf8_public_id" ||
    baseline.development_ranks !== "1-45" ||
    baseline.holdout_ranks !== "46-60"
  ) {
    fail("baseline selection algorithm drifted");
  }

  const challenge = sampleDesign.challenge;
  if (challenge.eligibility_freeze !== "after_raw_candidate_discovery_and_before_normalized_adapter_output") fail("challenge eligibility freeze is unsafe");
  if (challenge.shortfall_behavior !== "block_and_review_the_sample_design; do_not_fill_from_another_stratum") fail("challenge shortfalls may be silently substituted");
  if (challenge.split_rule !== "within_each_group_lowest_three_split_hashes_are_holdout_and_remaining_nine_are_development") fail("challenge split rule drifted");
  const expectedGroups = {
    nomenclature_crosswalk: [
      ["accepted_name_exact_unique", 3],
      ["authority_alias_exact_unique", 3],
      ["multiple_exact_or_former_name_ambiguity", 3],
      ["no_exact_name_candidate", 3],
    ],
    value_shape_applicability: [
      ["multiple_determinations_or_data_blocks", 2],
      ["parenthesized_or_separate_uncertainty", 2],
      ["unknown_or_inapplicable_placeholder", 2],
      ["nonambient_temperature_or_pressure", 2],
      ["incomplete_six_parameter_cell", 2],
      ["conflicting_or_multiple_space_group_forms", 2],
    ],
    provenance_risk: [
      ["duplicate_or_suboptimal_relation", 3],
      ["warning_error_or_retraction_status", 3],
      ["theoretical_or_synthetic_origin", 3],
      ["missing_or_nonstandard_publication_citation", 3],
    ],
  };
  const groups = array(challenge.groups, "challenge groups");
  if (groups.length !== 3) fail("challenge design must contain exactly three groups");
  for (const [index, [groupId, strata]] of Object.entries(expectedGroups).entries()) {
    const group = groups[index];
    if (group.id !== groupId || group.count !== 12 || group.development !== 9 || group.holdout !== 3) {
      fail("challenge group contract drifted for " + groupId);
    }
    const actualStrata = array(group.strata, "challenge group " + groupId + " strata");
    if (
      actualStrata.length !== strata.length ||
      actualStrata.some((row, rowIndex) => row.id !== strata[rowIndex][0] || row.quota !== strata[rowIndex][1])
    ) {
      fail("challenge strata contract drifted for " + groupId);
    }
    if (actualStrata.reduce((sum, row) => sum + row.quota, 0) !== 12) fail("challenge strata do not sum to 12 for " + groupId);
  }
}

export async function validateCodCrystallographyPilot(rootDirectory) {
  const root = path.resolve(rootDirectory);
  const pilotLoaded = await jsonFile(root, PILOT_FILE, "COD pilot contract");
  const pilot = object(pilotLoaded.value, "COD pilot contract");
  exact(
    pilot,
    ["format", "schema_version", "contract_revision", "pilot_id", "status", "reviewed_on", "human_contract", "digest_policy", "bindings", "scope", "schemas", "source_evidence_policy", "sample_design", "crosswalk_policy", "extraction_policy", "quality_gates", "runtime_boundary", "next_stage", "official_references"],
    "COD pilot contract",
  );
  if (pilot.format !== "waajacu-cod-crystallography-pilot-contract" || pilot.schema_version !== 1 || pilot.contract_revision !== 1) fail("invalid COD pilot format/version");
  if (pilot.pilot_id !== "cod-crystallography-v1" || pilot.status !== "contract_ready_candidate_discovery_not_run") fail("invalid COD pilot identity/status");
  if (pilot.human_contract !== "docs/COD_CRYSTALLOGRAPHY_PILOT_V1.md") fail("COD pilot human contract path drifted");
  if (
    pilot.digest_policy.registry_and_matrix !== "waajacu-canonical-json-v1" ||
    pilot.digest_policy.schema_artifacts !== "sha256_of_exact_file_bytes" ||
    pilot.digest_policy.prefix !== "sha256:"
  ) {
    fail("COD pilot digest policy drifted");
  }

  const [{ value: registry }, { value: matrix }] = await Promise.all([
    jsonFile(root, REGISTRY_FILE, "questionnaire registry"),
    jsonFile(root, MATRIX_FILE, "source admission matrix"),
  ]);
  const registryBinding = pilot.bindings.questionnaire_registry;
  if (
    registryBinding.path !== REGISTRY_FILE ||
    registryBinding.id !== registry.registry_id ||
    registryBinding.revision !== registry.registry_revision ||
    registryBinding.sha256 !== canonicalSha256(registry)
  ) {
    fail("COD pilot does not bind the current questionnaire registry");
  }
  const matrixBinding = pilot.bindings.source_admission_matrix;
  if (
    matrix.questionnaire_registry !== REGISTRY_FILE ||
    matrix.questionnaire_id !== registry.registry_id ||
    matrix.questionnaire_registry_revision !== registry.registry_revision ||
    matrix.questionnaire_registry_sha256 !== canonicalSha256(registry) ||
    matrix.population_release_id !== registry.population.release_id
  ) {
    fail("source admission matrix does not bind the questionnaire registry/population");
  }
  if (
    matrixBinding.path !== MATRIX_FILE ||
    matrixBinding.entry_id !== "cod_crystal_observations" ||
    matrixBinding.revision !== matrix.matrix_revision ||
    matrixBinding.sha256 !== canonicalSha256(matrix)
  ) {
    fail("COD pilot does not bind the current source admission matrix");
  }
  if (!SHA256.test(registryBinding.sha256) || !SHA256.test(matrixBinding.sha256)) fail("COD pilot has an invalid binding digest");
  if (
    pilot.bindings.population.release_id !== registry.population.release_id ||
    pilot.bindings.population.mineral_count !== registry.population.mineral_count ||
    pilot.bindings.population.identity_policy !== "fixed_existing_public_ids_only"
  ) {
    fail("COD pilot population binding drifted");
  }

  const module = registry.modules.crystallography;
  if (!module || module.lifecycle !== "not_launched") fail("crystallography module must remain not_launched for this technical pilot");
  const technical = module.technical_pilot;
  if (!technical || technical.pilot_id !== pilot.pilot_id || technical.contract !== PILOT_FILE) fail("questionnaire technical pilot binding drifted");
  sameStringSet(technical.question_keys, QUESTION_KEYS, "questionnaire technical pilot questions");
  if (technical.coverage_rows !== "forbidden" || technical.public_projection !== "forbidden") fail("questionnaire technical pilot boundary is unsafe");
  if (
    pilot.scope.module !== "crystallography" ||
    pilot.scope.module_lifecycle !== "not_launched" ||
    pilot.scope.kind !== "private_technical_adapter_contract_without_record_coverage"
  ) {
    fail("COD pilot scope drifted");
  }
  exactStrings(pilot.scope.question_keys, QUESTION_KEYS, "COD pilot question keys");
  for (const excluded of ["create_mineral", "change_ima_identity_or_status", "create_questionnaire_coverage_rows", "public_projection"]) {
    if (!pilot.scope.excluded_mutations.includes(excluded)) fail("COD pilot no longer excludes " + excluded);
  }

  const sourceEntry = matrix.entries.cod_crystal_observations;
  if (!sourceEntry || sourceEntry.decision !== "pilot_only") fail("COD source must remain pilot_only");
  exactStrings(sourceEntry.intended_ownership.question_keys, QUESTION_KEYS, "COD source ownership");
  if (sourceEntry.gate_status.units_and_semantics !== "pass" || sourceEntry.gate_status.reproducibility !== "not_yet_met") {
    fail("COD source gates do not reflect contract-ready semantics and an unrun pilot");
  }
  if (sourceEntry.identity_crosswalk.fuzzy_matching !== "forbidden") fail("COD source matrix permits fuzzy matching");

  const schemaCache = new Map();
  const valueBindings = array(pilot.schemas.value_schemas, "value schema bindings");
  if (valueBindings.length !== QUESTION_KEYS.length) fail("COD pilot must bind exactly three value schemas");
  for (const [index, questionKey] of QUESTION_KEYS.entries()) {
    const binding = valueBindings[index];
    if (binding.question_key !== questionKey || !SHA256.test(binding.sha256)) fail("invalid value schema binding for " + questionKey);
    const loaded = await lintJsonSchema(root, binding.path, schemaCache);
    if (bytesSha256(loaded.bytes) !== binding.sha256) fail("value schema digest drifted for " + questionKey);
    const question = module.questions[questionKey];
    if (question.definition_status !== "approved" || question.value_schema.status !== "approved") fail(questionKey + " is not approved for the technical pilot");
    if (question.value_schema.cardinality !== "repeatable") fail(questionKey + " must preserve repeatable observations");
    if (question.value_schema.json_schema.$ref !== binding.path) fail(questionKey + " registry schema reference drifted");
    if (loaded.schema.type !== "array" || loaded.schema.maxItems !== question.value_schema.maximum_items) fail(questionKey + " array bound drifts from the registry");
  }
  if (module.questions["crystallography.unit_cell"].consumer.comparison !== "off") fail("unit-cell comparison requires a future multidimensional comparator");

  const expectedSupportRoles = [
    "immutable_cod_evidence",
    "reviewed_crosswalk_decision",
    "frozen_metadata_discovery_query_plan",
    "offline_preparation_manifest",
    "metadata_discovery_execution_index",
    "frozen_challenge_eligibility_manifest",
    "frozen_selection_manifest",
    "private_result_item",
    "completed_run_manifest",
  ];
  const supportBindings = array(pilot.schemas.supporting_schemas, "supporting schema bindings");
  if (supportBindings.length !== expectedSupportRoles.length) fail("COD pilot must bind exactly nine supporting schemas");
  const supporting = new Map();
  for (const [index, expectedRole] of expectedSupportRoles.entries()) {
    const binding = supportBindings[index];
    if (binding.role !== expectedRole || !SHA256.test(binding.sha256)) fail("invalid supporting schema binding for " + expectedRole);
    const loaded = await lintJsonSchema(root, binding.path, schemaCache);
    if (bytesSha256(loaded.bytes) !== binding.sha256) fail("supporting schema digest drifted for " + expectedRole);
    supporting.set(expectedRole, loaded.schema);
  }

  const evidenceSchema = supporting.get("immutable_cod_evidence");
  if (evidenceSchema.properties.cod_id.pattern !== "^[0-9]{7}$" || evidenceSchema.properties.cod_svn_revision.minimum !== 1) fail("COD evidence identity constraints drifted");
  const crosswalkSchema = supporting.get("reviewed_crosswalk_decision");
  const acceptedRule = crosswalkSchema.allOf?.[0]?.then;
  if (!acceptedRule?.required?.includes("candidate") || !acceptedRule?.properties?.source_identity?.required?.includes("primary_publication_locator")) {
    fail("accepted COD crosswalk no longer requires a candidate and primary-publication review locator");
  }
  const discoveryPlanSchema = supporting.get("frozen_metadata_discovery_query_plan");
  const recoverySchema = (await lintJsonSchema(root, "schemas/pilots/cod-metadata-discovery-execution-index-v2.schema.json", schemaCache)).schema;
  if (recoverySchema.properties.schema_version.const !== 2 ||
      recoverySchema.properties.transport_recoveries.minItems !== 1 ||
      recoverySchema.properties.transport_recoveries.maxItems !== 8 ||
      recoverySchema.$defs.safetyAssertions.properties.request_start_spacing_and_retry_policy_were_enforced.const !== false ||
      recoverySchema.$defs.safetyAssertions.properties.reviewed_transport_recovery_policy_was_enforced.const !== true ||
      recoverySchema.$defs.transportRecovery.properties.halted_request.allOf[1].properties.attempts.minItems !== 4 ||
      recoverySchema.$defs.transportRecovery.properties.halted_request.allOf[1].properties.attempts.maxItems !== 4) {
    fail("reviewed transport recovery schema drifted");
  }
  if (
    discoveryPlanSchema.properties.requests.minItems !== 1000 ||
    discoveryPlanSchema.properties.requests.maxItems !== 1000 ||
    discoveryPlanSchema.$defs.transport.properties.method.const !== "GET" ||
    discoveryPlanSchema.$defs.transport.properties.redirects.const !== "forbidden" ||
    discoveryPlanSchema.$defs.transport.properties.concurrency.const !== 1 ||
    discoveryPlanSchema.$defs.limits.properties.planned_requests.const !== 1000 ||
    discoveryPlanSchema.properties.request_digest_policy.properties.canonicalization.const !== "waajacu-canonical-json-v1" ||
    discoveryPlanSchema.$defs.responseValidation.properties.profile.const !== "cod_metadata_json_rows_v1" ||
    discoveryPlanSchema.$defs.responseValidation.properties.scalar_policy.const !== "decimal_string_or_unsigned_json_integer_v1" ||
    discoveryPlanSchema.$defs.rowValidation.properties.enforcement.const !== "during_top_level_array_parse"
  ) {
    fail("COD metadata discovery query-plan safety contract drifted");
  }
  const preparationSchema = supporting.get("offline_preparation_manifest");
  if (
    preparationSchema.properties.format.const !== "waajacu-cod-pilot-preparation" ||
    preparationSchema.$defs.executionBoundary.properties.network_access.const !== "forbidden_during_prepare_and_verify" ||
    preparationSchema.$defs.executionBoundary.properties.database_writes.const !== "forbidden" ||
    preparationSchema.$defs.publicCatalogBinding.properties.mineral_count.const !== 6226 ||
    preparationSchema.$defs.queryPlanBinding.properties.path.const !== "cod-query-plan.json"
  ) {
    fail("COD offline preparation boundary drifted");
  }
  const discoveryIndexSchema = supporting.get("metadata_discovery_execution_index");
  if (
    !discoveryIndexSchema.properties.status.enum.includes("partial") ||
    !discoveryIndexSchema.properties.status.enum.includes("complete") ||
    discoveryIndexSchema.$defs.safetyAssertions.properties.normalized_output_was_not_produced.const !== true ||
    discoveryIndexSchema.$defs.safetyAssertions.properties.catalog_and_registry_databases_were_not_written.const !== true
  ) {
    fail("COD metadata discovery execution boundary drifted");
  }
  const eligibilitySchema = supporting.get("frozen_challenge_eligibility_manifest");
  if (
    eligibilitySchema.properties.status.const !== "frozen_before_normalized_output" ||
    eligibilitySchema.properties.entries.minItems !== 36 ||
    eligibilitySchema.$defs.freezeGuard.properties.normalized_adapter_output_inspected.const !== false ||
    eligibilitySchema.$defs.freezeGuard.properties.normalized_adapter_output_used_for_eligibility.const !== false
  ) {
    fail("COD challenge eligibility freeze boundary drifted");
  }
  const selectionSchema = supporting.get("frozen_selection_manifest");
  if (selectionSchema.properties.entries.minItems !== 96 || selectionSchema.properties.entries.maxItems !== 96) fail("selection manifest schema no longer requires exactly 96 minerals");
  const resultSchema = supporting.get("private_result_item");
  if (!resultSchema.description.includes("without creating questionnaire coverage rows")) fail("private result schema boundary drifted");
  const runSchema = supporting.get("completed_run_manifest");
  if (runSchema.properties.public_projection.const !== "forbidden" || runSchema.properties.adapter.properties.implementation_language.const !== "rust") {
    fail("completed run schema permits an unsafe runtime or implementation target");
  }

  challengeContract(pilot.sample_design);
  const crosswalk = pilot.crosswalk_policy;
  if (
    crosswalk.candidate_algorithm !== "exact_authority_names_v1" ||
    crosswalk.target !== "one_existing_material_public_id_or_no_link" ||
    crosswalk.many_cod_records_per_mineral !== true ||
    crosswalk.one_cod_sample_to_multiple_minerals !== false ||
    crosswalk.fuzzy_matching !== "forbidden" ||
    crosswalk.formula_role !== "diagnostic_only_never_candidate_generation_or_acceptance" ||
    crosswalk.manual_review !== "required_for_every_decision_and_every_accepted_link" ||
    crosswalk.identity_mutation !== "forbidden"
  ) {
    fail("COD crosswalk policy drifted");
  }
  exactStrings(crosswalk.accepted_match_bases, ["accepted_name_exact", "authority_alias_exact", "primary_publication_explicit"], "accepted COD match bases");

  const extraction = pilot.extraction_policy;
  if (
    extraction.implementation_target !== "versioned_rust_adapter" ||
    extraction.python_retrieval_automation !== "not_part_of_standard_workflow" ||
    extraction.placeholder_to_json_null !== "forbidden" ||
    extraction.placeholder_to_zero !== "forbidden" ||
    extraction.space_group_inference !== "forbidden" ||
    extraction.unit_cell_symmetry_fill !== "forbidden" ||
    extraction.unit_cell_averaging !== "forbidden" ||
    extraction.locally_recomputed_volume_as_sourced_claim !== "forbidden"
  ) {
    fail("COD extraction policy drifted");
  }
  const runtime = pilot.runtime_boundary;
  if (
    runtime.current_bulk_ingestion_compatible !== false ||
    runtime.current_public_exporter_questionnaire_gate !== false ||
    runtime.runtime_staging !== "blocked" ||
    runtime.required_ingestion_policy !== "cod_crystallography_v1" ||
    runtime.public_projection !== "forbidden"
  ) {
    fail("COD runtime boundary drifted");
  }
  if (pilot.next_stage.name !== "rust_candidate_discovery_runner_and_selection_freeze") fail("COD pilot next stage drifted");
  for (const forbidden of ["public_claims", "questionnaire_coverage_rows", "runtime_ingestion_batch"]) {
    if (!pilot.next_stage.must_not_produce.includes(forbidden)) fail("next stage no longer forbids " + forbidden);
  }
  for (const url of pilot.official_references) {
    if (typeof url !== "string" || !url.startsWith("https://")) fail("COD pilot has a non-HTTPS official reference");
  }

  return {
    pilotId: pilot.pilot_id,
    status: pilot.status,
    questionContracts: QUESTION_KEYS.length,
    valueSchemas: valueBindings.length,
    supportingSchemas: supportBindings.length,
    sampleMinerals: pilot.sample_design.counts.total,
    publicProjection: runtime.public_projection,
  };
}

async function main() {
  if (process.argv.length !== 3) fail("usage: validate-cod-crystallography-pilot.mjs REPOSITORY_ROOT");
  const result = await validateCodCrystallographyPilot(process.argv[2]);
  process.stdout.write(
    "Validated " +
      result.pilotId +
      ": " +
      result.questionContracts +
      " approved question contracts, " +
      result.valueSchemas +
      " value schemas, " +
      result.supportingSchemas +
      " supporting schemas, " +
      result.sampleMinerals +
      " planned minerals; public projection " +
      result.publicProjection +
      ".\n",
  );
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    process.stderr.write("COD crystallography pilot validation failed: " + error.message + "\n");
    process.exitCode = 1;
  });
}
