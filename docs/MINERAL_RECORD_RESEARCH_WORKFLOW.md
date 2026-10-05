# Record-by-record mineral research

The content objective covers every one of the existing 6,226 mineral records.
Codex performs the research, identifies relevant evidence, proposes structured
facts, writes useful draft descriptions, records gaps, and prepares reviewable
batches. Dataset imports such as COD supply evidence for parts of this work;
they do not replace the record-by-record effort.

The [questionnaire](MINERAL_RECORD_QUESTIONNAIRE_V1.md),
[source admission decisions](MINERAL_SOURCE_ADMISSION_V1.md), and
[enrichment backlog](MINERAL_RECORD_ENRICHMENT_BACKLOG.md) continue to govern
scientific content. Model knowledge helps choose searches and recognize
possible inconsistencies. Every proposed scientific fact still needs an exact
source locator. Research drafts may be written before source admission and
module implementation, but remain private proposals and never become approved
claims simply because a model wrote or checked them.

## Private workbench

`data/pilots/mineral-record-research-v1/` holds manually researched batches,
immutable source HTML, retrieval receipts, a queue containing every existing
public ID, and a readable progress report. This mutable research state is
ignored by Git and is not an importer input. Preserve it with the data-root
backup procedure.

The first batch covers Quartz, Calcite, Gypsum, Fluorite, and Talc. NPS
educational pages supply starting evidence; their scientific source-class fit
and specific field mappings are pending. NPS rights guidance distinguishes
government-created text from third-party images and protected marks. Only
HTML evidence is captured; linked media and assets are excluded. No public
source-admission decision is changed by this batch.

The workbench explicitly preserves unresolved points, including group versus
species statements, fluorite excitation conditions, gypsum formula typography,
and source-label errors. It does not infer an occurrence point, a safety
conclusion, or a species relationship from those statements.

Run the workbench inside the existing managed admin container:

```powershell
docker compose exec -T --user 0:0 admin bash tools/container-task.sh record-research capture
docker compose exec -T --user 0:0 admin bash tools/container-task.sh record-research prepare
docker compose exec -T --user 0:0 admin bash tools/container-task.sh record-research verify
```

`capture` retrieves only the four explicitly reviewed NPS URLs in the first
manual batch, sequentially at least 12 seconds apart. It caches exact bytes
under SHA-256, records URLs and retrieval time, bounds response size, and
rejects redirects outside the reviewed HTTPS NPS hosts. Existing captures are
verified and reused without network requests. It does not crawl the catalog
or contact COD.

`prepare` verifies the public catalog hash, opens SQLite read-only, checks all
6,226 unique IDs, validates draft question keys and source locators, binds
source receipts and input hashes, and creates deterministic queue/report
files. Generated revisions are preserved as content-addressed objects. Manual
batch files are never rewritten. `verify` reproduces those artifacts offline
and compares their bytes and hashes.

Queue states `not_started` and `draft_prepared` describe research work performed.
They are not questionnaire coverage, applicability, scientific approval, or
publication states. Remaining question counts likewise measure draft research
gaps and do not assert that every question applies to every mineral.

## Continue in bounded batches

1. Take the next unresearched public IDs from the queue. Keep names and formulas
   tied to the frozen public release; never use fuzzy merging.
2. Research the questionnaire's useful profile fields, including physical
   properties, geology, history, relationships, and editorial context.
3. Store proposed facts with source IDs, exact locators, qualifiers, and draft
   status. Record the search gaps and the next useful sources explicitly.
4. Write a readable draft supported by at least two of that mineral's proposed
   claim IDs. Scientific review must resolve those claims before public use.
5. Add a new immutable batch file and regenerate the queue. Existing batches
   need a separately recorded supersession procedure if revised; do not erase
   their original research or review history.
6. Complete source fit, admission, mappings, scientific review, and the required
   independent checks. Stage content through the existing review system only
   once its implementation and contract support the corresponding fields.

Report researched records, candidate claims, reviewed claims, and published
records separately. Visiting every record is the execution goal; unsupported
fields remain gaps. Complete scientific enrichment still follows the
questionnaire's explicit coverage and release criteria.
