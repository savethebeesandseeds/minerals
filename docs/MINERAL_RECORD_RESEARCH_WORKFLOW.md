# Record-by-record mineral research

**19 October 2026 checkpoint:** the sparse-record first pass is complete:
1,993 reviewed and saved, with zero pending, active or submitted assignments in
`broader_sparse`. The separate 4,233 catalogue jobs remain pending for a later
phase. Working evidence, drafts and review history remain private; queue
completion does not establish exhaustive knowledge or authorize publication.
The separately reviewed [19 October publication selection](PUBLIC_RESEARCH_RELEASE_2026_10_08.md#follow-up-selection--19-october-2026)
adds cited editorial summaries for the completed group and compatible source
observations while preserving the private originals and their qualifications.
Use `content-queue status` to check for subsequent changes. Keep the priority
filter below when resuming this phase so an exhausted queue does not silently
expand into the catalogue.

The current priority is the 1,993 minerals in the broader sparse-record group.
Finish that group first and report its remaining count, separately from the
full catalogue. The shared queue also retains the other 4,233 minerals for
later work; their presence does not expand this phase's scope.
The longer-term content objective covers all 6,226 mineral records.
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

## Start or resume: use the shared content queue

All content-research sessions use the shared dispatcher. Give each session a
stable, unique worker ID and run commands from `C:/Work/Minerals` through the
existing managed admin container:

```powershell
docker compose exec -T --user 0:0 admin bash tools/container-task.sh content-queue next --worker content-session-1 --compact --priority-group broader_sparse
docker compose exec -T --user 0:0 admin bash tools/container-task.sh content-queue status
```

`next` atomically reserves one unfinished mineral and returns its public ID,
name, formula, current record context and evidence, claim token, suggested
`review_file` path and instructions. Read that context before searching. Keep
the returned public ID and token; do not choose a
second record from a report, name list, old reviewer packet or draft queue.
Calling `next` again with the same worker ID resumes that worker's active
assignment. Begin or continue research only when `research_allowed` is true.
Different workers receive disjoint records, so separate sessions can continue
without manually dividing the catalogue.

`record_context.material` contains the live record, excluding embeddings and
search text; `record_context.existing_evidence` contains its saved evidence.
Use `--compact` for the initial claim: it keeps the complete live material,
including prior publication findings, and summarizes existing evidence rows.
Omit `--compact` with the same worker ID to read their full claim payloads
without taking another assignment. Summaries do not replace inspecting relevant
prior evidence, particularly accepted findings or disputed identity decisions.
The suggested review path is under
`/app/data/pilots/mineral-content-queue-v1/drafts/`, with the public ID and claim
token in its filename.

The dispatcher uses the live mineral population and reconciles saved first-pass
completion history. It gives the existing sparse-record review queue priority
before continuing through the rest of the catalogue. Existing COD references
or some publication observations alone do not establish a completed content
review. Historical queues and reports remain evidence of earlier work, not a
second assignment system.

The user has authorized four independent research sessions plus the original
session. Each holds disjoint queue claims, reads and scientifically verifies
its own findings, then uses the common batch-save command. The script handles
writer serialization and saved-record readback; routine messages between
sessions or a separate coordinator handoff are unnecessary. Do not delegate
additional reviewers or create another save routine. Each session uses its own
worker ID; a restarted session uses its previous ID. Pass
`--priority-group broader_sparse` to avoid drifting into the later catalogue.

Use this handoff for a reviewer:

```text
Read docs/MINERAL_RECORD_RESEARCH_WORKFLOW.md. Worker ID: <unique-session-id>.
Use content-queue next --priority-group broader_sparse to claim or resume one mineral. Read its existing record
and relevant primary sources, preserve supported findings and unresolved leads,
and write the review at the returned review_file path. Submit with the claim
token. Do not write minerals.db directly
or select a mineral from a separate list. Research only when research_allowed
is true. Verify each proposed fact against the source passage yourself.
Use --compact initially and expand relevant existing evidence when needed.
After submitting, claim the next item. Save roughly8–12 verified submissions
with content-save --worker <id> --confirm-reviewed. It completes saved queue
items automatically. No routine messages to other sessions.
```

## Keep the first pass moving

The priority is useful, cited coverage across the sparse records. Do not turn
each first pass into an exhaustive literature review. There is no required fact
count: keep every useful supported detail already found, without manufacturing
extra claims or stopping at an arbitrary quota.

- Start from the best existing source lead. A readable original description,
  original abstract or official notice can be enough for a useful first pass.
  Follow another source when it fills an important gap or resolves a material
  scientific issue, rather than routinely collecting several papers. A readable
  scholarly review can support useful private findings with its actual
  secondary status and access limits; do not imply its cited original was read.
- Routine records in the measured batch took about two to three minutes of
  reviewer time. Use that as a working guide, not an expiry or a reason to drop
  findings. After a publisher and one credible archive both fail, or roughly a
  minute spent on access/OCR repair yields no readable evidence, preserve the
  exact lead and access limitation and proceed with accessible evidence. Do not
  write a new PDF decoder or repeatedly retry mirrors for one first pass.
- Reuse the same review JSON shape and genuine source capture. Do not create
  per-mineral formatter programs, duplicate review copies or extra narrative
  handoffs when the immutable submission already contains the information.
  Keep source-specific measurement qualifications; avoid repeating general
  private/public policy in every individual claim.
- Make the source handoff readable: put the relevant extracted text beside
  the genuine source capture and include its path and page/section locators in
  the source metadata. Short `reviewed_passages` entries in the source object
  may link passages to claim IDs. Label extracted/OCR text as a derivative;
  it is not the original source-byte capture. Keep uncertain glyphs as leads.
- Sessions call `next` immediately after `submit` and save their own verified
  submissions in batches, normally about 8–12 records per save. Do not pay
  the full integrity-check cost for each individual mineral, or wait
  for every reviewer or a difficult record before saving a ready batch. At a
  handoff, safely save or checkpoint the smaller remainder. Keep one database
  writer at a time, enforced by the shared script.
- Each session checks every accepted claim against the cited source
  passages, using the existing genuine capture and readable text first. Do not
  routinely repeat the reviewer's search, download or whole-paper reading.
  Open the original page when a passage, number, species attribution or context
  needs checking. A source shared by several minerals can be read once, with
  each mineral's scientific attribution checked separately.
- One immutable submission, one accepted payload and one transaction receipt
  referencing the shared checkpoint supply the routine audit trail. Additional
  reports are for useful checkpoints, not a prerequisite for each mineral.
  Record actual research and save timing; do not project an unmeasured speedup.

## Finish one useful first pass

The goal is a useful, source-supported profile for every mineral. A first pass
is finished when the reviewer has inspected the existing record, read relevant
scientific sources, recorded useful supported findings, written a readable
private draft, and stated what remains missing or disputed. It need not resolve
every possible measurement, inaccessible paper or historical disagreement.

1. Read the assigned record and its previous findings. Start with an original
   description, official nomenclature notice or another relevant primary
   publication; use existing source leads before repeating searches.
2. Add useful identity/history, composition, structure, physical/optical and
   geological details that the sources actually support. The questionnaire
   supplies consistent field keys without limiting what evidence may be kept.
3. Compare species identity, specimen, composition, hydration, valence,
   polymorph, measurement method and conditions. Keep measured and calculated
   values distinct. A name or formula match is a lead, not proof.
4. Preserve sources, partial findings, disagreements and useful candidates.
   State access limitations accurately; an abstract or search excerpt is not
   full-text access, and a challenge page is not archived article content.
5. Write a readable private draft supported by the proposed observations and
   list the remaining gaps and next useful sources. Avoid spending the whole
   pass on a blocked paper when other useful evidence is available.
6. Verify the findings, submit, and save through the shared command. The queue marks the
   record complete only after the verified findings are saved to the private
   administration record and a valid save receipt is recorded.

Every proposed observation keeps its source URL/title, exact page/table/section
locator, specimen and conditions, original value and unit, uncertainty,
qualification and evidence level. Sources keep access information, retrieval
time and established license status; use `NOASSERTION` when rights are
unresolved. Hash genuine source bytes when available. Retain valuable private
findings even when their relationship or reuse status is unresolved.

Record absence as `not_reported_by_reviewed_sources` only for the sources
actually inspected. Unresearched fields remain gaps. Do not invent scalar
measurements, infer natural provenance or declare COD absence from a failed
search. Keep the protected mineral identity and prior evidence intact.

Scripts retrieve, format, coordinate or validate evidence. Reviewers read the
records and sources and make the scientific comparisons themselves. Do not
build a scraper, embeddings or another matching pipeline.

## Submit, verify and save

Save one JSON review object at the suggested `review_file` path. It contains
`material_public_id`, `canonical_name`, `review_note`, `sources`, `claims`,
`editorial_draft: {"text": "..."}` and `remaining_gaps`. Each source needs
`url`, `title`, `access` and `license_spdx`; each claim needs a unique
`claim_id`, `question_key`, `value`, `source_url` and `locator`. Retain units,
conditions, specimen context, qualifications and evidence level as applicable.
Keep source captures and useful unresolved candidates with the review.

Create review JSON and source captures with the shared private identity by
piping a Python script into the existing container launcher:

```powershell
@'
# Read/capture the reviewed URLs or serialize your manually prepared findings.
'@ | docker compose exec -T --user 0:0 admin bash tools/container-task.sh content-python
```

This runs the supplied script inside the existing container as the data owner.
Do not create root-owned shared review folders with raw `docker exec` commands;
other sessions and the save helper must be able to read the artifacts.

Keep handoffs consistent: include each source's `publisher` and `retrieved_at`,
and use this preferred shape when genuine source bytes were captured:

```json
{"archive": {"archive_path": "/app/data/.../source.pdf", "sha256": "<actual SHA-256>"}}
```

An inaccessible source or indexed passage can still be useful evidence; state
the access limit and omit a byte capture that does not exist. Older capture
formats remain valid evidence for the session to preserve and verify.
Use lists for `deferred_claims` and `remaining_candidates`, and include
`editorial_draft.supporting_claim_ids` to link the readable draft to its cited
observations. Keep research specimens distinct from formally designated types,
and uncertain OCR values as candidates until the printed source is checked.

Submit using the worker ID and token returned by `next`:

```powershell
docker compose exec -T --user 0:0 admin bash tools/container-task.sh content-queue submit --worker content-session-1 --token <claim_token> --review-file /app/data/<review-file>.json
```

Submission preserves an immutable review snapshot for verification and frees
the worker to call `next` while that review awaits its batch save. It does not
write scientific findings into `minerals.db` or publish them. The
session independently checks the sources and scientific support, then saves
accepted findings through the shared backed-up atomic review process.
Preserve prior source snapshots, observations, candidates, qualifications,
protected identity and unrelated state. Enforce one primary mineral target per
COD entry, and verify database integrity, foreign keys and preservation.

The shared save routine writes `accepted-reviews.json` with this metadata on
each accepted mineral review, binding it to the immutable queue submission:

```json
{
  "content_queue_claim": {
    "claim_token": "<claim_token>",
    "submission_sha256": "<submission_sha256>"
  }
}
```

Correct scientific values before submitting, preserving their sources and
qualifications. This metadata binds the accepted
review to its submitted assignment; an older receipt for the same mineral
cannot complete a new claim. The save receipt binds the accepted payload hash
to the actual saved database findings.

After verifying your cited findings, save your submitted batch:

```powershell
docker compose exec -T --user 0:0 admin bash tools/container-task.sh content-save --worker content-session-1 --confirm-reviewed
```

`--confirm-reviewed` attests that the session actually read the sources and
checked the scientific support; the script does not perform scientific
reasoning. It validates citations and capture hashes, serializes database
writers, ensures a shared daily checkpoint and atomically appends private
findings. Each batch checks the intended mineral rows, inserted evidence and
citations, and actual search-index matches. SQL write restrictions preserve
protected fields and prior evidence, and SQLite enforces foreign keys. Primary
COD conflicts are checked when adding primary COD assignments. The helper
reads the saved findings back and completes the corresponding queue items.
Interrupted committed saves are reconciled on the next invocation;
failed saves leave their assignments and artifacts reserved for recovery.
Full database copies are not created for individual batches. All workers share
one checkpoint for 24 hours, with only the newest two managed checkpoints kept
under `mineral-content-queue-v1/database-checkpoints`. Checkpoints are verified
and durably published before writes; retention runs only after a valid
checkpoint is available. SQLite transactions provide batch rollback, and each
batch's immutable accepted reviews and receipt provide committed-save recovery.
Whole-database integrity, foreign-key, search-index and COD audits run when a
new daily checkpoint is created. Routine batches do not hash every database
row, rescan global research totals, or reread an unchanged checkpoint file.
Version 2 receipts distinguish changed-record checks from the dated checkpoint
audit. Legacy receipts remain valid for recovery and completion.
Receipts record the checkpoint's creation time and policy; it can predate the
batch and expire after rotation. Keep the review files and receipts permanently.
Legacy per-batch backups are preserved; the new retention rule does not remove
them or source captures. Periodic local checkpoints do not replace verified
off-host backups.
There is no need to send reviews to another session. Keep all new work private.
If another batch holds the save lock, the command returns `result: busy`
without changing your submissions. Continue researching the next queue item
and retry after that review; do not repeatedly poll or wait idle. When finishing
or recovering a session, add `--wait-for-lock` to wait for the writer and finish
saving your remaining verified batch. Check the returned `completed` IDs;
submitted is not the same as saved.

The script uses the existing completion check internally. For recovery or an
older manually verified receipt, the equivalent completion command is:

```powershell
docker compose exec -T --user 0:0 admin bash tools/container-task.sh content-queue complete --public-id <material_public_id> --receipt /app/data/<commit-receipt>.json
```

`complete` verifies the receipt, accepted review payload and saved record
evidence; it does not replace scientific review. Findings and drafts stay
private until a separate reviewed public selection. Report first-pass
completion, scientific observations and public coverage separately; a completed
first pass can retain explicit gaps.

## Interruptions and handoff

- Claims survive session restarts. Use the same worker ID with `next` to
  recover the current active assignment.
- A stale claim remains reserved; elapsed time never silently authorizes
  another session to review the same record. Renew an active claim with
  `heartbeat --worker <id> --token <token>`.
- Keep partial source captures and findings in the assigned directory. Report
  a blocked or uncertain attempt using `fail --worker <id> --token <token>
  --reason "..."`. The default `uncertain` state holds the reservation; use
  `--kind failed` for a confirmed failure or `--kind needs_research` for a
  blocked review. These states are skipped until explicitly requeued.
- Explicitly requeue a record only after confirming the former worker is idle
  and reconciling its partial work: `requeue --public-id <id> --reason "..."
  --confirmed-idle`. The former token becomes invalid; history is retained.
- Never install a host toolchain or change Docker lifecycle to continue
  research. Resolve a stopped or inaccessible managed container separately.

Use the shared `status` command for current progress. Do not maintain another
independent assignment list or overwrite historical packets and receipts.
Its `priority_groups.broader_sparse` counts describe the current 1,993-record
phase; `counts` covers the full catalogue. Do not report the full-catalogue
pending count as the remaining work in the current phase.
The private queue state is
`data/pilots/mineral-content-queue-v1/queue.sqlite3`; preserve it in data-root
backups along with review artifacts. The dispatcher never changes the public
catalogue and never writes scientific content to the administration database.

## Concurrent pilot completed — 17 October 2026

Three reviewers completed two disjoint claims each. The coordinator verified
and saved six private profiles and 147 source observations, then completed all
six queue items against the actual saved records. Claim/resume, submission,
source preservation and receipt-backed completion worked without overlapping
assignments. The pilot took about 23 minutes including verification and saves.
At that historical checkpoint 64 of the 1,993 priority first passes were
complete. Obtain newer counts from `content-queue status`.

See the [pilot report and handoff](../data/pilots/mineral-content-queue-v1/pilot-2026-10-17/report.md)
for the six minerals, readable profiles, measured timings, sources, remaining
gaps and save receipts. These are private findings; no public release occurred.

The next twelve-record batch reached verified saved completion in 18 minutes
32 seconds from its first claim. Individual claim-to-submission time had a
median of 2 minutes 17 seconds and mean of 2 minutes 44 seconds; two source/OCR
outliers took 4 minutes 16 seconds and 6 minutes 8 seconds. The backed-up save
took 3 minutes 38 seconds. These are observed timings, not a forecast for the
remaining queue. They motivated the compact packets, blocked-source stop rule,
reusable source handoff and rolling work described above. Current completion
counts come from the shared queue, not this historical timing sample.

The compact context reduced a saved Baksanite handoff from 54,539 to 23,685
JSON bytes (56.6%) while preserving its full live material and the ability to
expand the same assignment. All 28 isolated queue tests passed. A separate
read-only timing sample measured one full prior-state hash at 31.36 seconds,
SQLite integrity checking at 51.04 seconds, backup-file hashing at 2.55 seconds
and public-file hashing at 0.35 seconds. These historical measurements describe
the earlier whole-database validation on every batch. On 18 October, full audits
moved to the daily checkpoint and routine saves switched to changed-record
validation. A memory-mapped I/O experiment was inconclusive and made no changes.

## Historical broader reading — 8 October 2026

The user authorized direct reading and enrichment one mineral at a time,
with source attribution and intention to cover the population. At that time,
the priority checklist contained all 1,993 records without attached COD
relationships. A COD assignment is not required to save useful publication
findings. Previously saved partial research remains available and does not
automatically count as a complete profile or completed broader review.

Aliettite was saved at checkpoint 96 / ingestion run 102: 23 source observations
and review notes from three primary publications, plus one readable private
editorial draft supported by observation IDs. Individual observations retain
source URLs, exact locators, specimen/occurrence scope, conditions,
qualifications and review state. Source metadata retains title, publisher,
retrieval/access information, licensing status and actual source-byte hashes
where available. An access/challenge response is not hashed as article content.

The findings are saved directly in the private administration database's
existing evidence schema and additive publication observations, with backups
and preservation checks. The public catalogue and source admission remain
unchanged. Unestablished fields stay explicit gaps; no scalar measurement,
natural provenance or COD absence is inferred from missing information.

Read the [historical report and checklist](../data/pilots/cod-crystallography-v1/parallel-review-v1/2026-10-06-12h/resumed-2026-10-08/report.md).
At that historical checkpoint, the checklist had one newly reviewed record and
1,992 awaiting that first pass. Of the priority records, 519 had publication
observations and 1,474 had none. The earlier twelve-hour automation was paused.
Later checkpoint receipts supersede these counts; obtain current assignments
and progress from `content-queue`.

## Historical overnight continuation — 9–10 October 2026

The user authorized further overnight reading and enrichment through 08:00
Dubai time on 10 October. The heartbeat resumed with that scope and the saved
broader queue. Up to three reviewers read disjoint individual records and
primary sources; the coordinator verified and saved additive findings in
backed-up atomic transactions. That time window has ended; this historical
authorization does not restart the heartbeat. New research stays private until a
separate public selection. Progress and source receipts are preserved in
`overnight-2026-10-09` beside the existing review checkpoints.

The separately reviewed public research interface release is documented in
[the release note](PUBLIC_RESEARCH_RELEASE_2026_10_08.md). It preserves the
page style and historical private evidence; it does not mean the broader
record-by-record queue is complete.

## Historical private workbench

`data/pilots/mineral-record-research-v1/` holds manually researched batches,
immutable source HTML, retrieval receipts, a queue containing every existing
public ID, and a readable progress report. This mutable research state is
ignored by Git and is not an importer input. Preserve it with the data-root
backup procedure.

This initial workbench queue measures its own draft batches. Its historical
`not_started` states do not describe later scientific evidence already saved
directly to the mineral records; use the current checkpoint reports for that
progress. Preserve the original workbench batches and their source receipts.

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

These legacy workbench commands reproduce that initial batch only. They do not
reserve current work or replace the shared content dispatcher. Preserve the
original batches and receipts rather than regenerating them to record new
assignments. Complete public source fit, admission, mappings, scientific review
and release checks separately from private research progress.
