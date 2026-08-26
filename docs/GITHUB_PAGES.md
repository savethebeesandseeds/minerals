# GitHub Pages deployment

Waajacu's public catalog is a standalone static website. GitHub Pages serves
files; it does not run the Rust administrator, hold a writable database, or
receive service credentials.

This project uses one branch, `main`. There is no `gh-pages` branch and no
release archive to keep synchronized by hand. The publication path is:

```text
commit pushed to main
        |
        v
assemble exact committed public-app + public-catalog
        |
        v
validate schema, hashes, compression, boundaries, and browser worker
        |
        v
deploy only the assembled directory to GitHub Pages
        |
        v
hash the live files until they match that commit
```

The workflow is [`.github/workflows/pages.yml`](../.github/workflows/pages.yml).
It runs automatically after every push to `main`; `workflow_dispatch` exists
only to rerun the same commit without changing the deployment contract.

## Public and private boundary

Intentionally public and version-controlled:

- all source code and documentation;
- the exact static application files allowlisted by `export-public`;
- `public-catalog/catalog-manifest.json`; and
- the matching sanitized, content-addressed SQLite database and its Brotli and
  gzip representations under `public-catalog/data/`.

Local service state that must never enter Git or Pages:

- `.env.local` and real passwords, API keys, ingestion tokens, or tunnel
  credentials;
- `data/minerals.db`, WAL/SHM/journal files, backups, reviews, ingestion
  state, quarantine data, generated reports, and unpublished images;
- `.cloudflared`, local archives, build output, and release work directories;
  and
- administrator cookies, backup keys, and service-provider secrets.

The public catalog is rebuilt from scratch with a small public-only schema; it
is not the operational database with tables removed. The validator rejects
private tables, unknown columns, malformed metadata, bad foreign keys, broken
FTS, digest or size mismatches, invalid compressed streams, extra files, and
links. The repository boundary check also scans public SQLite text for
high-confidence credential formats, requires the worktree bytes to equal the
staged Git blobs, and uses Node.js's built-in Brotli and gzip decoders to prove
both sidecars contain that same sanitized database:

```bash
python3 tools/check-public-boundary.py
```

Because the repository is public, a committed catalog snapshot remains in Git
history even after a later snapshot removes a row. Review evidence, licensing,
offers, attribution, and descriptive text as material intended for permanent
public distribution. Each current database file is below GitHub's 100 MiB
single-file limit; use another public data distribution mechanism before a
future raw snapshot approaches that limit.

## Updating application code

Edit `public-app/`, test, commit, and push to `main`. The Pages workflow
combines that exact commit with the current committed catalog automatically.
It never takes application files from an older release or tag.

The workflow:

1. checks out `github.sha`;
2. runs the repository boundary check and publication-tool tests;
3. builds the pinned Rust release assembler;
4. creates a fresh temporary release from only `public-app` and
   `public-catalog`;
5. runs full public-database validation and the real SQLite-WASM worker tests;
6. refuses to upload if a newer `main` commit already exists;
7. uploads only that validated temporary directory;
8. deploys it through the official GitHub Pages action; and
9. retries live HTTPS downloads for up to 15 minutes, requiring every
   allowlisted app asset, the manifest, and both compressed database sidecars
   to match the pushed bytes.

A workflow run is successful only after the live verification passes. GitHub
Pages and Cloudflare may briefly retain stable-named files for up to their
configured cache lifetime; the verification job waits for convergence.

## Updating the public mineral data

Only export on the private administration machine that holds the reviewed live
database. Always use a fresh output directory.

PowerShell:

```powershell
$review = ".\public-releases\review-2026-08-22-1"
cargo build --locked --release -p minerals-public-catalog --bin export-public
& .\target\release\export-public.exe `
  --data-root .\data `
  --output $review `
  --app-root .\public-app

& .\target\release\export-public.exe `
  --validate-release $review `
  --app-root .\public-app
```

Linux or macOS:

```bash
review="./public-releases/review-2026-08-22-1"
cargo build --locked --release -p minerals-public-catalog --bin export-public
./target/release/export-public \
  --data-root ./data \
  --output "$review" \
  --app-root ./public-app

./target/release/export-public \
  --validate-release "$review" \
  --app-root ./public-app
