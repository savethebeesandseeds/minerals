# Waajacu's Minerals

Waajacu's Minerals is becoming an open, provenance-first engine to find,
research, and source minerals. It is a Rust/Axum modular monolith with SQLite,
full-text mineral search, multilingual profiles, evidence-aware ingestion,
provider offers, and HTML/PDF research reports.

Read [the product vision](docs/VISION.md), [architecture](docs/ARCHITECTURE.md),
[ingestion policy](docs/INGESTION.md), and [operations guide](docs/OPERATIONS.md).

## What works

- Search mineral records by name, formula, identifier, or synonym.
- Distinguish generated, sourced, reviewed, verified, and disputed knowledge.
- Attach licensed evidence with granular claims, confidence, and review state.
- Stage versioned source releases in resumable, idempotent chunks, review their
  identity diff, and activate the reviewed digest atomically.
- Import providers and time-sensitive offers without treating commercial claims
  as scientific evidence.
- Preserve the original multilingual catalog and admin publishing flow.
- Generate deterministic research summaries plus isolated HTML/XeLaTeX reports.
- Record future synthetic media explicitly as synthetic with provenance fields.

Images are not required for identity, search, evidence, or publication. The
first complete mineral registry is intentionally image-free; licensed or
clearly labelled illustrative media can be added later without blocking it.

The development dataset contains one legacy Phenakite catalog record. It
migrates into the global registry as `generated` legacy knowledge, not as a
verified scientific fact.

## Quick start with Docker

Create `.env.local` with a long random admin password and stable reviewer ID:

```dotenv
ADMIN_PASSWORD=replace-with-at-least-12-random-characters
ADMIN_REVIEWER_ID=replace-with-a-stable-operator-id
# SQLITE_DURABILITY=FULL
# INGESTION_API_TOKEN=optional-32-plus-character-machine-staging-secret
# INGESTION_ADAPTER_ID=required-when-machine-staging-is-enabled
# TRUSTED_PROXY_IPS=127.0.0.1,::1 # only if these peers terminate trusted TLS
# OPENAI_API_KEY=optional-for-ai-assisted-drafting
```

Then:

```bash
docker compose up -d --build
curl --fail http://127.0.0.1:7979/livez
curl --fail http://127.0.0.1:7979/readyz
```

Open:

- `http://127.0.0.1:7979/minerals` - worldwide mineral discovery;
- `http://127.0.0.1:7979/admin` - authenticated mineral management;
- `http://127.0.0.1:7979/admin/reviews` - individual mineral review queue;
- `http://127.0.0.1:7979/admin/ingestion` - dataset release review queue.

The container builds with the locked Rust dependency graph, uses a read-only
root filesystem, persists only `./data`, and includes XeLaTeX, `latexmk`, and
Noto fonts. Compose uses a small root bootstrap to make the bind mount private,
then clears all capabilities and runs the service as the mount owner (or UID
10001 on Docker Desktop). It also provides configurable CPU/memory bounds, log
rotation, and a graceful shutdown window longer than one ingestion chunk.

## Native development

Rust 1.96 or a compatible current stable toolchain is recommended.

```bash
cargo test
cargo run
```

Native runs bind to `127.0.0.1:7979` by default. The container overrides
`BIND_ADDRESS=0.0.0.0` internally while publishing only to host loopback.

## Static public catalog

The public mineral experience can be deployed as ordinary static files. The
browser loads a sanitized, content-addressed SQLite snapshot in the vendored
SQLite WebAssembly runtime, then performs search, pagination, and detail
queries locally. The Axum service and live `data/minerals.db` remain the private
administration and publication control plane.

Build the exporter when application code changes:

```bash
cargo build --locked --release --bin export-public
```

Then publish content without recompiling. On Windows:

```powershell
.\target\release\export-public.exe --data-root .\data --output .\public-dist
```

On Linux or macOS:

```bash
./target/release/export-public --data-root ./data --output ./public-dist
```

The command copies only the explicit public-app asset allowlist, reads one
consistent read-only registry snapshot, creates a public-only SQLite database
under `public-dist/data/`, verifies it, emits precompressed `.br` and `.gz`
representations, and replaces
`public-dist/catalog-manifest.json` last. Serve `public-dist/` over HTTPS;
literal localhost or loopback HTTP is supported for development, while
`file:` URLs cannot run module workers or WebAssembly. Hash routes such as
`/#/minerals` work on basic static hosts without rewrite rules. A host with an
SPA fallback can additionally expose clean `/minerals/:slug` routes.

