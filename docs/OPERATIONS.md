# Waajacu's Minerals operations

This service is intentionally a single-writer Rust/Axum application backed by
SQLite. A complete image-free mineral catalog is small for SQLite; ingestion
correctness, source lineage, recovery, and predictable publication are the
operational constraints.

## Production topology

- Run exactly **one** application instance against a database.
- Keep `/app/data` on a local, durable block volume. Do not place the SQLite
  database or its WAL on NFS, SMB, a Docker network filesystem, or a volume
  concurrently mounted by another application instance.
- Put a TLS-terminating reverse proxy in front of the loopback-bound host port.
- Treat `/app/data` as private. It contains the registry, pending candidates,
  review history, raw-source metadata, images when present, and reports.
- Images are optional. An image-free catalog is fully valid and is the expected
  first complete release.
- Run media on object storage and move the writer to PostgreSQL only when the
  product actually needs multiple independent writers or horizontal replicas.

The image runs as unprivileged UID/GID `10001:10001`. Compose makes the root
filesystem read-only, gives `/tmp` a bounded disposable mount, drops all Linux
capabilities, prevents privilege escalation, and limits PIDs, CPU, memory, and
local log growth.

## Probes

The endpoints have deliberately different meanings:

| Endpoint | Meaning | Use |
|---|---|---|
| `GET /livez` | The HTTP process and event loop can answer. It does not promise that ingestion or the database is usable. | Orchestrator liveness/restart probe |
| `GET /readyz` | The registry can open, its required ingestion schema is present, and the data root is writable. It is not a full integrity scan. | Load balancer and Compose health check |
| `GET /healthz` | Backward-compatible health alias. | Older monitoring only; migrate to the explicit probes |

Do not run `PRAGMA quick_check` on every probe. It belongs in scheduled
maintenance and post-recovery verification. During shutdown, remove the
instance from traffic first, send `SIGTERM`, and allow the configured grace
period for the active, at-most-500-record chunk to finish.

## Configuration and secrets

Compose loads `.env` and then `.env.local`; the latter takes precedence. Keep
development secrets in the gitignored `.env.local` and use the platform secret
store in production. Neither file is sent to the Docker builder.

`docker compose config` expands and prints environment values. Use its
`--quiet` form for validation and never attach full rendered output to a ticket
or log when secrets are present.

Application settings:

| Variable | Production guidance |
|---|---|
| `ADMIN_PASSWORD` | Required browser-admin secret; at least 12 characters and preferably randomly generated. |
| `ADMIN_REVIEWER_ID` | Stable, non-secret operator identifier recorded with approval/rejection decisions. Do not reuse a display name that can be reassigned. |
| `INGESTION_API_TOKEN` | Optional machine-staging bearer token of at least 32 characters. It cannot approve or publish a release. |
| `INGESTION_ADAPTER_ID` | Stable adapter identity recorded for machine-staged releases. Set it whenever the ingestion token is enabled. |
| `INGESTION_BATCH_MAX_BYTES` | Stored canonical chunk JSON plus duplicated per-item JSON per batch; defaults to 64 MiB (range 1 MiB-1 GiB). Size it from measured artifacts while staying inside the process memory budget. |
| `INGESTION_QUARANTINE_MAX_BYTES` | The same accounting across active `receiving`, `ready`, and `needs_attention` batches; defaults to 512 MiB, cannot be below the batch cap, and is bounded at 16 GiB. |
| `INGESTION_ABANDONED_HOURS` | Inactive, never-finalized `receiving` batches are tombstoned and their payload reclaimed; defaults to 336 hours (14 days), accepted range 1-8,760 hours. |
| `SQLITE_DURABILITY` | Use `FULL` in production. A less durable mode is acceptable only for disposable development data. |
| `COOKIE_SECURE` | Set `true` behind HTTPS. |
| `ADMIN_SQL_ENABLED` | Keep `false`; enable only for short, supervised, read-only diagnostics. |
| `DEFAULT_LANG` | Default UI language; `en` when unset. |
| `OPENAI_API_KEY` | Optional for drafting/translation; not required to serve or ingest an image-free catalog. |
| `RUST_LOG` | Structured log filter. Never log credentials, bearer tokens, raw payloads, source URLs with secrets, or full review notes. |

