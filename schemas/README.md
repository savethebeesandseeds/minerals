# Mineral content contracts

This directory contains versioned, reviewable contracts for catalog
enrichment. It is not runtime data and it is not an import queue.

- [`mineral-record-questionnaire-v1.json`](mineral-record-questionnaire-v1.json)
  mirrors the 91 standard mineral questions, 21 occurrence fields, evidence
  thresholds, missing/review states, consumer behavior, and legacy
  dispositions.
- [`mineral-source-admission-matrix-v1.json`](mineral-source-admission-matrix-v1.json)
  records source × question-group ownership, rights, identifiers, locators,
  reproducibility gates, review burden, and decisions.
- [`values/`](values/) contains the executable Draft 2020-12 value schemas for
  question contracts that have completed scientific review. The first three
  cover repeatable COD pilot observations for space group, unit cell, and
  structure references.
- [`pilots/cod-crystallography-pilot-v1.json`](pilots/cod-crystallography-pilot-v1.json)
  binds those questions and schema bytes to the source matrix, fixed mineral
  population, strict crosswalk rules, and the not-yet-run 96-mineral private
  pilot. Adjacent schemas define the frozen 1,000-shard metadata query,
  resumable discovery execution index, reviewed challenge-eligibility freeze,
  immutable COD evidence, reviewed crosswalks, frozen selection, private result
  items, and completed run manifests.

Both files are intentionally marked `draft_for_pilot_validation`. A question's
`definition_status`, value-schema `status`, controlled-vocabulary `status`, and
applicability-rule `status` show what still needs scientific approval. Do not
interpret a syntactically valid draft as permission to ingest content.

Validate the contracts from the repository root:

```bash
node tools/validate-mineral-content-contracts.mjs .
node tools/validate-cod-crystallography-pilot.mjs .
node --test tools/test_validate_mineral_content_contracts.mjs \
  tools/test_validate_cod_crystallography_pilot.mjs
```

The validator rejects human/machine drift, duplicate questions, weakened
public/redaction profiles, unsafe map or safety-banner behavior, structured
search/comparison values without selectors, source ownership whose declared
scope or value kinds do not match the registry, incompatible source classes,
matrix/registry digest drift, and public admission without passed rights and
reproducibility gates. Draft modules and the draft occurrence collection cannot
enter public projection; private pilots remain non-public.

The COD validator additionally resolves every local JSON Schema reference,
rejects remote references and open typed objects, verifies exact-file schema
hashes plus canonical registry/matrix hashes, protects the deterministic
60+36 and 72/24 sample design, requires sequential redirect-free discovery,
freezes challenge eligibility before normalized output is inspected, and keeps
runtime staging and public projection blocked. Validating the contract does not
mean that candidate discovery, retrieval, crosswalking, ingestion, or
publication has occurred.
