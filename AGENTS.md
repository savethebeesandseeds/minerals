# Minerals project instructions

## Record descriptions from stored facts

For the description phase, read `docs/MINERAL_DESCRIPTION_QUEUE_V1.md` and claim
one mineral at a time:

```powershell
docker compose exec -T --user 0:0 admin bash tools/container-task.sh description-queue next --worker <stable-unique-session-id>
```

- Work only on the claimed mineral and token. Reuse the same worker ID to resume.
- Write a short specimen header from the packet: color, crystal sharpness or habit, and where it is found. Do not search, open papers, add sources, or inventory the stored fields.
- `missing` and `thin` jobs are composed. `retain` copies the existing cited text.
- Complete with `description-queue complete` and the review file from the packet.
- Do not use `content-queue` or `content-save` for this work, and do not write
  `minerals.db` directly. Use `description-queue status` for progress.

## Cached COD metadata extraction assignments

For the COD metadata extraction phase, read
`docs/COD_METADATA_EXTRACTION_WORKFLOW.md` and use the prepared shared queue:

```powershell
docker compose exec -T --user 0:0 admin bash tools/container-task.sh cod-extraction-queue next --worker <stable-unique-session-id> --compact
```

- Work only on the claimed mineral and token; reuse the same worker ID to resume.
- This extraction phase uses five independent sessions at medium reasoning
  effort, each with its own stable worker ID. Keep routine tool output concise.
- Inspect each cached source revision and its mapped candidate, including prior
  models, source locators, uncertainties, conditions and origin/status flags.
- Complete through this queue with the source-check review and
  `--confirm-reviewed`, then claim the next item. Each completed artifact is
  durably staged privately; no additional batch save is needed.
- Extraction checks do not approve species identity, publish data, or complete
  the full content-research queue below. Do not use `content-save` for these
  artifacts or write `minerals.db`. Keep deferred sources and gaps explicit.
- Do not scrape COD, repeat whole-paper research or create per-mineral parsers.
  Use `cod-extraction-queue status` for progress. Stale claims stay reserved
  until explicitly reconciled with the previous worker idle.

## Shared mineral content assignments

For mineral-detail research, read `docs/MINERAL_RECORD_RESEARCH_WORKFLOW.md`
and claim work through the shared content queue in the existing admin container:

```powershell
docker compose exec -T --user 0:0 admin bash tools/container-task.sh content-queue next --worker <stable-unique-session-id> --compact --priority-group broader_sparse
```

- Every concurrent session/reviewer uses its own stable ID and works only on
  its returned mineral. Calling `next` again resumes an active assignment.
- Use the returned live record and prior evidence. Do not pick from historical
  draft queues or dispatch lists, and do not maintain a competing work list.
- Submit the cited review with its claim token. Submission keeps the mineral
  reserved while allowing that worker to request another record.
- Keep sessions moving after submission. Use the compact packet first and
  omit `--compact` with the same worker ID to expand relevant prior evidence.
- Follow the workflow's first-pass time guidance: preserve blocked source/OCR
  leads, reuse genuine captures and readable source passages, and avoid
  per-mineral formatter scripts or repeated whole-paper research.
- The user authorizes four independent research sessions plus the original
  session. Each verifies its own source-supported findings and saves submitted
  batches with `container-task.sh content-save --worker <id> --confirm-reviewed`.
  The shared save routine serializes database writers, reuses a daily database
  checkpoint (retaining only the newest two managed checkpoints),
  verifies changed records and completes saved queue items. Full database audits
  run with new daily checkpoints, not each batch. Do not write
  minerals.db directly or make another save script. Routine session-to-session
  messages or a coordinator handoff are unnecessary; use queue state.
- Stale or uncertain work stays reserved until explicitly reconciled. Preserve
  submitted reviews, unresolved candidates and source qualifications. Completion
  means a useful first review with explicit gaps, not exhaustive knowledge.
- Use `content-queue status` for shared progress. Image assignments continue to
  use their separate `image-queue`.

## One canonical annotatable development page

During active visual development, `http://127.0.0.1:18965/` is the only public
web entry and the real application must be directly selectable by Codex's
annotation control.

- Do not create a separate review HTML document, selector-session URL, helper
  server, or alternate rendering path.
- Keep the canonical `index.html` meta policy and local Nginx response aligned
  with the narrow `style-src-elem 'self' 'unsafe-inline'` exception required by
  the annotation overlay.
- Keep inline scripts blocked and retain `style-src-attr 'none'`.
- Leave `deploy/nginx/minerals-static.conf` strict. Harden the canonical HTML
  policy only when the user says visual development is complete and the final
  production-security pass begins.