Machine staging and human publication have separate authority. Rotate
`INGESTION_API_TOKEN` by updating the secret store and restarting the single
instance. Revoke the old token immediately; adapters must treat `401` as a hard
stop, not retry it in a loop. The browser reviewer session is the only path that
may approve a release.

Compose resource settings are deployment-time variables, not container
secrets:

| Variable | Default |
|---|---:|
| `MINERALS_CPUS` | `2.0` |
| `MINERALS_MEMORY_LIMIT` | `1g` |
| `MINERALS_MEMORY_RESERVATION` | `512m` |
| `MINERALS_LOG_MAX_SIZE` | `10m` |
| `MINERALS_LOG_MAX_FILES` | `5` |
| `MINERALS_STOP_GRACE_PERIOD` | `90s` |
| `MINERALS_HOST_PORT` | `7979` |
| `MINERALS_BIND_ADDRESS` | `127.0.0.1` |

Raise a resource or quarantine limit only after measuring the intended release
artifact on the deployment topology. Always retain a memory limit and log
rotation; a higher safety ceiling is not evidence of higher capacity.

## Data-directory permissions

On Linux, run with the UID/GID that own `./data`:

```bash
export MINERALS_UID="$(id -u)"
export MINERALS_GID="$(id -g)"
docker compose up -d --build
```

Alternatively, make the directory belong to `10001:10001`. Never make it
world-writable or run the service as root to bypass permissions. Check the
mount before a first production start:

```bash
docker compose run --rm --entrypoint sh minerals -c \
  'test -w /app/data && echo "data mount is writable"'
```

Docker Desktop may present an existing bind-mounted database as owned by root.
If the preflight fails, use a short-lived helper that targets only this mount,
then start the application under its normal identity:

```powershell
docker run --rm --mount "type=bind,source=$PWD\data,target=/data" `
  debian:bookworm-slim chown -R 10001:10001 /data
```

## Build and start

```bash
docker compose build --pull
docker compose up -d
docker compose ps
curl --fail http://127.0.0.1:7979/livez
curl --fail http://127.0.0.1:7979/readyz
```

The build uses `Cargo.lock` and a pinned Rust toolchain. Update the pin
deliberately when applying toolchain/security updates:

```bash
docker compose build --pull --build-arg RUST_VERSION=1.96.0
```

The host port binds only to loopback by default. If a controlled environment
must bind directly, set `MINERALS_BIND_ADDRESS=0.0.0.0` and enforce network
access outside the container.

## Release-ingestion runbook

The durable release API is resumable and idempotent; details and payload
contracts are in [INGESTION.md](INGESTION.md). Operationally:

1. Archive the exact raw source outside the application data directory. Record
   its checksum, retrieval time, license, source release, parser version, and
   adapter version.
2. Generate a strict schema-v2 manifest and deterministic chunks of no more
   than 500 records. Verify the human attribution party, exact work URL/title,
   canonical license URL, changes notice, non-endorsement notice, and derived
   data SPDX license before hashing. A retry must reproduce the same bytes and
   digests.
3. Create or resume the batch by posting the exact same manifest. Its canonical
   manifest hash is the idempotency identity. Machine callers may stage with
   `Authorization: Bearer <token>`; never put the token in a URL, fixture, log,
   or error report, and avoid interactive shell history in production.
4. Upload chunks in index order. Retry an uncertain response with the exact
   same release, chunk index, and payload. Never mutate a chunk after another
   chunk has been accepted.
5. Finalize validation and inspect counts, duplicates, identity conflicts,
   additions, changes, and anomalies. Missing source records must not silently
   withdraw published minerals. For `ima_identity_v1`, confirm the exact
   dataset/source pair matches the policy's exclusive binding.
6. A browser reviewer inspects the manifest, exact publication attribution,
   source and derived-data licenses, counts, diff, and UI anomaly samples, then
   records a decision note. Any statistical/risk-based sampling
   plan is an additional operator procedure. `ADMIN_REVIEWER_ID`, policy,
   manifest digest, and report digest make the server decision attributable.
7. Create, copy, and verify an encrypted off-host backup **before approval**.
   Record its external location and checksum in the release change ticket. This
   is an operator gate; the application cannot see or verify the external copy.
8. Approve only the reviewed digest. Every new report item must have a
   `target_baseline_hash`; a legacy `null` baseline or any target, absence,
   collision, dataset-head, or authority change makes approval stale. The
   server also creates and hashes a local SQLite pre-activation snapshot while
   holding the writer lock and before public changes; local backup failure
   aborts activation. Publication is all-or-nothing from a reader's
   perspective. Monitor readiness, errors, latency, DB/WAL size, and counts.
9. Run post-activation integrity and search checks, then checkpoint/optimize in
   a maintenance window.

Never approve a schema-v1 or unattributed release. Historical terminal v1
batches are audit records only; startup tombstones non-terminal v1 staging so
the adapter can restage the same archived source as a newly hashed v2 release.

HTTP bounds are 256 KiB for the manifest, 8 MiB and 500 items for each chunk,
and 16 KiB for finalize/decision actions. A manifest may describe at most
100,000 records and 10,000 chunks. These are safety ceilings, not recommended
working sizes; use 500-record chunks.

On JSON staging writes, authentication, the one-writer permit, content type,
and the endpoint body ceiling are admitted before or while the body is
extracted. Known oversized `Content-Length` requests are rejected without
buffering the declared body; extraction still bounds chunked or misreported
bodies. Stored-payload quotas necessarily run after strict parsing and canonical
serialization and must not be confused with the HTTP body ceilings.

Example machine create request (the literal placeholder is intentionally
unusable):

```bash
curl --fail-with-body \
  -H 'Authorization: Bearer REPLACE_WITH_32_PLUS_CHARACTER_SECRET' \
  -H 'Content-Type: application/json' \
  --data-binary @manifest.json \
  http://127.0.0.1:7979/admin/ingestion/batches
