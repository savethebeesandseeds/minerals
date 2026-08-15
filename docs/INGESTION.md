# Mineral release generation and ingestion

Waajacu's Minerals aims to publish a complete, image-optional mineral registry
without turning a bulk source into untraceable truth. The unit of work is a
**versioned source release**, not a large unlabelled array and not an image.

Generated records are useful leads. They remain generated until independent
sources and review support a stronger scientific status.

## Invariants

- The catalog is mineral-only. General chemical compounds are out of scope.
- A valid mineral record has no image requirement. Missing media is ordinary,
  not an ingestion warning and never a reason to omit a mineral.
- A formula or name is a search attribute, not identity. Polymorphs may share a
  formula and nomenclature changes may replace a name.
- Stable source identity is the pair `(dataset, source_record_id)`. An authority
  identifier may be attached to it and must be unique within that authority.
- Every release has an immutable manifest digest, expected count, source and
  release identity, rights/license metadata, retrieval time, schema version,
  parser name/version/code revision/configuration digest, adapter identity, and
  field-ownership policy.
- Every accepted chunk is immutable and content-addressed. Retrying identical
  bytes is a no-op; changing bytes at an accepted index is a conflict.
- Staging never means publication. Only a browser-authenticated reviewer can
  approve the exact validated release digest.
- Missing rows never withdraw live minerals automatically. Withdrawal is a
  separate explicit reviewed action.
- Source data may change only the fields its declared policy owns.
- Readers see a complete old catalog or complete new catalog, never a partially
  activated release.
- The first approved `ima_identity_v1` release permanently and exclusively
  binds that policy to its reviewed `(dataset.key, source.key)` pair. Later
  releases must use that exact pair. A different source cannot claim official
  identity authority merely by selecting the policy name; the binding is
  rechecked at finalization and approval. Startup refuses a migrated database
  whose historical approved releases already assert more than one such pair;
  that ambiguity requires explicit operator resolution.
- Publication attribution is part of the hashed manifest, not mutable display
  copy. Every schema-v2 source declares the supplied human attribution party,
  exact work title and URL, canonical license URL, explicit changes/adaptation
  notice, non-endorsement notice, and the SPDX license of Waajacu's derived
  data output.

The repository's `AGPL-3.0-only` license covers the application software. It
does not license imported source works or the mineral claims derived from them.
`source.license_spdx` and `source.attribution.license_url` describe the admitted
source work; `source.attribution.derived_output_license_spdx` separately
describes the published derived mineral identity output. Reviewers must never
copy the software license into either data field by default.

## Source admission

Before writing an adapter, record:

1. dataset owner/publisher and canonical source URL;
2. release/version identifier and publication date when available;
3. license or rights statement, redistribution constraints, and attribution;
4. retrieval time and SHA-256 of the exact raw bytes;
5. parser/adapter version and a reproducible normalization specification;
6. the dataset's stable per-record identity and its rename/discredit semantics;
7. the field-ownership policy requested by the adapter.

Record the source's requested attribution verbatim. The changes notice must
describe the transformations the adapter actually performs (for example PDF
table extraction, whitespace or typography normalization, formula review, and
status mapping), not a generic claim that data was "processed." The
non-endorsement notice must not imply that the source organization reviewed or
endorsed Waajacu's adaptation.

Keep raw input and its checksum in private, durable archive storage. It does
not belong in Git, `/static`, a public artifact route, application logs, or an
error response.

## Stable identity and names

The internal mineral row may have a route slug, but an adapter resolves the row
through its persisted `(dataset, source_record_id)` mapping. Slugs may change
with a reviewed rename without creating a second mineral. Authority identifiers
use a normalized authority/value pair and cannot be reassigned silently.

Resolution rules:

1. Match the exact existing dataset/source-record mapping.
2. If absent, inspect an exact authority identifier owned by that source.
3. Any collision, reassignment, ambiguous identifier, or unsupported merge is a
   validation anomaly requiring human resolution.
4. Never merge by formula, normalized name, or fuzzy similarity.
5. Preserve former names and source-scoped synonyms as aliases with their
   provenance; do not erase curator aliases.

For a grandfathered species without an IMA number, the adapter assigns and
maintains a stable `source_record_id` and carries it across every rename. Never
recompute that ID from the current name, formula, CAS number, row position, or
slug. `ima_number` and `ima_symbol` are optional authority identifiers and
useful cross-checks; neither is required to serve as the source identity.

## Field ownership

Every manifest chooses one supported policy. Policy names are versioned so a
future semantic change requires a new name.

### `create_only_v1`

