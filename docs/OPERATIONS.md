# Waajacu's Minerals operations

The private admin service is intentionally a single-writer Rust/Axum
application backed by SQLite. A complete image-free mineral catalog is small
for SQLite; ingestion correctness, source lineage, recovery, and predictable
publication are the operational constraints.

## Production topology

- Run exactly **one** application instance against a database.
- Keep `/app/data` on a local, durable block volume. Do not place the SQLite
  database or its WAL on NFS, SMB, a Docker network filesystem, or a volume
  concurrently mounted by another application instance.
- Put a TLS-terminating reverse proxy in front of the loopback-bound host port.
- Treat `/app/data` as private. It contains the registry, pending candidates,
  review history, raw-source metadata, and images when present.
- Images are optional. An image-free catalog is fully valid and is the expected
  first complete release.
- Run media on object storage and move the writer to PostgreSQL only when the
  product actually needs multiple independent writers or horizontal replicas.

Static catalog releases use a separate immutable deployment root. Follow
[`deploy/README.md`](../deploy/README.md) to export under
`/srv/waajacu/releases/<release-id>`, validate the content-addressed database,
and atomically switch `/srv/waajacu/current`. The included nginx configuration
listens on port 8080 and supplies the required SQLite/WASM MIME types, cache
policy, strict production CSP response header, and gzip transfer compression.
This production origin is separate from the local Compose `web` review service.
When production nginx runs in a container, mount all of `/srv/waajacu`
read-only so a host-side `current` symlink switch becomes visible without
restarting the container.

Each export includes immutable `.sqlite3.br` and `.sqlite3.gz` sidecars. The
included nginx configuration uses the gzip sidecar when supported; origins with
the optional Brotli static module should prefer the Brotli sidecar. All encoded
responses must retain `Vary: Accept-Encoding`, while the manifest continues to
describe and authenticate the decoded database bytes.

The Dockerfile-free local Compose stack has two services. `web` publishes the
selector-safe review site only on `127.0.0.1:18965`; `admin` publishes the
private control plane only on `127.0.0.1:7979`. Each service has its own bridge
network, so neither receives a route to the other. Both start from the same
digest-pinned Rust image and run the read-only mounted `setup.sh`. Package
installation and locked builds happen inside the containers, never on the host.
The setup phase starts as root with a reduced capability set, then permanently
drops to the service-specific non-root identity with an empty capability
bounding set before opening its listener. Temporary files are bounded,
privilege escalation is disabled, and PIDs, CPU, memory, and local log growth
are limited.

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

The `admin` service's `env_file` list loads `.env` and then `.env.local`; the
latter takes precedence for application variables such as `RUST_LOG`. Keep
development secrets in the gitignored `.env.local` and use the platform secret
store in production. The files are injected as environment values and are not
source-mounted into either container. This loading is performed by Docker
Compose: the Rust binary itself reads only its process environment and never
opens `.env` files. The public `web` review service receives no admin env file.

Compose `${MINERALS_*}` and `${PUBLIC_CATALOG_*}` interpolation for ports and
resource limits is a separate phase: it reads the invoking shell and the
project `.env`, not the service's `env_file` list. To put those deployment
values in `.env.local`, pass both files explicitly, with the local file last:

```bash
docker compose --env-file .env --env-file .env.local up -d --no-build
```

`docker compose config` expands and prints environment values. Use its
`--quiet` form for validation and never attach full rendered output to a ticket
or log when secrets are present.

Application settings:

| Variable | Production guidance |
|---|---|
| `ADMIN_PASSWORD` | Required browser-admin secret natively. Local Compose generates and persists a strong fallback when it is absent or shorter than 12 characters. |
| `ADMIN_REVIEWER_ID` | Stable, non-secret operator identifier recorded with approval/rejection decisions. Do not reuse a display name that can be reassigned. |
| `INGESTION_API_TOKEN` | Optional machine-staging bearer token of at least 32 characters. It cannot approve or publish a release. |
| `INGESTION_ADAPTER_ID` | Stable adapter identity recorded for machine-staged releases. Set it whenever the ingestion token is enabled. |
| `INGESTION_BATCH_MAX_BYTES` | Stored canonical chunk JSON plus duplicated per-item JSON per batch; defaults to 64 MiB (range 1 MiB-1 GiB). Size it from measured artifacts while staying inside the process memory budget. |
| `INGESTION_QUARANTINE_MAX_BYTES` | The same accounting across active `receiving`, `ready`, and `needs_attention` batches; defaults to 512 MiB, cannot be below the batch cap, and is bounded at 16 GiB. |
| `INGESTION_ABANDONED_HOURS` | Inactive, never-finalized `receiving` batches are tombstoned and their payload reclaimed; defaults to 336 hours (14 days), accepted range 1-8,760 hours. |
| `SQLITE_DURABILITY` | Use `FULL` in production. A less durable mode is acceptable only for disposable development data. |
| `COOKIE_SECURE` | Set `true` behind HTTPS. |
| `TRUSTED_PROXY_IPS` | Comma-separated exact IP allowlist for direct reverse-proxy TCP peers. `X-Forwarded-For` is ignored when the peer is not listed. Keep empty without a proxy; a same-host TLS proxy commonly uses `127.0.0.1,::1`. |
| `ADMIN_SQL_ENABLED` | Keep `false`; enable only for short, supervised, read-only diagnostics. |
| `PUBLIC_CATALOG_BASE_URL` | HTTPS base URL for links to the deployed static catalog. The tracked Compose `.env` points at the local review service on `http://127.0.0.1:18965`; native runs default to unset, and literal-loopback HTTP is accepted only for development. |
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
| `MINERALS_MEMORY_LIMIT` | `2g` |
| `MINERALS_MEMORY_RESERVATION` | `512m` |
| `MINERALS_LOG_MAX_SIZE` | `10m` |
| `MINERALS_LOG_MAX_FILES` | `5` |
| `MINERALS_STOP_GRACE_PERIOD` | `90s` |
| `MINERALS_HOST_PORT` | `7979` |
| `MINERALS_BIND_ADDRESS` | `127.0.0.1` |
| `PUBLIC_CATALOG_HOST_PORT` | `18965` |
| `PUBLIC_CATALOG_BIND_ADDRESS` | `127.0.0.1` |

Raise a resource or quarantine limit only after measuring the intended release
artifact on the deployment topology. Always retain a memory limit and log
rotation; a higher safety ceiling is not evidence of higher capacity.

When `ADMIN_PASSWORD` is absent or too short, `setup.sh` writes a generated
local secret to `/runtime/admin-password` in the private
`minerals-admin-runtime` named volume with mode `0400`. It survives ordinary
container replacement and `docker compose down`. Retrieve it only from a
trusted local terminal:

```bash
docker compose exec -T --user 0:0 admin cat /runtime/admin-password
```

The command prints the password. Never attach its output to a ticket or log,
paste it into chat, or capture it in shell automation. The file does not exist
when a valid `ADMIN_PASSWORD` is configured explicitly.

## Data-directory permissions

No UID/GID exports or `chmod 777` workaround are needed. On each `admin` start,
`setup.sh` targets only the real `/app/data` directory, retains a non-root bind
mount owner's numeric UID/GID (or uses the configured UID/GID fallback for a
root-owned Docker Desktop mount), and normalizes ownership without following
symlinks. It sets every private directory to `0700`, every regular file to
`0600`, and a `077` umask for new database, WAL, image, and backup files. The
process then clears all capabilities and executes the service under that
non-root identity.

The tracked `data/minerals/` tree is the immutable legacy import seed for a
clean checkout. `data/minerals.db`, its sidecars, and backups are private
runtime state and are ignored by Git. Never add a live database to
source control; take backups using the database procedure below.

## Build and start

```bash
docker compose config --quiet
docker compose up -d --no-build
docker compose ps
curl --fail http://127.0.0.1:18965/healthz
curl --fail http://127.0.0.1:18965/catalog-manifest.json
curl --fail http://127.0.0.1:7979/livez
curl --fail http://127.0.0.1:7979/readyz
docker compose logs --tail=200 web admin
```