```

The JSON response includes `batch_id`, status, manifest/report hashes, and
received/expected chunk and record counts. Use that `batch_id` for chunk upload:

```bash
curl --fail-with-body -X PUT \
  -H 'Authorization: Bearer REPLACE_WITH_32_PLUS_CHARACTER_SECRET' \
  -H 'Content-Type: application/json' \
  -H 'X-Content-SHA256: sha256:REPLACE_WITH_CANONICAL_CHUNK_HASH' \
  --data-binary @chunks/chunk-00000.json \
  http://127.0.0.1:7979/admin/ingestion/batches/REPLACE_WITH_BATCH_ID/chunks/0
```

`X-Content-SHA256` is optional but strongly recommended; it is the canonical
chunk hash from `fixture-index.json`, not the pretty-printed file-byte hash.
Finalize after all expected chunks are acknowledged:

```bash
curl --fail-with-body -X POST \
  -H 'Authorization: Bearer REPLACE_WITH_32_PLUS_CHARACTER_SECRET' \
  -H 'Content-Type: application/json' \
  --data '{}' \
  http://127.0.0.1:7979/admin/ingestion/batches/REPLACE_WITH_BATCH_ID/finalize
```

A lost HTTP response is not proof that a write failed. Inspect a known batch:

```bash
curl --fail-with-body \
  -H 'Authorization: Bearer REPLACE_WITH_32_PLUS_CHARACTER_SECRET' \
  http://127.0.0.1:7979/admin/ingestion/batches/REPLACE_WITH_BATCH_ID