`create_only_v1` creates only previously absent source identities and blocks
every collision; it never updates/withdraws.

Use it for a cautious first baseline or a source that cannot reliably express
updates. A retry of an already staged identical release is idempotent, but a new
release encountering an existing identity reports the collision rather than
silently overwriting it. It cannot carry `ima_number`, `ima_symbol`, or any
other official authority identifier. An official IMA identity baseline must use
the exclusively bound `ima_identity_v1` policy.

### `ima_identity_v1`

`ima_identity_v1` owns canonical name, approved formula, nomenclature
status/current-valid flag, authority identifier keys, source-scoped
synonyms/former names, the authority's contextual country/reference/status
fields, and evidence associations from that same dataset. Contextual fields
are stored separately from curator properties and retain their dataset and
release provenance.

It preserves family/classification, descriptions, CAS, properties, safety,
media, offers, curator aliases/evidence, and quality/verification on existing
records, and never auto-withdraws missing rows. On new records, non-owned fields
remain absent/default; record license and initial scientific status come from
the reviewed payload.

This boundary is deliberate: an official nomenclature list is authoritative
for identity but is not automatically authoritative for every physical,
safety, commercial, or editorial field.

Formula/name are not identity—the `dataset/source_record_id` mapping is.

## Release lifecycle

1. **Normalize deterministically.** The adapter should emit Unicode NFC before
   hashing; the server preserves accepted spellings and rejects controls but
   does not silently repair Unicode forms. Preserve chemical case and leave
   unknown values absent rather than inventing zero or empty facts.
2. **Build the manifest.** Pin schema, source release, record/chunk counts,
   ownership policy, raw artifact URL/digest, canonical records digest,
   parser name/version/code/configuration, source license, complete publication
   attribution, derived-data license, snapshot kind, and exact
   approved base batch. The content-addressed manifest is the idempotency
   identity; there is no mutable client-selected key.
3. **Split deterministic chunks.** Each chunk has at most 500 records, a stable
   zero-based index, and its own SHA-256. Chunk boundaries and JSON
   canonicalization must reproduce exactly on retry.
4. **Create or resume.** The server persists the release before accepting
   records and returns its content-addressed batch identifier. Re-posting the
   identical manifest recovers the same batch; a changed manifest has a
   different identity and cannot overwrite it.
5. **Stage chunks.** The server validates and commits one chunk transaction at
   a time, recording accepted items and received counts. An invalid item causes
   the entire chunk request to be rejected; it does not disturb earlier
   accepted chunks.
6. **Finalize validation.** Exact expected counts, missing/duplicate indexes,
   record digests, stable identities, authority uniqueness, field bounds,
   source rights, and evidence gates are checked. Finalization produces an
   immutable additions/changes/conflicts/anomalies summary.
7. **Review.** A browser reviewer inspects the exact source credit, work and
   license links, changes, non-endorsement notice, derived-data license,
   manifest hashes, diff, counts, and anomaly samples shown by the UI. A
   statistical/risk-based sample
   is an additional operator procedure, not something the current UI selects.
   The required decision note is stored with `ADMIN_REVIEWER_ID`, decision time,
   policy version, and exact release digest.
8. **Back up externally.** Before approval, the operator creates and verifies an
   encrypted off-host backup. This is an operational gate; the application
   cannot verify the external copy.
9. **Activate atomically.** While holding the writer lock, approval creates and
   hashes a local SQLite pre-activation snapshot before changing public rows.
   If that local backup fails, activation fails. Every new report item carries
   a `target_baseline_hash`; approval rebuilds the release diff and rechecks
   every frozen target, absence, collision, dataset head, and authority binding
   inside the writer transaction. A changed precondition makes the report stale
   before any public write. A legacy report item whose baseline is `null` cannot
   be approved; replace it with a newly staged and finalized batch, then review
   the new report. Activation applies only fields owned by the reviewed policy
   in one transaction. Approval never upgrades scientific verification
   implicitly and absence never implies withdrawal.
10. **Verify and maintain.** Compare manifest/catalog counts and sample
    provenance; check foreign keys and FTS; monitor traffic; then optimize and
    checkpoint WAL in a quiet window.

Batch states are `receiving`, `ready`, `needs_attention`, `approved`, and
`rejected`. Finalization freezes the exact report and moves a conflict-free
batch to `ready`; blocking conflicts produce `needs_attention`. Approval and
rejection are terminal and idempotent only when all supplied hashes/base
coordinates still match the stored decision.

