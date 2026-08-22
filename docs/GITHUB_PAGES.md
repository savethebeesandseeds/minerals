# GitHub Pages deployment

Waajacu's public catalog can be hosted safely as a static GitHub Pages site.
The repository and the generated public release may be public; the operational
database, administrator credentials, ingestion state, backups, and tunnel
configuration must remain local to the private service.

This project uses one branch, `main`. It does not create a `gh-pages` branch.
The deployment path is:

```text
tagged commit on main
        |
        v
local export from the reviewed operational database
        |
        v
sanitized GitHub Release asset + recorded SHA-256
        |
        v
manually dispatched Pages workflow verifies everything
        |
        v
GitHub Pages, optionally served through Cloudflare
```

The separation is deliberate. A checkout does **not** contain the current real
catalog and therefore cannot reproduce a production release on a GitHub
runner. The tracked legacy seed contains one development record. In CI,
`tools/ci-static-release.sh` changes that fixture to one Quartz record solely
to exercise the browser tests. That one-mineral fixture must never be deployed.

## Public and private boundary

Safe to publish:

- source code and documentation;
- the checked-in `public-app` assets;
- a release produced by `export-public`, including its sanitized,
  content-addressed SQLite database and compressed sidecars; and
- the fixed-name `waajacu-public-catalog-pages.tar.gz` GitHub Release asset.

Keep outside Git:

- `.env.local` and every real password, API key, ingestion token, or tunnel
  credential;
- `data/minerals.db`, its WAL/SHM/journal files, backups, review and ingestion
  state, quarantine data, generated reports, and unpublished images;
- `.cloudflared`, local archives, release work directories, and build output;
  and
- administrator cookies, off-host backup keys, raw source archives, and any
  service-provider secret.

The tracked `.env` contains non-secret defaults only. Put local overrides in
the ignored `.env.local`, or preferably inject production values from the
service's secret store. Run this check before every commit:

```bash
python3 tools/check-public-boundary.py
```

CI runs the same check against the Git index and fails on forbidden runtime
paths and high-confidence secret formats.

## Before the first public deployment

1. Rotate the API key and administrator password currently held in the local,
   ignored `.env.local`. Ignoring a file prevents future commits; it does not
   revoke a credential that has already been exposed to a process or copied.
2. Review all catalog rows, evidence licenses, offer data, and attribution that
   the exporter will intentionally make public.
3. Commit and push the release's application source to `main`. The release tag,
   archive, and deployment must all refer to that same commit.

## Build and validate the public release

Run the exporter only on the private administration machine that has the
reviewed live database. The output directory must not already exist.

PowerShell:

```powershell
$releaseName = "catalog-2026-08-22-1"
$releaseParent = Join-Path (Resolve-Path .) "public-releases"
New-Item -ItemType Directory -Force -Path $releaseParent | Out-Null
$releaseDirectory = Join-Path $releaseParent $releaseName

cargo build --locked --release -p minerals-public-catalog --bin export-public
& .\target\release\export-public.exe `
  --data-root .\data `
  --output $releaseDirectory `
  --app-root .\public-app

& .\target\release\export-public.exe `
  --validate-release $releaseDirectory `
  --app-root .\public-app

$env:WAAJACU_CATALOG_SMOKE_DIR = $releaseDirectory
node --test .\public-app\tests.mjs
Remove-Item Env:\WAAJACU_CATALOG_SMOKE_DIR
```

Linux or macOS:

```bash
release_name=catalog-2026-08-22-1
release_parent="$PWD/public-releases"
release_directory="$release_parent/$release_name"
mkdir -p "$release_parent"

cargo build --locked --release -p minerals-public-catalog --bin export-public
./target/release/export-public \
  --data-root ./data \
  --output "$release_directory" \
  --app-root ./public-app

./target/release/export-public \
  --validate-release "$release_directory" \
  --app-root ./public-app

WAAJACU_CATALOG_SMOKE_DIR="$release_directory" \
  node --test public-app/tests.mjs
