# Architecture

## Current runtime

The application is a Rust/Axum service with Askama-rendered HTML and SQLite.
It intentionally remains a modular monolith while the domain and data contracts
stabilize.

```text
browser / API client
        |
        v
Axum routes and Askama views
        |
        +-- legacy catalog compatibility (`minerals`, `catalog`, `images`)
        +-- global mineral registry and FTS5 search
        +-- evidence, immutable review revisions, and versioned source releases
        +-- providers, offers, and sourcing requests
        +-- deterministic report analysis
        +-- bounded XeLaTeX report worker
        |
        v
SQLite + private data root
```

Only three file surfaces are public:

- `/static/*`: application-owned CSS, JavaScript, fonts, and images (template
  source files such as `.html` and `.tex` are denied);
- `/media/images/:file`: database-registered image files;
- `/artifacts/:folder/:run/:file`: only `report.pdf` and `report.html` from
  opaque, request-specific report runs.

The database, source JSON, ingestion payloads, TeX working files, and backups
are not public routes.

The media and artifact handlers consult the authoritative publication state
before reading bytes. A withdrawn mineral's unshared images and legacy reports
therefore return `404`; image responses revalidate and personalized reports use
`no-store`.

## Data boundaries

### Legacy publication model

The original `minerals`, `catalog`, and `images` tables remain compatible with
the existing catalog, admin publishing, translations, and reports. A trigger
projects new legacy mineral rows into the global registry.

Legacy scientific content is imported with:

- `source_kind = legacy_catalog`;
- `verification_status = generated`;
- `data_quality_score = 0.35`;
- no fabricated citation or external identifier.

### Registry model

`materials` represents published searchable mineral records. The neutral table
name is retained for migration compatibility, but compound records are outside
the public product scope. It stores the current public profile, publication
state, and explicit scientific verification state. Images are nullable and
orthogonal: a complete image-free catalog is a normal production state.
`material_aliases` supports synonyms and multilingual names. FTS5 indexes names,
formulas, identifiers, aliases, and descriptions.

`mineral_review_revisions` stores each validated imported mineral payload as an
immutable, monotonically numbered candidate. Publication state is separate
from scientific verification: approving a candidate applies that exact payload
to the live registry, while rejecting it preserves the current published row.
Public queries explicitly require `publication_status = published`.

Stable imported identity does not depend on a slug, formula, or display name.
It is the persisted mapping from `(dataset, source_record_id)` to an internal
mineral. Authority identifier keys add a unique cross-check. A reviewed rename
can update the public name/route and retain the former name without creating a
second mineral.

`evidence_sources` stores source identity, publisher, retrieval time, content
hash, and license. `material_evidence` binds a source to a scoped claim document
with confidence and review status. Bulk schema-v2 publication also snapshots
the human attribution party, exact work title/URL, canonical license URL,
changes and non-endorsement notices, and derived-data SPDX license on each
material/evidence association. Public rendering never relies on mutable global
source metadata for those credits. New claims use granular paths such as
`identity.formula`, `identifiers.cas_number`,
`properties.hardness_mohs`, and `safety.handling`. Their JSON envelope contains
the asserted `value` plus optional `unit`, `conditions`, `source_locator`, and
`note` fields. This preserves source-specific values without allowing an
evidence import to update the current public profile automatically.

Evidence associations from each source remain source-owned. Each imported
candidate preserves its exact payload inside the immutable review revision, so
different sources can retain different claims for the same scope without
destructive averaging. Release and decision records retain the source manifest,
payload digest, ownership policy, adapter identity, reviewer identity, policy
version, note, and decision time.

`material_media` records whether an optional asset is sourced, uploaded, or
synthetic. Synthetic assets require generation provenance and remain
illustrative; neither searchability nor release membership requires media.

### Release ingestion model

Large source releases use a durable state machine instead of one oversized
request:

```text
source admission + raw SHA-256
              |
              v
immutable manifest -- create/resume --> deterministic chunks (<= 500)
              |                              |
              +------------------------------+
                              |
                              v
             validation + identity resolution + diff
                              |
                       browser review
                              |
              operator-verified off-host backup
                 + automatic local snapshot
                              |
                              v
                    atomic activation
```

The server persists the release before accepting chunks. Manifest idempotency,
per-chunk digests, unique indexes, and recorded checkpoints make disconnects
and restarts resumable. A repeated identical request is a no-op; a different
payload at an accepted release/chunk identity is a conflict.

Only one SQLite writer commits a chunk at a time. Earlier chunks remain private
when a later chunk fails. Finalization checks the manifest count, all chunk and
record hashes, stable source identities, authority identifier uniqueness, and
strict bounded schemas before producing a release-level diff. A
browser-authenticated reviewer approves the exact digest; the optional machine
bearer credential can stage and inspect but cannot publish.