Approval or reviewer rejection also compacts the duplicated raw chunk/item JSON
inside that same decision transaction. The batch retains its manifest, report
and report items, decision, events (including accepted chunk hashes), and
compacted chunk/record/byte counters; approved mappings and evidence remain in
the registry. Status counts therefore remain stable, but ordinary raw-record
review samples are empty after a terminal decision. Report-derived anomaly
items remain inspectable. The external raw-source archive is the durable source
for replay and audit beyond this compacted server record.

## Strict manifest and chunk contract

Schema version `2` uses the exact nested `dataset`, `source`, `release`,
`retrieval`, `artifact`, and `parser` objects shown in the example. It also
requires `policy`, expected record/chunk counts, `records_sha256`,
`snapshot_kind`, and `base_batch_id`. A chunk contains only `schema_version`,
`chunk_index`, and `items`; a skeleton item contains only stable source ID and
locator, proposed slug, name, formula, nomenclature status/current-valid flag,
official identifiers, source-scoped synonyms, and an optional
`official_facts` object. That object has the exact optional keys
`discovery_country`, `first_reference`, `second_reference`, and
`source_status`. `source.attribution` is
strict and contains exactly `attribution_party`, `work_title`, `work_url`,
`license_url`, `changes_notice`, `no_endorsement_notice`, and
`derived_output_license_spdx`.

Schema-v1 terminal batches remain readable as immutable audit history. They
predate the attribution boundary and cannot be created, finalized, or
approved. Startup rejects any non-terminal v1 batch without rewriting its
manifest or report; generate and review a new v2 release instead.

Hashes use `sha256:` followed by 64 lowercase hexadecimal characters. Canonical
JSON is compact UTF-8 with object keys recursively sorted lexicographically and
array order preserved. The manifest hash covers the complete manifest, a chunk
hash covers the complete chunk, and `records_sha256` covers one flat item array
ordered by `chunk_index` then `item_index`.

`base_batch_id` is `null` only when the dataset has no approved head. Otherwise
it is the exact current approved batch and is rechecked during finalization and
activation. This compare-and-swap prevents an older review from overwriting a
newer release.

The endpoint rejects unknown fields, unsupported schema or policy versions,
missing/malformed digests, counts outside release limits, unsupported authority
identifier keys, duplicate source identities, chunks over 500 records,
oversized bounded fields, malformed URLs/timestamps, control characters, and
unsupported nomenclature states. Source and derived-data SPDX expressions must
be explicit; `NONE` and `NOASSERTION` are rejected. Attribution URLs must be
valid, the license URL must be canonical HTTPS, and known Creative Commons
SPDX identifiers must use their canonical license URL. A CC BY-SA release must
declare the same share-alike SPDX expression for its derived output. These
technical gates do not replace source admission or legal review.

Schema version 2 accepts nomenclature states `approved`, `grandfathered`, `renamed`,
`redefined`, `discredited`, `questionable`, `uncertain`, and `unknown`; a
`discredited` row cannot also be a valid species. Authority keys are restricted
to the unique identity keys `ima_number` and `ima_symbol`, and only
`ima_identity_v1` may carry them or `official_facts`; `create_only_v1` requires
an empty identifier map and no official facts. When an IMA raw status is
present, it must agree with the normalized status/current-valid pair. Release
year and nomenclature status are attributes, not stable
identifiers. There may be at most 16 authority values and 100 synonyms per
item. Source record IDs and names are bounded to 240 characters, source
locators/formulas/references to 500, and every proposed slug must use the `mineral.`
namespace.

See these deliberately non-importable shape examples:

- [`examples/release-manifest.nonimportable.json`](../examples/release-manifest.nonimportable.json)
- [`examples/release-chunk.nonimportable.json`](../examples/release-chunk.nonimportable.json)

Every example contains reserved placeholder identities and an explicit
non-importable marker. The application must reject it unchanged. Never remove
that marker with a broad search/replace; generate a real manifest from archived
source bytes.

Machine adapters may use the optional bearer staging credential:

```http
Authorization: Bearer REPLACE_WITH_32_PLUS_CHARACTER_SECRET
```

Set both `INGESTION_API_TOKEN` and a stable `INGESTION_ADAPTER_ID`. The token
can create, inspect, resume, upload, and finalize private staged work only; it
cannot approve, reject, activate, withdraw, or access the browser reviewer
session. Never log the header or embed it in a fixture/URL. Approval remains a
same-origin browser action attributed by `ADMIN_REVIEWER_ID`.

## Chunk retry and crash semantics

- A client timeout is an unknown outcome. Query release status before retrying.
- Retrying an identical accepted chunk returns the recorded result and does not
  create new candidate revisions.