```

`--validate-release` is read-only. It rejects extra files, links, private
tables, malformed metadata, digest or size mismatches, invalid SQLite schema,
bad compressed streams, and application assets that do not exactly match the
tagged `public-app` source.

## Package and publish a GitHub Release asset

The archive must contain the release files at its root, not inside an extra
directory. Its name is fixed because the workflow refuses ambiguous assets.

PowerShell:

```powershell
$archivePath = Join-Path (Resolve-Path .) "waajacu-public-catalog-pages.tar.gz"
tar -czf $archivePath -C $releaseDirectory .
$archiveSha256 = (Get-FileHash -Algorithm SHA256 $archivePath).Hash.ToLowerInvariant()
$archiveSha256
```

Linux or macOS:

```bash
archive_path="$PWD/waajacu-public-catalog-pages.tar.gz"
tar -czf "$archive_path" -C "$release_directory" .
archive_sha256="$(sha256sum "$archive_path" | cut -d' ' -f1)"
printf '%s\n' "$archive_sha256"
```

Keep the displayed lowercase SHA-256 in the release notes or another reviewed
channel. Then:

1. tag the exact `main` commit whose `public-app` files were exported;
2. create a **draft** GitHub Release for that tag;
3. attach exactly one asset named
   `waajacu-public-catalog-pages.tar.gz`;
4. publish it as a normal release, not a prerelease; and
5. do not change the asset after recording its SHA-256. Publish a new release
   for every change.

The archive is intentionally public. It contains only the public application
and sanitized public projection. Never attach the live database or the entire
`data` directory.

## Configure and run GitHub Pages

In the repository's **Settings > Pages**, set **Source** to **GitHub Actions**.
The workflow is `.github/workflows/pages.yml` and is manual by design:

1. open **Actions > Deploy verified public catalog to GitHub Pages**;
2. choose **Run workflow** from `main`;
3. enter the published release tag and the recorded lowercase archive SHA-256;
4. wait for both `Verify public release` and `Deploy GitHub Pages` to finish;
   and
5. inspect the `github-pages` environment URL before changing DNS.

The workflow checks out the release tag, requires it to be on `main`, downloads
the fixed-name asset, checks the independently supplied archive digest, safely
extracts it without links or path traversal, rebuilds the validator from that
tag, validates the database and every static asset, runs the browser-worker
smoke test, and uploads only the verified release root to Pages.

An older release remains a rollback unit. To roll back, manually dispatch the
same workflow with that release's tag and recorded archive SHA-256.

## Cloudflare and the custom domain

Use separate public and private hostnames, for example:

```text
minerals.example.org       GitHub Pages public catalog
admin.example.org          private Axum administration service
```

For the public hostname:

1. [verify the custom domain for the GitHub organization](https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/verifying-your-custom-domain-for-github-pages);
2. add the hostname under the repository's **Settings > Pages**;
3. for a subdomain, create a Cloudflare CNAME to
   `savethebeesandseeds.github.io`; use GitHub's current documented A/AAAA
   records if an apex domain is required;
4. leave the record **DNS only** while GitHub provisions the certificate;
5. enable **Enforce HTTPS** in GitHub Pages and verify direct HTTPS; and
6. only then enable the Cloudflare proxy and use **SSL/TLS: Full (strict)**.

Do not use Flexible TLS. Disable Rocket Loader or any feature that rewrites
module scripts. Hash routes such as `/#/minerals` need no Pages rewrite rule.

### Cache rules

Create the rules in this order:

1. bypass cache for `/catalog-manifest.json`;
2. cache `data/catalog-<64-hex>.sqlite3` and its `.br`/`.gz` sidecars as
   eligible content with a one-year edge and browser TTL; and
3. keep `index.html`, JavaScript, CSS, and other stable-named assets short-lived
   or revalidated, because those names are reused by later releases.

SQLite is not one of Cloudflare's default cached file extensions, so the
content-addressed database needs an explicit Cache Rule. Never apply a
year-long wildcard rule to all files.

### Compression rule

Create a Cloudflare Compression Rule for the canonical content-addressed
`.sqlite3` response and enable Brotli followed by gzip. GitHub Pages does not
automatically negotiate the exporter's adjacent `.br` and `.gz` files.
Cloudflare can dynamically compress the canonical database; the browser then
receives decoded bytes and still verifies the manifest's uncompressed length
and SHA-256. If dynamic compression is unavailable, the uncompressed database
remains correct but downloads more slowly. Serving the prebuilt sidecars
exactly requires a Worker or another configurable origin.

### Security response headers

Use a Cloudflare Response Header Transform Rule on the public hostname to set:

```text
Content-Security-Policy: default-src 'self'; base-uri 'self'; object-src 'none'; script-src 'self' 'wasm-unsafe-eval'; script-src-attr 'none'; style-src 'self'; style-src-attr 'none'; img-src 'self' data:; font-src 'self'; connect-src 'self'; worker-src 'self'; child-src 'self'; manifest-src 'self'; form-action 'self'; frame-ancestors 'none'
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
Referrer-Policy: no-referrer
Permissions-Policy: camera=(), geolocation=(), microphone=()
```

The HTML already contains the compatible CSP meta policy. The response header
adds `frame-ancestors`, which a meta element cannot enforce.

## Keep administration private

GitHub Pages receives no admin code, password, cookie, writable database, or
ingestion route. Host the Axum administrator separately on
`admin.example.org`, preferably behind a VPN, Cloudflare Access, or both. Do
not cache admin responses. Set production values through the private service's
secret store, including:

```dotenv
PUBLIC_CATALOG_BASE_URL=https://minerals.example.org
COOKIE_SECURE=true
```

Only configure `TRUSTED_PROXY_IPS` for exact proxy peers that directly connect
to the service. Cloudflare Tunnel credentials belong in the ignored local
`.cloudflared` directory or the platform's managed secret store, never Git.

## Verify the live site

Use the manifest to obtain the current content-addressed database URL, then
check the important responses:

```bash
curl --fail --head https://minerals.example.org/catalog-manifest.json
curl --fail --head https://minerals.example.org/vendor/sqlite/sqlite3.wasm
curl --fail --compressed --head \
  https://minerals.example.org/data/catalog-REPLACE_WITH_64_HEX.sqlite3
```

Confirm that:

- every redirect and final application URL is HTTPS;
- the manifest is not cached;
- SQLite WASM is `application/wasm`;
- the database is served from the content-addressed path and becomes a
  Cloudflare cache hit after warm-up;
- Brotli or gzip is selected when the client requests it; and
- the CSP, framing, MIME-sniffing, referrer, and permissions headers are
  present.

Finally, load search, a mineral detail, the map, back/forward navigation, and a
mobile viewport in a fresh browser profile.