```

Review the generated content. Then update only these tracked artifacts:

```text
public-catalog/catalog-manifest.json
public-catalog/data/catalog-<manifest SHA-256>.sqlite3
public-catalog/data/catalog-<manifest SHA-256>.sqlite3.br
public-catalog/data/catalog-<manifest SHA-256>.sqlite3.gz
```

Remove the previous three content-addressed files from
`public-catalog/data/` only after the new four-file set is ready. The
assembler rejects extra or mismatched snapshots. Test the exact tracked
combination in another fresh directory:

```bash
./target/release/export-public \
  --assemble-catalog ./public-catalog \
  --output ./target/pages-review \
  --app-root ./public-app

WAAJACU_CATALOG_SMOKE_DIR=./target/pages-review \
  node --test public-app/tests.mjs
python3 tools/check-public-boundary.py
```

Commit the reviewed snapshot and push `main`. No tag, GitHub Release, archive,
manual checksum entry, or secondary branch is needed.

## Rollback and recovery

Git already preserves every committed state. If a public change needs to be
undone, make a normal new commit on `main` that restores the reviewed files
(for example with `git revert` after inspecting the exact commit), then push.
This does not delete or rewrite history. The same automatic validation and live
hash proof applies to the corrective commit.

Never use a force-push, branch replacement, or history rewrite as a deployment
operation.

## Cloudflare and the custom domain

The current public hostname is:

```text
minerals.waajacu.com       GitHub Pages static public catalog
```

A future administrator service must use a separate private hostname such as
`admin.waajacu.com`. The public catalog does not need Cloudflare Tunnel or an
application server.

For a Pages subdomain, configure the repository custom domain first, then use a
Cloudflare CNAME to `savethebeesandseeds.github.io`. Keep GitHub's verified
domain TXT record. Provision GitHub HTTPS before optionally enabling the
Cloudflare proxy; if proxied, use **SSL/TLS Full (strict)**, never Flexible.
Disable Rocket Loader or any feature that rewrites module scripts.

Hash routes such as `/#/minerals` and `/#/about` need no rewrite rule. The
fragment stays in the browser and is not part of the HTTP request.

### Cache rules

Use short-lived or revalidated caching for:

- `/` and `/index.html`;
- `/catalog-manifest.json`; and
- stable-named JavaScript and CSS.

The content-addressed database and its `.br`/`.gz` sidecars may use a
one-year immutable cache. SQLite is not a default Cloudflare cached extension,
so add a narrow Cache Rule if edge caching is wanted. Do not apply a one-year
wildcard to the whole site.

The browser prefers the committed `.sqlite3.gz` sidecar and verifies the
decoded database against the manifest. Cloudflare may also dynamically
compress the canonical `.sqlite3` response. The raw file remains a correct
fallback.

### Security response headers

Use a Cloudflare Response Header Transform Rule on the public hostname:

```text
Content-Security-Policy: default-src 'self'; base-uri 'self'; object-src 'none'; script-src 'self' 'wasm-unsafe-eval'; script-src-attr 'none'; style-src 'self'; style-src-attr 'none'; img-src 'self' data:; font-src 'self'; connect-src 'self'; worker-src 'self'; child-src 'self'; manifest-src 'self'; form-action 'self'; frame-ancestors 'none'
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
Referrer-Policy: no-referrer
Origin-Agent-Cluster: ?1
Permissions-Policy: camera=(), geolocation=(), microphone=(), tools=(self)
```

The HTML contains a compatible CSP meta policy. The response header adds
`frame-ancestors`, which a meta element cannot enforce. The origin-agent
header makes WebMCP's required origin-keyed agent cluster explicit, while the
permissions policy limits site-tool discovery to the catalog's own origin.
The live deployment verifier also requires a JavaScript MIME type and
non-immutable caching for `.mjs` files. Because native Pages cannot add custom
security headers, it permits those headers to be absent, but rejects an
explicit `Origin-Agent-Cluster: ?0` or a `tools` policy broader or narrower
than `self`.

## Keep administration separate

GitHub Pages receives no admin routes, password, cookie, writable database, or
ingestion endpoint. Run the Rust administrator separately and keep it on
loopback, a private network, Cloudflare Access, or a similarly controlled
boundary. Do not cache its responses. Set service values through private
environment or secret storage, for example:

```dotenv
PUBLIC_CATALOG_BASE_URL=https://minerals.waajacu.com
COOKIE_SECURE=true
```

Cloudflare credentials belong in its managed secret store or the ignored local
`.cloudflared` directory, never in this public repository.