```

If the create response was lost, re-post the identical manifest to recover the
content-addressed batch ID/status/counts. Retry an uncertain chunk with the same
index, canonical hash, and body. Browser reviewers use `GET /admin/ingestion`;
approval is intentionally not demonstrated with a bearer token because
adapters have no decision authority.

### Quarantine capacity and retention

The per-batch and global byte counters include canonical chunk JSON plus the
canonical per-item JSON duplicated into indexed rows. The 64 MiB and 512 MiB
defaults are quarantine-storage guardrails, not statements about how many
records a release can process. The global counter includes only `receiving`,
`ready`, and `needs_attention` batches; terminal approved/rejected batches are
excluded.

Approval and reviewer rejection compact the raw chunk/item JSON in the same
transaction as the decision. The server records compacted chunk/record/byte
counters and a `terminal_payload_compacted` event, then removes the duplicated
payload. The manifest, report/report items, decision, events and accepted chunk
hashes, plus approved mappings/evidence, remain. Status endpoints continue to
report the logical received counts; ordinary raw-record `review_samples` are
empty after compaction, while report-derived anomaly items remain available.
The archived raw source and deterministic adapter inputs must therefore remain
in encrypted external storage for replay or deeper audit. Startup also compacts
raw payload left by terminal batches created by an older schema version.

Batch creation and chunk upload opportunistically reclaim abandoned
`receiving` batches. Age is measured from the newest stored chunk, or from
batch creation when no chunk exists. At the default 336 hours, expiry changes
the batch to `rejected`, stores note `expired_abandoned_batch`, appends
`batch_expired`, records compacted counters, removes its chunks/items, and
retains its manifest/events as an audit tombstone. This is not a background
timer, and there is currently no HTTP maintenance endpoint. Other states are
not silently expired.

The library also exposes
`expire_abandoned_mineral_ingestion_batches(data_root, actor, older_than_hours)`
for a future supervised maintenance integration. Do not build an external job
that edits the SQLite tables directly. Invalid limit values, a global limit
below the batch limit, or an invalid retention window fail startup/readiness.

## SQLite durability and maintenance

Production uses WAL mode and should use `SQLITE_DURABILITY=FULL`. The one-writer
rule is part of the storage contract, not an optimization. Deployment tooling
must prevent a second instance from sharing the file.

Inspect logs and storage:

```bash
docker compose logs --tail=200 minerals
docker compose exec minerals sh -c \
  'du -h /app/data/minerals.db /app/data/minerals.db-wal 2>/dev/null || true'
```

During a quiet maintenance window, verify and optimize:

```bash
docker compose exec minerals sqlite3 /app/data/minerals.db \
  'PRAGMA quick_check; PRAGMA foreign_key_check; PRAGMA optimize;'
docker compose exec minerals sqlite3 /app/data/minerals.db \
  'PRAGMA wal_checkpoint(TRUNCATE);'
```

`quick_check` must print `ok`; `foreign_key_check` must print no rows. Also
compare the active dataset head and release report against public catalog
counts, stable authority identities, and representative FTS results. Do not
`VACUUM` routinely; it needs free disk approximately equal to the database and
an explicit maintenance window.

Alert on readiness failure, `SQLITE_BUSY`, failed/retrying chunks, rejected
identities, run inactivity, backup age, disk free space, database/WAL growth,
RSS, and 5xx latency. Avoid mineral slugs, source URLs, and other unbounded
values as metric labels.

## Backups and activation safety

The server automatically creates a consistent local snapshot under
`DATA_ROOT/backups` inside every successful approval, hashes and records it
before public writes, and retains the newest ten local activation snapshots.
Those local files share the deployment's failure domain and are not an off-host
backup.

Before browser approval, create an additional consistent operator snapshot
while the service is running:

```bash
docker compose exec minerals sqlite3 /app/data/minerals.db \
  ".backup '/app/data/minerals.db.pre-activation'"
