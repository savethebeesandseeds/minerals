# Static catalog deployment

This directory provides a small Linux/nginx deployment workflow for the public
catalog. It keeps publication separate from the private admin server:

```text
/srv/waajacu/
|-- releases/
|   |-- release-2026-08-21-1/
|   `-- release-2026-08-21-2/
`-- current -> releases/release-2026-08-21-2
```

Every directory under `releases/` is immutable. `current` is the only mutable
entry and is replaced with one same-directory `rename(2)` operation. Retaining
an earlier release makes rollback the same operation as activation.

## Export and activate

Create the deployment root once, then export to a fresh versioned directory:

```bash
sudo install -d -m 0755 /srv/waajacu/releases
cargo build --locked --release -p minerals-public-catalog --bin export-public
sudo ./target/release/export-public \
  --data-root ./data \
  --output /srv/waajacu/releases/release-2026-08-21-1 \
  --app-root ./public-app
```

Activate only after the exporter succeeds:

```bash
sudo sh deploy/activate-static-release.sh \
  /srv/waajacu release-2026-08-21-1
```

The activation script rejects unsafe release names, symlinked release content,
private SQLite artifacts, missing runtime files, and a catalog database whose
actual SHA-256, filename, byte count, or manifest declarations disagree. It
never edits a release directory. It also refuses to replace `current` when
that path is a real file or directory.

To roll back, activate a retained older release:

```bash
sudo sh deploy/activate-static-release.sh \
  /srv/waajacu release-2026-08-21-1
```

Pruning is deliberately not automated. Delete an old release only after it is
no longer active, the new release has been observed in production, and the
retention policy permits removal. `readlink /srv/waajacu/current` shows the
active release.

The script targets Linux and requires GNU `mv` plus `sha256sum`. The deployment
root, its `releases` directory, the temporary symlink, and `current` must remain
on one filesystem for the atomic rename guarantee.

## nginx

[`nginx/minerals-static.conf`](nginx/minerals-static.conf) listens on port 8080
and serves `/srv/waajacu/current`. It provides:

- `text/javascript` for `.js` and `.mjs`;
- `application/wasm` for `.wasm`;
- the registered `application/vnd.sqlite3` type for `.sqlite3`;
- `no-cache` for the manifest and `no-store` for stable-named application assets;
- a one-year immutable policy only for the SHA-256-named catalog database;
- verified prebuilt gzip delivery for SQLite plus dynamic gzip transfer
  compression for WASM, JavaScript, CSS, and JSON (deployments with the
  optional Brotli module may prefer the exported `.br` sidecar); and
- the application's CSP as an HTTP header, including `frame-ancestors 'none'`,
  plus MIME-sniffing, framing, referrer, origin-agent-cluster, and
  self-only WebMCP permission defenses.

Install it on a conventional nginx host with:

```bash
sudo install -m 0644 deploy/nginx/minerals-static.conf \
  /etc/nginx/conf.d/minerals-static.conf
sudo nginx -t
sudo systemctl reload nginx
```

Terminate TLS in nginx or an upstream proxy before exposing the catalog to the
internet. Set the private admin service's `PUBLIC_CATALOG_BASE_URL` to the
external HTTPS origin. Literal loopback HTTP, such as
`http://127.0.0.1:8080`, is accepted only for local development.

For a container, mount the **whole deployment root**, not the `current`
symlink. Mounting only a symlink can pin the bind mount to the old target and
hide later atomic switches:

```bash
docker run --rm --name waajacu-catalog \
  -p 127.0.0.1:8080:8080 \
  -v /srv/waajacu:/srv/waajacu:ro \
  -v "$PWD/deploy/nginx/minerals-static.conf:/etc/nginx/conf.d/default.conf:ro" \
  nginx:stable-alpine
```

## Verification

Validate configuration before reload and inspect the public response:

```bash
nginx -t
curl --fail --silent --show-error \
  http://127.0.0.1:8080/catalog-manifest.json
curl --fail --head http://127.0.0.1:8080/map/minerals_map.wasm
curl --fail --head http://127.0.0.1:8080/healthz
```

The manifest response must say `Cache-Control: no-cache`. The map and SQLite
WASM responses must use `application/wasm`. A manifest-named catalog database
must use `application/vnd.sqlite3` and the immutable cache policy. Application
and worker modules must use `text/javascript`. Production responses should
also include `Origin-Agent-Cluster: ?1` and a `Permissions-Policy` whose
`tools` feature is limited to `self`.

Because `current` can change between requests, stable-named assets always
revalidate. Only the database is immutable because its URL includes its
verified content digest. After activation, keep the old release available long
enough for rollback and incident analysis. A browser that fetched the previous
manifest immediately before the switch refreshes the manifest once if that
manifest's database is no longer available, then repeats the usual size and
SHA-256 verification against the new content-addressed database.