For the small transfer size, configure the static host to serve the generated
`.sqlite3.br` or `.sqlite3.gz` sidecar for the canonical `.sqlite3` request
with the matching `Content-Encoding` and `Vary: Accept-Encoding`. Browsers
decode it transparently before the existing byte-length and SHA-256 checks.
The uncompressed file remains as the compatibility fallback. See
[the static app contract](public-app/README.md#precompressed-database-delivery).

After an admin approval, withdrawal, dataset activation, or provider update,
run the exporter and atomically deploy the resulting directory. Never publish
the live database, its WAL/SHM files, backups, review records, or ingestion
state. The static database contains only currently public, valid mineral rows
and their public evidence and offers. See [the static app contract](public-app/README.md)
and [the map integration handoff](docs/MAP_STATIC_APP_HANDOFF.md).

## Storage

The mutable root defaults to `data/` and can be changed with `DATA_ROOT`.

```text
data/
|-- minerals.db       SQLite authority (mutable and never tracked)
|-- backups/          bounded local pre-activation safety snapshots
|-- images/           optional registered uploaded/source media
|-- minerals/         legacy localized metadata and old report artifacts
`-- reports/          opaque request-specific generated reports
```

A clean checkout builds the database from the version-controlled legacy seed
under `data/minerals/`; a live database, WAL, backup, or report must never be
committed. Compose makes data directories mode `0700`, files mode `0600`, and
uses umask `077` for new private state.

The server does **not** expose the data root. Public file surfaces are limited
to database-registered images under `/media/images/:file` and the two published
report files under `/artifacts/:folder/:run/:file`. Databases, source JSON,
pending chunks, TeX working files, and backups remain private.

Image and report downloads are checked against current publication state on
every request. Withdrawing a mineral makes its unshared image and legacy report
URLs return `404`; image responses require revalidation and reports are never
stored by browser caches.

### Data model

Legacy compatibility uses `minerals`, `catalog`, and `images`. The research
registry uses:

- `materials`, `material_aliases`, and FTS5 `material_search` (internal neutral
  names retained for migration compatibility; public records are mineral-only);
- `evidence_sources`, `material_evidence`, and optional `material_media`;
- providers/offers plus durable sourcing request/search records;
- immutable record-review revisions and their decisions;
- datasets/source releases, stable source-record identities, immutable chunks,
  manifests/diffs/decisions, ingestion runs/items, and schema migrations.

Migrations run on startup in an immediate transaction. SQLite foreign keys,
WAL, a busy timeout, and configurable durability are enabled. Production is one
application instance on a local durable block volume; sharing SQLite WAL over
NFS/SMB or between instances is unsupported.

## Public routes and APIs

```text
GET  /livez
GET  /readyz
GET  /healthz                 compatibility alias
GET  /minerals?q=&page=
GET  /catalog                    permanent redirect to /minerals
GET  /minerals/:slug
GET  /api/minerals?q=&limit=
GET  /api/minerals/:slug
GET  /api/minerals/:slug/offers
POST /minerals/:slug/pdf
POST /api/minerals/:slug/pdf
```

Legacy browser links under `/materials` permanently redirect to the matching
mineral page and expose no separate records or API.

Report generation is limited per client and to two concurrent jobs, times out
after 60 seconds, kills abandoned LaTeX processes, compiles in a private
workspace, and publishes only PDF/HTML into opaque per-request directories. The
newest 25 runs per mineral are retained.

## Ingestion and review

Small curator/provider operations require an admin session and same-origin
request:

```text
GET  /admin/reviews
POST /admin/minerals/import
POST /admin/minerals/review
POST /admin/minerals/withdraw
POST /admin/providers/import
```

Individual mineral import accepts one object or a small array and queues
immutable candidate revisions. Provider import accepts one provider and its
offer array. See `examples/mineral-import.json` and
`examples/provider-import.json`; both are non-importable shape templates.

Complete datasets use the durable release-ingestion API: a strict,
content-addressed schema-v2 manifest with publication attribution and a
separate derived-data license; deterministic chunks of at most 500 identity
records; persisted checkpoints; idempotent retry; release-level
validation/diff; attributed browser review; an operator-managed encrypted
off-host backup plus an automatic local pre-activation snapshot; and atomic
publication. Stable identity is `(dataset, source_record_id)`, not name,
formula, or slug. Missing rows never withdraw published minerals.
Official country, bibliography, and source-status fields travel through the
same reviewed release and are stored separately from curator-authored physical
properties.

A terminal approval/rejection compacts duplicated chunk/item JSON while
retaining the manifest, frozen report, decisions/events, content hashes, and
logical counts. Keep the exact raw source and adapter inputs in encrypted
external archives; the application quarantine is not their permanent archive.

Machine adapters can stage with a separate `INGESTION_API_TOKEN` and stable
`INGESTION_ADAPTER_ID`, but that credential cannot approve or publish. Human
approval remains same-origin browser work and is attributed using the
server-side `ADMIN_REVIEWER_ID`. See [the ingestion
policy](docs/INGESTION.md) and [operations runbook](docs/OPERATIONS.md).

`ima-release stage` requires HTTPS for remote servers. Literal loopback IPs
(`127.0.0.0/8` or `::1`) may use HTTP for local development; that exception
disables proxies, and staging never follows redirects.

```text
GET  /admin/ingestion                                  browser review queue
POST /admin/ingestion/batches                          create/resume manifest
GET  /admin/ingestion/batches/:batch_id                inspect status/counts
PUT  /admin/ingestion/batches/:batch_id/chunks/:index  stage immutable chunk
POST /admin/ingestion/batches/:batch_id/finalize       freeze validation report
POST /admin/ingestion/batches/:batch_id/decision       browser-only decision
```

The create/chunk/finalize JSON routes accept either an authenticated admin or
the optional staging token; the status route also accepts either identity. The
decision route accepts only an authenticated same-origin browser form.

A successful individual import does not publish. An authenticated operator
inspects the exact candidate and evidence under `/admin/reviews`, records a
required decision note, and approves or rejects by immutable review ID.
Rejection leaves any published revision unchanged. Pending and rejected content
never appears in public search, detail, statistics, or provider availability.

Editorial approval and scientific verification are separate. Approval
preserves the candidate's scientific status. Withdrawal removes a mineral and
its offers from public surfaces and supersedes any pending revision for that
slug.

Each individual mineral source entry supports one granular record path such as
`identity.formula`, `identifiers.cas_number`,
`properties.hardness_mohs`, or `safety.handling`. Its claim contains a `value`
and may include unit, conditions, source locator, and note. See
[record and evidence validation](docs/INGESTION.md#record-and-evidence-validation).

The admin SQL endpoint is disabled by default. If explicitly enabled with
`ADMIN_SQL_ENABLED=true`, SQLite still enforces read-only statements.

## Environment

| Variable | Default | Purpose |
|---|---:|---|
| `PORT` | `7979` | HTTP port |
| `BIND_ADDRESS` | `127.0.0.1` | Native bind address |
| `DATA_ROOT` | `data` | Private mutable storage root |
| `DEFAULT_LANG` | `en` | Default UI/profile language |
| `ADMIN_PASSWORD` | required | Admin password; minimum 12 characters |
| `ADMIN_REVIEWER_ID` | `local-admin` | Stable server-side review actor; override in production |
| `INGESTION_API_TOKEN` | unset | Optional machine-staging bearer secret; minimum 32 characters and no publication authority |
| `INGESTION_ADAPTER_ID` | unset | Stable adapter actor; required with an ingestion token |
| `INGESTION_BATCH_MAX_BYTES` | `67108864` | Maximum stored canonical chunk-plus-item payload per staged batch (64 MiB) |
| `INGESTION_QUARANTINE_MAX_BYTES` | `536870912` | Maximum of that payload across `receiving`, `ready`, and `needs_attention` batches (512 MiB) |
| `INGESTION_ABANDONED_HOURS` | `336` | Tombstone an inactive `receiving` batch and reclaim its chunks/items after 14 days |
| `SQLITE_DURABILITY` | `NORMAL` debug / `FULL` release | Explicitly set `FULL` for production publication durability |
| `COOKIE_SECURE` | `false` | Add `Secure` to the admin cookie behind HTTPS |
| `TRUSTED_PROXY_IPS` | unset | Exact TCP-peer IPs allowed to supply `X-Forwarded-For`, separated by commas |
| `ADMIN_SQL_ENABLED` | `false` | Enable read-only emergency SQL diagnostics |
| `OPENAI_API_KEY` | unset | AI-assisted image drafting and translation |
| `OPENAI_MODEL` | configured in `.env` | Drafting model |
| `OPENAI_TRANSLATION_MODEL` | configured in `.env` | Translation model |
| `RUST_LOG` | service info | Structured log filter |

Secrets belong in the gitignored `.env.local` or deployment secret store. Never
log bearer headers or embed credentials in URLs, examples, or source manifests.
Invalid ingestion-limit values fail initialization and readiness. These limits
bound private storage and memory exposure; they are not capacity guarantees.

## Security and truth boundaries

- Admin passwords are compared in constant time; failed login attempts are
  throttled per client address and sessions expire server-side.
- Admin cookies are `HttpOnly` and `SameSite=Strict`; production can require
  `Secure`.
- State-changing browser requests reject cross-origin submissions; machine
  staging has separate, strictly smaller bearer-token authority.
- Uploaded images are size-limited and checked by file signature.
- Search rows are rendered by auto-escaping server templates; database/model
  text is never treated as HTML.
- AI calls have connection and overall timeouts. Translation cannot modify
  chemical formulas or numeric invariants.
- Provider claims and synthetic images never become scientific evidence by
  implication.

## Tests

```bash
cargo fmt -- --check
cargo test --locked
cargo run --locked --example generate_ingestion_fixture -- generate \
  --count 6500 --output .tmp/load-6500
cargo run --locked --example generate_ingestion_fixture -- check \
  --input .tmp/load-6500
```

The 6,500-record command verifies deterministic fixture shape and hashes. It
does not measure staging throughput or production capacity. The operations
guide defines the separate latency, memory, WAL, idempotency, crash/restart,
atomic activation, and restore gates that must be measured on the deployment
topology.

## License

The application source code is licensed under
[GNU AGPL-3.0-only](LICENSE). That software license does not replace or absorb
the licenses of imported mineral datasets, evidence, media, or provider data.
Each published evidence association carries its source license and immutable
attribution snapshot; derived mineral identity data carries the explicit SPDX
license reviewed in its release manifest. Mixed records may therefore contain
claims from several independently attributed sources.
