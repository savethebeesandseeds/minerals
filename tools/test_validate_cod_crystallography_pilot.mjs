import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { validateCodCrystallographyPilot } from "./validate-cod-crystallography-pilot.mjs";

const SOURCE_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const PILOT = "schemas/pilots/cod-crystallography-pilot-v1.json";
const REGISTRY = "schemas/mineral-record-questionnaire-v1.json";
const MATRIX = "schemas/mineral-source-admission-matrix-v1.json";
const FILES = [
  PILOT,
  REGISTRY,
  MATRIX,
  "schemas/values/crystallography-space-group-v1.schema.json",
  "schemas/values/crystallography-unit-cell-v1.schema.json",
  "schemas/values/crystallography-structure-reference-v1.schema.json",
  "schemas/pilots/cod-evidence-v1.schema.json",
  "schemas/pilots/cod-crosswalk-decision-v1.schema.json",
  "schemas/pilots/cod-metadata-discovery-query-plan-v1.schema.json",
  "schemas/pilots/cod-crystallography-preparation-manifest-v1.schema.json",
  "schemas/pilots/cod-metadata-discovery-execution-index-v1.schema.json",
  "schemas/pilots/cod-crystallography-challenge-eligibility-v1.schema.json",
  "schemas/pilots/cod-crystallography-selection-manifest-v1.schema.json",
  "schemas/pilots/cod-crystallography-pilot-item-v1.schema.json",
  "schemas/pilots/cod-crystallography-run-manifest-v1.schema.json",
];

function canonicalJson(value) {
  if (Array.isArray(value)) return "[" + value.map(canonicalJson).join(",") + "]";
  if (value !== null && typeof value === "object") {
    return "{" + Object.keys(value).sort().map((key) => JSON.stringify(key) + ":" + canonicalJson(value[key])).join(",") + "}";
  }
  return JSON.stringify(value);
}

function canonicalSha256(value) {
  return "sha256:" + createHash("sha256").update(canonicalJson(value), "utf8").digest("hex");
}

function bytesSha256(bytes) {
  return "sha256:" + createHash("sha256").update(bytes).digest("hex");
}

async function fixture(context) {
  const root = await mkdtemp(path.join(os.tmpdir(), "waajacu-cod-pilot-"));
  await mkdir(path.join(root, "schemas", "values"), { recursive: true });
  await mkdir(path.join(root, "schemas", "pilots"), { recursive: true });
  for (const relative of FILES) {
    await copyFile(path.join(SOURCE_ROOT, ...relative.split("/")), path.join(root, ...relative.split("/")));
  }
  context.after(async () => {
    const resolved = path.resolve(root);
    if (!resolved.startsWith(path.resolve(os.tmpdir()) + path.sep)) throw new Error("refusing to remove a non-temporary fixture");
    await rm(resolved, { recursive: true, force: true });
  });
  return root;
}

async function readJson(root, relative) {
  return JSON.parse(await readFile(path.join(root, ...relative.split("/")), "utf8"));
}

async function writeJson(root, relative, value) {
  await writeFile(path.join(root, ...relative.split("/")), JSON.stringify(value, null, 2) + "\n");
}

async function mutateJson(root, relative, mutate) {
  const value = await readJson(root, relative);
  mutate(value);
  await writeJson(root, relative, value);
}

async function rebind(root) {
  const registry = await readJson(root, REGISTRY);
  const matrix = await readJson(root, MATRIX);
  matrix.questionnaire_registry_revision = registry.registry_revision;
  matrix.questionnaire_registry_sha256 = canonicalSha256(registry);
  await writeJson(root, MATRIX, matrix);

  const pilot = await readJson(root, PILOT);
  pilot.bindings.questionnaire_registry.revision = registry.registry_revision;
  pilot.bindings.questionnaire_registry.sha256 = canonicalSha256(registry);
  pilot.bindings.source_admission_matrix.revision = matrix.matrix_revision;
  pilot.bindings.source_admission_matrix.sha256 = canonicalSha256(matrix);
  for (const binding of [...pilot.schemas.value_schemas, ...pilot.schemas.supporting_schemas]) {
    binding.sha256 = bytesSha256(await readFile(path.join(root, ...binding.path.split("/"))));
  }
  await writeJson(root, PILOT, pilot);
}

