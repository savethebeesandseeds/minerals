# Minerals Catalog (Rust + SQLite + HTML + LaTeX)

This project serves minerals from a local SQLite database and generates report artifacts per catalog item.

## Storage model

### Metadata database

Database file:

`data/minerals.db`

Core tables:

- `minerals`: broad source mineral records (canonical/base layer).
- `catalog`: published catalog entities (rocks/items) that reference one source mineral.
- `images`: global image registry.

This deployment is intentionally simplified to **3 tables total**.

### Embeddings placeholders

To support future vector indexing, these tables include `embeddings_json` (initialized as empty JSON arrays):

- `minerals`
- `catalog`
- `images`

### Image storage

All uploaded/source images are stored in one shared folder:

`data/images/`

`catalog.image_id` and `minerals.image_id` reference `images.id`.

### Report artifacts

Generated report artifacts remain per catalog slug:

`data/minerals/<slug>/`

- `report.html`
- `report.tex`
- `report.pdf`

## Migrations

On startup, if `catalog` is empty, the app auto-migrates in this order:

1. Legacy SQL schema (`catalog_entries` + related tables), if present.
2. `data/minerals.db.json` (previous JSON database), if present.
3. `data/minerals/*` folder metadata (`mineral.<lang>.json` / `mineral.json`) and legacy images.

## Query examples

Get catalog records with their source minerals:

```sql
SELECT
  c.slug AS catalog_slug,
  c.folder_name,
  m.slug AS source_mineral_slug,
  c.metadata_json,
  ci.stored_name AS catalog_image,
  mi.stored_name AS mineral_image
FROM catalog c
JOIN minerals m ON m.id = c.source_mineral_id
LEFT JOIN images ci ON ci.id = c.image_id
LEFT JOIN images mi ON mi.id = m.image_id
ORDER BY c.slug;
```

Inspect source mineral rows:

```sql
SELECT * FROM minerals ORDER BY slug;
```

## Routes

- `/` Home + language selection
- `/minerals` All Minerals sketch page (world-scale rendering scaffold)
- `/catalog` Published catalog list
- `/minerals/:slug` Catalog item detail + report generation
- `/admin` Admin login + publish/delete workflows
- `/admin/db/query` Authenticated SQL console endpoint (single-statement query/DML, schema changes blocked)

## Run in a Debian container

```bash
docker run --name minerals -it -p 7979:7979 -v "$PWD":/minerals debian:latest
```

Inside the container:

```bash
cd /minerals
apt-get update
apt-get install -y ca-certificates openssl update-ca-certificates
apt-get install -y \
  curl ca-certificates build-essential pkg-config libssl-dev \
  latexmk texlive-xetex texlive-latex-extra texlive-fonts-recommended \
  texlive-lang-arabic texlive-lang-cjk texlive-lang-chinese texlive-lang-japanese texlive-lang-other \
  fonts-noto-core fonts-noto-cjk fonts-noto-extra
curl https://sh.rustup.rs -sSf | sh -s -- -y
. "$HOME/.cargo/env"
```

## Build and run

```bash
cd /minerals
cargo run
```

Server starts on `http://localhost:7979` (override with `PORT`).

## Environment files

- `.env`: tracked in git; shared defaults and variable documentation.
- `.env.local`: gitignored; private overrides/secrets for your machine.

Current variables:

- `PORT`
- `DEFAULT_LANG`
- `ADMIN_PASSWORD` (required)
- `OPENAI_MODEL`
- `OPENAI_TRANSLATION_MODEL` (optional override; defaults to `OPENAI_MODEL`)
- `OPENAI_API_KEY`

## Web usage

1. Open `http://localhost:7979/`.
2. Select language.
3. Open `http://localhost:7979/admin` and authenticate.
4. Upload an image (optional context).
5. Generate AI draft, review fields, and publish.
6. Publish writes localized metadata into SQLite and stores image files in `data/images`.
7. Open `/catalog`, pick a record, and generate reports.

## API usage

Generate a PDF + HTML report for one catalog slug:

```bash
curl -X POST http://localhost:7979/api/minerals/mineral.silicate.0xabc123/pdf \
  -H "content-type: application/json" \
  -d '{
    "audience": "resource geologist",
    "purpose": "mine planning",
    "site_context": "north pit phase-2"
  }'
```

## Project structure

- `src/main.rs`: routes, admin/auth, AI drafting/publish flow.
- `src/models.rs`: SQLite schema, persistence, migrations, image linkage.
- `src/agent.rs`: analysis chain.
- `src/pdf.rs`: HTML/LaTeX rendering + `latexmk`.
- `src/web.rs`: Askama response/template structs.
- `static/all_minerals.html`: world-scale sketch page.
- `static/index.html`: published catalog page.
- `static/mineral.html`: detail + report generation page.

## Notes

- If PDF generation fails, the UI displays `latexmk` output.
- Metadata authority is `data/minerals.db`; image authority is `data/images`.
