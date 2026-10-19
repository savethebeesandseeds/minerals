# Minerals project instructions

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