test("accepts the checked-in private COD pilot contract", async () => {
  assert.deepEqual(await validateCodCrystallographyPilot(SOURCE_ROOT), {
    pilotId: "cod-crystallography-v1",
    status: "contract_ready_candidate_discovery_not_run",
    questionContracts: 3,
    valueSchemas: 3,
    supportingSchemas: 9,
    sampleMinerals: 96,
    publicProjection: "forbidden",
  });
});

test("rejects an unbound value-schema edit", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, "schemas/values/crystallography-unit-cell-v1.schema.json", (schema) => {
    schema.$defs.angle.properties.value.exclusiveMaximum = 181;
  });
  await assert.rejects(validateCodCrystallographyPilot(root), /value schema digest drifted for crystallography\.unit_cell/);
});

test("rejects remote JSON Schema references even when hashes are rebound", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, "schemas/values/crystallography-space-group-v1.schema.json", (schema) => {
    schema.items.$ref = "https://example.invalid/unknown.schema.json";
  });
  await rebind(root);
  await assert.rejects(validateCodCrystallographyPilot(root), /uses a remote \$ref/);
});

test("keeps accepted crosswalks tied to reviewed publication evidence", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, "schemas/pilots/cod-crosswalk-decision-v1.schema.json", (schema) => {
    delete schema.allOf[0].then.properties.source_identity.required;
  });
  await rebind(root);
  await assert.rejects(validateCodCrystallographyPilot(root), /accepted COD crosswalk no longer requires/);
});

test("keeps metadata discovery sequential, bounded, and redirect-free", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, "schemas/pilots/cod-metadata-discovery-query-plan-v1.schema.json", (schema) => {
    schema.$defs.transport.properties.redirects.const = "allowed";
  });
  await rebind(root);
  await assert.rejects(validateCodCrystallographyPilot(root), /metadata discovery query-plan safety contract drifted/);
});

test("freezes challenge eligibility before normalized output is inspected", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, "schemas/pilots/cod-crystallography-challenge-eligibility-v1.schema.json", (schema) => {
    schema.$defs.freezeGuard.properties.normalized_adapter_output_inspected.const = true;
  });
  await rebind(root);
  await assert.rejects(validateCodCrystallographyPilot(root), /challenge eligibility freeze boundary drifted/);
});

test("rejects changing the 96-mineral sample totals", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, PILOT, (pilot) => {
    pilot.sample_design.counts.total = 95;
  });
  await assert.rejects(validateCodCrystallographyPilot(root), /sample_design\.counts\.total must remain 96/);
});

test("rejects silent challenge-stratum substitution", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, PILOT, (pilot) => {
    pilot.sample_design.challenge.groups[1].strata[0].quota = 3;
    pilot.sample_design.challenge.groups[1].strata[1].quota = 1;
  });
  await assert.rejects(validateCodCrystallographyPilot(root), /challenge strata contract drifted for value_shape_applicability/);
});

test("keeps fuzzy and formula matching from accepting identities", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, PILOT, (pilot) => {
    pilot.crosswalk_policy.fuzzy_matching = "candidate_generation_allowed";
  });
  await assert.rejects(validateCodCrystallographyPilot(root), /COD crosswalk policy drifted/);
});

test("keeps runtime staging and public projection blocked", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, PILOT, (pilot) => {
    pilot.runtime_boundary.runtime_staging = "allowed";
    pilot.runtime_boundary.public_projection = "allowed";
  });
  await assert.rejects(validateCodCrystallographyPilot(root), /COD runtime boundary drifted/);
});

test("keeps unit-cell comparison off without a multidimensional comparator", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.modules.crystallography.questions["crystallography.unit_cell"].consumer.comparison = "measurement";
  });
  await rebind(root);
  await assert.rejects(validateCodCrystallographyPilot(root), /unit-cell comparison requires a future multidimensional comparator/);
});

test("does not let a technical adapter contract launch the module", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.modules.crystallography.lifecycle = "pilot";
  });
  await rebind(root);
  await assert.rejects(validateCodCrystallographyPilot(root), /crystallography module must remain not_launched/);
});
