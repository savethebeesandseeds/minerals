import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { validateMineralContentContracts } from "./validate-mineral-content-contracts.mjs";

const SOURCE_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const REGISTRY = "schemas/mineral-record-questionnaire-v1.json";
const MATRIX = "schemas/mineral-source-admission-matrix-v1.json";
const DOCUMENT = "docs/MINERAL_RECORD_QUESTIONNAIRE_V1.md";

async function fixture(context) {
  const root = await mkdtemp(path.join(os.tmpdir(), "waajacu-content-contracts-"));
  await mkdir(path.join(root, "schemas"));
  await mkdir(path.join(root, "docs"));
  for (const relative of [REGISTRY, MATRIX, DOCUMENT]) {
    await copyFile(path.join(SOURCE_ROOT, ...relative.split("/")), path.join(root, ...relative.split("/")));
  }
  context.after(async () => {
    const resolvedRoot = path.resolve(root);
    const temporaryPrefix = path.resolve(os.tmpdir()) + path.sep;
    if (!resolvedRoot.startsWith(temporaryPrefix)) {
      throw new Error("refusing to remove a non-temporary fixture");
    }
    await rm(resolvedRoot, { recursive: true, force: true });
  });
  return root;
}

async function mutateJson(root, relative, mutate) {
  const file = path.join(root, ...relative.split("/"));
  const value = JSON.parse(await readFile(file, "utf8"));
  mutate(value);
  await writeFile(file, JSON.stringify(value, null, 2) + "\n");
}

test("accepts the checked-in questionnaire registry and source matrix", async () => {
  const result = await validateMineralContentContracts(SOURCE_ROOT);
  assert.deepEqual(result, {
    modules: 11,
    questions: 91,
    occurrenceFields: 21,
    sourceEntries: 8,
    admittedSources: 1,
    pilotOnlySources: 4,
  });
});

test("rejects drift between the human questionnaire and machine registry", async (context) => {
  const root = await fixture(context);
  const file = path.join(root, ...DOCUMENT.split("/"));
  const markdown = await readFile(file, "utf8");
  await writeFile(
    file,
    markdown.replace(
      "What is the authority-accepted name?",
      "What name should the interface display?",
    ),
  );
  await assert.rejects(
    validateMineralContentContracts(root),
    /identity\.canonical_name question text drifts from Markdown/,
  );
});

test("rejects a question key repeated in two modules", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.modules.names_classification.questions["identity.canonical_name"] =
      structuredClone(registry.modules.protected_identity.questions["identity.canonical_name"]);
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /question occurs in more than one module: identity\.canonical_name/,
  );
});

test("rejects an unknown source requirement", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.modules.physical_diagnostic.questions["physical.hardness_mohs"]
      .requirements.sourced.codes = ["MEMORY1"];
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /references unknown source requirement MEMORY1/,
  );
});

test("rejects an unknown consumer profile", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.modules.physical_diagnostic.questions["physical.hardness_mohs"]
      .consumer.facet = "guess_a_bucket";
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /references unknown profile guess_a_bucket/,
  );
});

test("keeps physical measurements from triggering safety banners", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.modules.physical_diagnostic.questions["physical.radioactivity"]
      .consumer.safety_banner = "hazard_trigger";
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /physical\.radioactivity must not drive a safety banner/,
  );
});

test("rejects source ownership of an unknown question", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, MATRIX, (matrix) => {
    matrix.entries.cod_crystal_observations.intended_ownership.question_keys.push(
      "physical.imaginary_property",
    );
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /references unknown question physical\.imaginary_property/,
  );
});

test("does not admit a source with an unpassed rights gate", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, MATRIX, (matrix) => {
    matrix.entries.ima_cnmnc_2026_07_identity.gate_status.rights = "conditional";
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /admitted source has an unpassed gate/,
  );
});

test("does not admit a source without a frozen artifact hash", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, MATRIX, (matrix) => {
    matrix.entries.ima_cnmnc_2026_07_identity.reproducibility.artifact_sha256 =
      "not_yet_captured";
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /admitted source lacks an approved frozen artifact/,
  );
});