External encrypted backup verification is an operator gate before approval.
The activation transaction additionally creates and hashes a private local
SQLite snapshot before changing public rows; failure aborts activation. The
service retains a bounded ten local snapshots, which are recovery aids rather
than substitutes for off-host backup.

Activation applies a versioned field-ownership policy. `create_only_v1` creates
only absent source identities and blocks collisions; `ima_identity_v1` owns
official identity/nomenclature fields, source contextual facts, and
source-scoped aliases/evidence while
preserving curator, classification, scientific-property, media, and commerce
fields. Its first approved release binds the policy exclusively to that exact
dataset/source pair. Approval revalidates the dataset head, authority binding,
and every frozen target/absence/collision baseline before any public write.
Missing rows never cause automatic withdrawal. See
[INGESTION.md](INGESTION.md) for the exact boundary.

Only that bound `ima_identity_v1` source may stage `ima_number`/`ima_symbol` or
official country/reference/status facts; `create_only_v1` requires an empty
official-identifier map and no official facts. A terminal reviewer
decision compacts duplicated chunk/item JSON in the same transaction while
retaining the manifest, frozen report/report items, decisions/events and
logical byte/record/chunk counters. Raw source archives remain an operator
responsibility outside the application quarantine.

### Commerce model

`providers` describes organizations and their independent verification state.
`offers` contains time-sensitive provider claims: product URL, price basis,
minimum order, stock state, grade, purity, origin, evidence, and last check.

Provider claims never update mineral properties or evidence automatically.
The response ordering is deterministic:

1. offer verification;
2. provider verification;
3. stock state;
4. provider trust score;
5. observation freshness.

The initial public action opens the provider's product or quote page. Payments
and order placement are deliberately out of scope until identity, compliance,
and confirmation workflows exist.

`sourcing_requests` and `provider_search_runs` are the durable queue foundation
for future agent-led searches across approved connectors.

## Trust states

Mineral records use:

- `draft`: incomplete private or imported candidate;
- `generated`: model- or legacy-produced lead without sufficient sourcing;
- `sourced`: at least one canonical valid cited source;
- `reviewed`: at least one source claim has been reviewed;
- `verified`: at least two canonical distinct reviewed sources, including one
  verified source, are required by the import gate;
- `disputed`: known conflict or challenge.

Offer records use separate states: `provider_claim`, `observed`, `verified`, and
`disputed`. Provider organizations have their own independent verification and
trust score.

## Search contract

`GET /minerals?q=<query>&page=<number>` is the localized public discovery
surface. It is backed by the same mineral registry and returns 24 records per
page. `/catalog` is retained only as a permanent redirect for old links.

`GET /api/minerals?q=<query>&limit=<1..100>`
uses SQLite FTS5 with alphanumeric prefix tokens. Empty queries list records by
quality and canonical name. Results include verification, evidence count, and
active offer count; attributed results also include a compact derived-data
license signal and stable detail-page attribution path. Commerce does not alter
knowledge relevance.

Mineral detail and offer endpoints are:

```text
GET /api/minerals/:slug
GET /api/minerals/:slug/offers
```

## Import contracts

Individual curator imports remain available as all-or-nothing SQLite
transactions:

```text
GET  /admin/reviews
POST /admin/minerals/import
POST /admin/minerals/review
POST /admin/minerals/withdraw
POST /admin/providers/import
```

The single/batch mineral endpoint is intended for small curator work. Validation
happens for the whole request before any write. Valid imports are queued as
immutable candidate revisions and remain private until an operator approves
the exact revision. Large authoritative datasets use the release-ingestion
state machine: strict manifest, deterministic at-most-500-record chunks,
resumable/idempotent staging, release-level validation and diff, attributed
browser review, verified backup, then atomic activation. Each `sources` entry
represents one granular scoped claim; the contract is documented in
[INGESTION.md](INGESTION.md).

Approval does not change scientific verification state. Withdrawal atomically
marks the authoritative mineral non-public, retires its offers, and supersedes
pending revisions so an older candidate cannot later republish it.

The provider endpoint accepts one provider plus its offers. It rejects unknown
minerals and inconsistent price/currency, quantity, stock, URL, and evidence
fields. The offer array is a complete current snapshot; previously known offers
omitted from a later import are retired. Examples live in `examples/`.

## Scaling path

SQLite is appropriate for the complete roughly 6,200-mineral catalog. Foreign
keys, WAL, configurable durability, busy timeouts, bounded transactions,
indexes, and FTS5 are enabled. Deployment is exactly one application instance
with one writer and a local durable block volume; SQLite WAL is not supported
on NFS/SMB or a shared multi-instance mount. Schema and quarantine ceilings are
safety bounds, not capacity guarantees; release acceptance must be measured on
the deployed topology.

Move the authoritative writer to PostgreSQL only when multiple independent
writers or horizontally scaled application replicas become product
requirements. Move optional media to object storage as it grows, and add a
dedicated search service only after measured retrieval needs justify it, while
preserving stable public/source identities and API contracts.