docker compose exec minerals sha256sum /app/data/minerals.db.pre-activation
```

Copy the snapshot, source manifest/raw archives, and any referenced media to
encrypted storage outside the deployment host. Verify the copied SHA-256 and
record the backup time, active catalog release, source release, image tag, and
schema version. Remove the temporary in-place snapshot after verification.

An image-free baseline needs only the database and raw/manifests, but once
media exists a database-only backup is incomplete. Do not copy a live data
directory as a coherent snapshot without quiescing writers; SQLite WAL and
media publication may otherwise be from different moments.

Run scheduled encrypted off-host backups and regular restore drills in
addition to the automatic local activation snapshots. An off-host backup is not
accepted until it has been restored to a fresh, isolated data root and passed
integrity, count, provenance, and sample search/detail checks.

## Recovery

When ingestion or activation fails:

1. Stop new ingestion and preserve the run ID, manifest/chunk hashes, logs, DB,
   WAL, and raw source. Do not delete or rewrite the failed run.
2. If public reads remain correct, leave them online and diagnose the private
   staged release. Resume only with identical chunks.
3. If readiness or public consistency fails, remove the instance from traffic
   and stop it gracefully. Preserve the current data directory before restore.
4. Restore the complete verified snapshot to a fresh local data directory with
   correct ownership. Start exactly one application instance.
5. Check `/livez`, `/readyz`, `quick_check`, foreign keys, FTS integrity,
   catalog membership/counts, and representative provenance/search/detail
   records before returning traffic.
6. Re-run the release from the archived raw source and deterministic manifest;
   do not hand-edit the production database to make counts agree.

For disk-full or read-only failures, free or replace storage outside the data
directory, then restart and resume from the last accepted chunk. For an
uncertain activation, compare the active release and manifest digest before any
retry. Readers must observe either the old release or the new release, never a
partial mix.

## Load and recovery acceptance targets

Generate and verify the fixtures with the repository's contract-linked Rust
tool (it imports the same manifest structs and canonical hash functions as the
server):

```bash
cargo run --locked --example generate_ingestion_fixture -- generate \
  --count 6500 --output .tmp/load-6500
cargo run --locked --example generate_ingestion_fixture -- check \
  --input .tmp/load-6500
```

This command verifies deterministic fixture construction and hashes only. It
does not exercise the HTTP API, SQLite staging, activation, concurrency, or
resource use. A production candidate has not passed the following targets until
the server-side measurements and destructive-test report are archived.

Fixture profile:

- 6,500 minerals: expected complete-catalog scale;
- image-free identity skeletons, five source-scoped aliases and a source locator
  per item;
- a changed-release mode with deterministic missing-row warnings, new
  identities, names, formulas, and nomenclature/validity changes;
- optional injected authority-identifier conflict for `needs_attention` tests.

The generator defaults to `ima_identity_v1`, which is required for an official
IMA baseline and emits synthetic `ima_number` values for the rehearsal. With
`--policy create_only_v1` it emits an empty `official_identifiers` map;
`--inject-conflicts` is rejected with that policy because only the exclusively
bound IMA policy may carry official authority identifiers.

After approving the baseline, generate its comparison release with the exact
returned batch ID:

```bash
cargo run --locked --example generate_ingestion_fixture -- generate \
  --count 6500 --output .tmp/load-6500-changed --variant changed \
  --base-batch-id REPLACE_WITH_APPROVED_BASE_BATCH_ID
```

Add `--inject-conflicts` and use a separate empty output directory to generate
an intentional conflict batch. Test maximum legal field sizes with targeted
fixtures; do not bloat every capacity record.

Required destructive scenarios include identical retry, invalid final record,
disconnect/resume, kill during staging and activation, concurrent submissions,
50-user public traffic during ingestion, backup/restore, disk full, read-only
storage, and a long-lived reader during checkpoint.

Acceptance targets on a 2-vCPU/1-GiB instance:

| Gate | Required result |
|---|---|
| 500-record chunk | p95 at or below 5 seconds |
| 6,500-record staging | at or below 90 seconds |
| Browse/search/detail | p95 at or below 250 ms normally and 500 ms during ingestion |
| Errors | no leaked `SQLITE_BUSY`; below 0.1% 5xx |
| Memory | RSS below 512 MiB |
| WAL after checkpoint | below 256 MiB |
| Identical retry | no new release, chunk, or candidate revision |
| Activation | readers see the complete old or complete new release only |
| Restart recovery | at or below 60 seconds |
| Verified image-free restore | at or below 15 minutes |

After every destructive case run `quick_check`, `foreign_key_check`, FTS
verification, manifest/count comparison, and sampled provenance checks. Keep
the reports with the tested image digest and host specification.

## Updates and shutdown

Before an image update, make and verify a complete backup, run the new image
against a copy, and exercise probes, search/detail, admin authentication,
ingestion status, and PDF generation. Image rollback does not roll back the
bind-mounted database; restore the matching data snapshot if a migration is not
backward compatible.

```bash
docker compose down
```

`down` leaves `./data` intact. Never use destructive cleanup against the data
root unless a verified external backup exists.