test("rejects pilot modules in the public projection lifecycle", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.consumer_contract.public_projection_gate.allowed_module_lifecycle.unshift("pilot");
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /consumer_contract\.public_projection_gate does not match the protected contract/,
  );
});

test("rejects weakening redaction before public projection", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.consumer_contract.public_projection_gate.public_redaction_before_every_surface = false;
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /consumer_contract\.public_projection_gate does not match the protected contract/,
  );
});

test("keeps every occurrence surface off by default", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.consumer_contract.occurrence_defaults.worker = "public_answer";
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /consumer_contract\.occurrence_defaults does not match the protected contract/,
  );
});

test("requires reviewed public geometry", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.consumer_contract.profiles.map.geometry.reviewed_public_geometry_only = false;
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /consumer profile map\.geometry does not match the protected contract/,
  );
});

test("does not treat missing safety evidence as safe", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.consumer_contract.profiles.safety_banner.hazard_trigger.missing_is_safe = true;
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /consumer profile safety_banner\.hazard_trigger does not match the protected contract/,
  );
});

test("requires a typed affirmative discriminator for safety triggers", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    delete registry.modules.safety.questions["safety.toxicity"].value_schema.affirmative_discriminator;
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /safety\.toxicity needs an affirmative discriminator for reviewed_positive/,
  );
});

test("rejects overlapping safety discriminator states", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.modules.safety.questions["safety.toxicity"].value_schema
      .affirmative_discriminator.negative_values = ["hazard_present"];
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /affirmative discriminator value sets overlap at hazard_present/,
  );
});

test("requires explicit selectors for structured search projections", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    delete registry.modules.names_classification.questions["names.aliases"]
      .value_schema.projection_selectors;
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /names\.aliases needs a search projection selector for its structured value/,
  );
});

test("derives source claim scopes and value kinds from owned fields", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, MATRIX, (matrix) => {
    matrix.entries.cod_crystal_observations.claim_contract.value_kinds.push("text");
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /cod_crystal_observations\.claim_contract\.value_kinds does not match owned fields/,
  );
});

test("binds the source matrix to the canonical questionnaire digest", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, MATRIX, (matrix) => {
    matrix.questionnaire_registry_sha256 = "sha256:" + "0".repeat(64);
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /source matrix does not bind the questionnaire registry\/population/,
  );
});

test("preserves the verified-claim floor in SCI2", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.source_requirements.SCI2.minimum_verified_claims = 0;
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /source requirement SCI2\.minimum_verified_claims must be a positive integer/,
  );
});

test("requires source classes compatible with owned questions", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, MATRIX, (matrix) => {
    matrix.entries.cod_crystal_observations.authority_fit = ["primary_measurement_index"];
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /cod_crystal_observations authority_fit cannot satisfy crystallography\.space_group source requirements/,
  );
});

test("protects the bounded private adapter prerequisite gates", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, MATRIX, (matrix) => {
    matrix.policy.pilot_adapter_prerequisite_gates = ["rights"];
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /matrix\.policy\.pilot_adapter_prerequisite_gates does not match the versioned vocabulary/,
  );
});

test("does not approve a source matrix against a draft questionnaire", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, MATRIX, (matrix) => {
    matrix.status = "approved";
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /approved source matrix requires an approved questionnaire registry/,
  );
});

test("does not activate a public module with draft question contracts", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.modules.names_classification.lifecycle = "active";
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /cannot enter a public module with draft definitions or value schemas/,
  );
});

test("rejects repository-escaping approved value-schema references", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.modules.crystallography.questions["crystallography.space_group"]
      .value_schema.json_schema.$ref = "../outside.schema.json";
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /json_schema\.\$ref must be a safe repository-local schema path/,
  );
});

test("keeps a technical adapter contract from launching record coverage", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.modules.crystallography.technical_pilot.coverage_rows = "allowed";
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /technical pilot may create coverage or public projection/,
  );
});

test("requires every technical-pilot question contract to be approved", async (context) => {
  const root = await fixture(context);
  await mutateJson(root, REGISTRY, (registry) => {
    registry.modules.crystallography.technical_pilot.question_keys.push("crystallography.twinning");
  });
  await assert.rejects(
    validateMineralContentContracts(root),
    /technical pilot references a draft question contract: crystallography\.twinning/,
  );
});
