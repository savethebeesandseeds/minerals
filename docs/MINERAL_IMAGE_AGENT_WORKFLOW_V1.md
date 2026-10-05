# Agents: one image per mineral record

Use the [common prompt](MINERAL_IMAGE_SINGLE_IMAGE_V1.md), with the same black
background and lighting for every record. Use the image tool directly.
Use the shared dispatcher for assignments; do not keep a separate progress list.

## Dispatch

Give each worker a stable, unique session ID. Every worker uses these commands
from `C:/Work/Minerals`, through the existing admin container:

```powershell
docker compose exec -T --user 0:0 admin bash tools/container-task.sh image-queue next --worker images-session-1
docker compose exec -T --user 0:0 admin bash tools/container-task.sh image-queue complete --worker images-session-1 --token <claim_token>
docker compose exec -T --user 0:0 admin bash tools/container-task.sh image-queue status
```

`next` atomically reserves one pending record and returns its ID, name, formula,
record context, common prompt, output path and claim token. Replace only the
appearance slot using supported facts or a brief mineralogical reference.
Generate only when `generation_allowed` is true. Save the actual final image,
then call `complete` with the returned token before requesting another record.
Use the native extension if the returned image is JPEG or WebP.

Start with two workers; use up to three subagents when the current four-slot
environment has capacity. Reuse idle workers with `followup_task`. Read the
common instructions once per worker session. Pass this task to each worker:

```text
Read C:/Work/Minerals/docs/MINERAL_IMAGE_AGENT_WORKFLOW_V1.md and the common
image instructions. Worker ID: <unique-session-id>.
Ask the dispatcher for one record. If generation_allowed is false, recover
the existing assignment without starting another call. Otherwise fill the
appearance slot, generate with the built-in image tool and opaque black
background, inspect briefly, copy the original file to the returned save path,
and complete the claim with its token. Report the saved path or failure state.
Do not create sidecars, scripts, progress files or subagents.
```

## Attempts and continuation

- One initial call per record. Allow at most one targeted correction for an
  obvious problem; preserve the mineral appearance and black studio setup.
- Await the original pending call. A timeout or silent worker does not authorize
  another generation of the same record.
- Save before reporting completion. Use the actual returned path and native
  format. Never guess a path or overwrite another record's image.
- The dispatcher reconciles existing valid record images, including a saved
  result whose session stopped before calling `complete`. It skips completed
  records and retains active claims across restarts. Unknown legacy filenames
  are ignored. File validation does not replace visual inspection.
- Reservations last 30 minutes for reporting purposes. Expiry flags a stale job;
  it never automatically reassigns it. A long-running call can renew its claim
  with `heartbeat --worker <id> --token <token>`.
- Report a call whose outcome is unknown with `fail --worker <id> --token <token>
  --reason "..."`; the default `uncertain` state keeps the reservation. Use
  `--kind failed` only after a confirmed failure, or `--kind needs_research` for
  unsupported appearance. Those records are skipped until explicitly requeued.
- After confirming that no image call remains active or unresolved, use
  `requeue --public-id <id> --reason "..." --confirmed-idle`. Old tokens become
  invalid; claim history is retained. Existing files are never overwritten or
  deleted by the dispatcher. Review partial or conflicting files before retrying.
- Parallel workers share usage limits. Pause dispatch on a quota or rate limit;
  do not change the tool, model or billing route, or add workers to evade a limit.

Use `spawn_agent`, `followup_task`, `send_message`, and `wait_agent`; do not create
separate user chats. Keep one final image per public record ID. Register and
visibly label AI illustrations when integrating them into the catalog.

All shared state lives in the private `data/images/.image-queue.sqlite3` file.
Run `docker compose exec -T --user 0:0 admin bash tools/container-task.sh image-queue-test`
for the isolated assignment and recovery tests.