There is no Dockerfile and `--build` must not be used. Compose pulls the
digest-pinned Rust base image when it is absent. On first start, each service's
`setup.sh` installs its explicit Debian package profile inside that container,
then compiles the required locked Cargo target from read-only source mounts.
The shared `minerals-cargo-registry`, `minerals-cargo-git`, and
`minerals-cargo-target` named volumes serialize and retain build work;
`minerals-admin-runtime` retains the executable and generated local password,
while `minerals-web-runtime` retains the validated selector-review release. The
services use `restart: on-failure:3`, so setup or runtime failures stop after
three automatic retries rather than entering an unbounded install/build loop.

Update the Rust image pin deliberately by changing the matching audited digest
in both `compose.yaml` and `setup.sh`, then validate and rehearse the stack. The
host ports bind only to loopback by default. If a controlled environment must
bind directly, set the corresponding `MINERALS_BIND_ADDRESS` or
`PUBLIC_CATALOG_BIND_ADDRESS` and enforce network access outside the container.
The selector-safe Nginx configuration mounted by `web` is for local review only;
production continues to use the strict `deploy/nginx/minerals-static.conf`.

## Release-ingestion runbook

The durable release API is resumable and idempotent; details and payload
contracts are in [INGESTION.md](INGESTION.md). Operationally:

The official IMA extractor requires exactly CPython `3.12.13` as well as every
pin in `scripts/ima-requirements.txt`; it exits before either PDF engine runs
when the implementation or patch version differs.

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
   `ima-release stage` permits remote HTTPS only, never follows redirects, and
   permits HTTP solely for a literal loopback IP with proxy use disabled.
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
docker compose ps
docker compose logs --tail=200 web admin
docker compose exec -T --user 0:0 admin sh -c \
  'du -h /app/data/minerals.db /app/data/minerals.db-wal 2>/dev/null || true'
```

The runtime dependency profile intentionally does not install a `sqlite3`
executable. Use a pinned operator workstation/container with access to the
bind-mounted `./data` path and run it as the same numeric UID/GID as the
service. During a quiet maintenance window, stop request traffic, then verify
and optimize:

```bash
sqlite3 ./data/minerals.db \
  'PRAGMA quick_check; PRAGMA foreign_key_check; PRAGMA optimize;'
sqlite3 ./data/minerals.db \
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
sqlite3 ./data/minerals.db ".backup './data/minerals.db.pre-activation'"
sha256sum ./data/minerals.db.pre-activation
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
an intentional conflict batch. Conflict injection requires `--variant changed`
and a `--count` of at least 2 so the request cannot silently produce a
non-conflicting fixture. Test maximum legal field sizes with targeted fixtures;
do not bloat every capacity record.

Required destructive scenarios include identical retry, invalid final record,
disconnect/resume, kill during staging and activation, concurrent submissions,
concurrent admin reads during ingestion, backup/restore, disk full, read-only
storage, and a long-lived reader during checkpoint.

Acceptance targets on a 2-vCPU/1-GiB instance:

| Gate | Required result |
|---|---|
| 500-record chunk | p95 at or below 5 seconds |
| 6,500-record staging | at or below 90 seconds |
| Admin reads | p95 at or below 250 ms normally and 500 ms during ingestion |
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

Before a pinned base-image or dependency update, make and verify a complete
backup, rehearse the updated `setup.sh` and Compose stack against a copy, and
exercise both services' probes, admin authentication, ingestion status, and a
verified public export. Rolling back the container inputs does not roll back the
bind-mounted database; restore the matching data snapshot if a migration is not
backward compatible.

```bash
docker compose down
```

`down` removes the two containers and their separate networks, but retains the
named runtime/Cargo volumes and the host-bound `./data` directory. To reset only
the containers later, run the same `docker compose up -d --no-build` command.

```bash
docker compose down -v
```

`down -v` additionally deletes `minerals-admin-runtime`,
`minerals-web-runtime`, and the three named Cargo cache volumes. That discards
the generated admin password and forces a cold Cargo/release build on the next
start. Package installation and setup run in newly created containers after
either form of `down`; ordinary `down` can still reuse the named build caches
and password. Neither command deletes the host bind `./data`. Use `-v` only
when that reset is intentional. Never run destructive cleanup against the data
root unless a verified external backup exists.