- Reusing the same release/index with a different digest is a permanent
  conflict. Create a corrected source release rather than editing history.
- A rejected chunk is not persisted. The adapter must retain its request,
  canonical hash, response status, and validation diagnostics in its own
  private run log. Corrected bytes require a new manifest/batch identity; do
  not overwrite an accepted chunk or claim that a pre-database rejection was
  stored by the server.
- A restart resumes from persisted accepted indexes. The adapter must not infer
  progress only from its local checkpoint.
- Only one ingestion writer commits at a time. Concurrent mutations receive a
  `503 Service Unavailable` with `Retry-After`; clients must retry idempotently
  with backoff. Raw SQLite errors are never public.
- Write routes authenticate the caller, acquire the sole writer permit, require
  JSON, and enforce their transport body ceiling before or while extracting a
  body. An unauthenticated, busy, wrong-content-type, or known oversized request
  is rejected without buffering its declared body. The canonical quarantine
  quota is checked later, after strict JSON parsing and canonicalization.
- Quarantine accounting deliberately includes both the canonical chunk JSON and
  the duplicated canonical JSON stored for each indexed item. Defaults are 64
  MiB per batch and 512 MiB globally across batches in `receiving`, `ready`, or
  `needs_attention`. A terminal reviewer decision compacts its chunk/item JSON,
  and terminal `approved` and `rejected` batches do not consume the active
  global quota. These are safety bounds, not measured capacity.
- Creating a batch or storing a chunk opportunistically expires `receiving`
  batches whose latest chunk activity (or creation time before the first chunk)
  is at least 336 hours old. Expiry marks the batch `rejected`, records decision
  note `expired_abandoned_batch` and event `batch_expired`, deletes its stored
  chunks/items, and retains the immutable manifest and events as an audit
  tombstone. There is no background timer or public cleanup endpoint. `ready`,
  `needs_attention`, `approved`, and explicitly rejected batches never expire
  through this rule.
- Configure the bounds with `INGESTION_BATCH_MAX_BYTES`,
  `INGESTION_QUARANTINE_MAX_BYTES`, and `INGESTION_ABANDONED_HOURS`. Invalid or
  inconsistent values fail server initialization and `/readyz`.

## Record and evidence validation

- `sourced` records require one canonical distinct HTTP(S) evidence URL.
- `reviewed` records require at least one reviewed source.
- `verified` records require two canonical distinct reviewed sources, including
  at least one whose source review state is `verified`.
- Approval preserves the candidate scientific state; editorial publication
  never upgrades it.
- CAS checksum validation establishes syntax only, not mineral ownership.
- Identifiers, aliases, strings, request bodies, and record counts have explicit
  maxima. Unknown fields are errors, not ignored extensions.
- Source and derived-data licenses must be explicit. The server validates the
  bounded expression and known Creative Commons URL/share-alike relationship,
  but it does not certify that the submitter owns the rights or that the chosen
  terms are legally sufficient.

Each evidence item supports one granular `claim_scope`:

- `identity.canonical_name`, `identity.formula`,
  `identity.nomenclature_status`, or `identity.current_valid`;
- `identifiers.<key>`;
- `properties.<key>`;
- `safety.<key>`.

Keys use lowercase ASCII letters, digits, and underscores. The claim envelope
retains context rather than flattening incomparable measurements:

```json
{
  "value": {"min": 6.0, "max": 6.5},
  "unit": "Mohs",
  "conditions": {"specimen": "natural, unweathered"},
  "source_locator": "Table X, row Y",
  "note": "Optional qualification"
}
```

Multiple sources can retain different values for the same scope. They are not
averaged. An explicit `disputed` review state records a known conflict.

## Generated records and images

Model-generated mineral content uses `verification_status = generated`, records
the model/run in operational metadata, and never invents citations or authority
identifiers. A model is an extraction aid, not evidence. Translations copy
formulas and numeric invariants from reviewed source fields.

Bulk identity ingestion should omit images. Media can be added later as a
separate, licensed, provenance-aware workflow. Future synthetic media must be
visibly labelled illustrative and retain generator provider, model/version,
prompt/input hashes, parameters, seed, generation time, and licenses for every
reference asset. It is never specimen evidence or identification ground truth.

## Provider ingestion is separate

Provider adapters populate providers, listings, and time-stamped offers only.
Purity, grade, origin, stock, price, and composition remain provider claims and
never update scientific records. Missing provider offers may retire an offer
only under the provider snapshot contract; that commerce behavior does not
apply to scientific source releases.
