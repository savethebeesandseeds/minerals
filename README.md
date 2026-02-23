# Minerals Folder Builder (Rust + HTML + LaTeX)

This project serves minerals from a folder-first structure and generates report artifacts per mineral.

## Folder model

Each mineral lives in:

`data/minerals/mineral.<family>.0x<short-hex-id>/`

The folder contains at minimum:

- `mineral.en.json` (authoritative English metadata)
- `mineral.<lang>.json` localized metadata files (`en`, `es`, `cs`, `zh`, `ar`, `fr`, `de`, `pt`, `hi`, `ja`)
- `mineral.json` (legacy fallback copy, currently aligned to English)
- `image.<ext>` (uploaded via admin)
- generated artifacts: `report.html`, `report.tex`, `report.pdf`

## Run in a Debian container

```bash
docker run --name minerals -it -p 7979:7979 -v "$PWD":/minerals debian:latest
```

Inside the container:

```bash
cd /minerals
apt-get update
apt-get install -y ca-certificates openssl update-ca-certificates
apt-get install -y curl ca-certificates build-essential pkg-config libssl-dev latexmk texlive-latex-extra texlive-fonts-recommended
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
- On startup, the app loads `.env` first, then `.env.local` (local values override shared defaults).

Current variables:

- `PORT`
- `DEFAULT_LANG` (default UI language code; fallback when no `lang` cookie is present)
- `ADMIN_PASSWORD` (required)
- `OPENAI_MODEL`
- `OPENAI_TRANSLATION_MODEL` (optional override for translation calls; defaults to `OPENAI_MODEL`)
- `OPENAI_API_KEY` (set in `.env.local`)

## Web usage

1. Open `http://localhost:7979/`.
2. On Home, select language and continue to `/minerals`.
3. Open `http://localhost:7979/admin`.
4. Login with password (env `ADMIN_PASSWORD`).
5. In admin, upload an image (optionally add operator context).
6. Click **Suggest Fields With OpenAI** to generate common name, description, and technical fields.
7. Review/edit the English form and click **Publish Mineral**.
8. Publish writes `mineral.en.json` and attempts translation into all 10 language files.
9. Open the mineral page and generate report artifacts (`report.html` and `report.pdf`) in that mineral folder.

## API usage

Generate a PDF + HTML report for one mineral:

```bash
curl -X POST http://localhost:7979/api/minerals/mineral.silicate.0xabc123/pdf \
  -H "content-type: application/json" \
  -d '{
    "audience": "resource geologist",
    "purpose": "mine planning",
    "site_context": "north pit phase-2"
  }'
```

Example response:

```json
{
  "pdf_path": "/data/minerals/mineral.silicate.0xabc123/report.pdf",
  "html_path": "/data/minerals/mineral.silicate.0xabc123/report.html",
  "summary": "For resource geologist ..."
}
```

## Project structure

- `src/main.rs`: HTTP routes, admin session/auth, OpenAI-assisted mineral drafting + publish.
- `src/models.rs`: mineral models + filesystem loader (`data/minerals`).
- `src/agent.rs`: analysis chain (metrics -> summary -> recommendations).
- `src/pdf.rs`: HTML/LaTeX rendering and `latexmk` execution.
- `src/web.rs`: Askama response + template structs.
- `static/app.css`: shared UI design system and navigation styling.
- `static/home.html`: language selector home page.
- `static/index.html`: all-minerals catalog page.
- `static/mineral.html`: mineral detail + report generation page.
- `static/admin.html`: admin login + create mineral page.
- `static/about.html`: about page.
- `static/report.html`: generated static HTML report template.
- `static/report.tex`: generated PDF template.
- `static/logo_transparent.png`: preferred UI logo asset.

## Notes

- If PDF generation fails, the UI shows `latexmk` output in-page.
- Rendering is fully folder-backed: creating a valid mineral folder is sufficient for server-side discovery.
