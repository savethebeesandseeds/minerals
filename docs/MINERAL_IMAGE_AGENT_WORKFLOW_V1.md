# Mineral image generation: parallel agents and continuation v1

Status: documented manual protocol. This file does not create a runner, start
generation, or authorize an unlimited catalog run. A generation run needs an
explicit scope: mineral public IDs or a bounded selection, style version,
maximum total image calls, and destination. The user has requested an agent
workflow; a coordinator may delegate that authorized work without creating
separate Codex chats.

The single-image contract is
[MINERAL_IMAGE_SINGLE_IMAGE_V1.md](MINERAL_IMAGE_SINGLE_IMAGE_V1.md).
Its `minerals-hero-v1` prompt block is the sole source of visual instructions.
The coordinator extracts the fenced text between
`MINERALS_IMAGE_PROMPT_V1_BEGIN` and `MINERALS_IMAGE_PROMPT_V1_END`, substitutes
only `{{mineral_name}}`, `{{appearance_brief}}`, `{{depiction_mode}}`, and
`{{species_constraints}}`, and writes the exact rendered prompt as UTF-8 with
LF line endings and one trailing newline. Hash those bytes. Unfilled slots
block dispatch. Workers pass that rendered prompt unchanged. A substantive
change to the depicted appearance or style requires a new job identity;
it inherits the original image's budget and history. A targeted correction to
the rendering stays in the existing job.

## Scope, speed, and resource limits

- Use the built-in image generation tool, one call per requested mineral image.
  Each edit is another call. Do not silently switch to an API, CLI, model, or
  billing route. The installed
  [imagegen skill](C:/Users/santi/.codex/skills/.system/imagegen/SKILL.md)
  defines the supported default and fallback paths.
- Use actual available agent slots. The present environment has four total
  slots including the coordinator, so at most three image workers can run at
  once. Start with two workers; increase to three only while the provider is
  accepting calls and the review queue remains manageable. Nested workers and
  unrelated live agents also consume slots. Do not assume a later environment
  has the same capacity.
- Parallel work overlaps waiting and review. It does not guarantee faster
  generation, separate quotas, or additional allowance. Do not spawn agents
  to evade a usage or provider limit. Built-in image generation uses general
  Codex usage limits; see the official
  [image generation guidance](https://learn.chatgpt.com/docs/image-generation).
- Prepare appearance briefs before dispatch. A worker receives one ready job
  at a time. Reuse idle workers with `followup_task` rather than repeatedly
  creating agents.
- Set `generation_call_limit: 3` for each job across its entire history, including
  edits and confirmed failed calls. Also enforce the authorized total run-call
  budget. Assign a persistent `generation_budget_id` to the planned mineral
  image; replacement jobs for a revised brief or style keep that ID, cumulative
  calls, and unresolved attempts. Changing a hash or job ID cannot renew the
  three-call allowance. An unresolved invocation reserves one call and blocks
  that image until reconciled. Continuation never resets either limit. Counters
  such as
  `transient_retries_used` and `quality_revisions_used` are diagnostic breakdowns,
  not extra call budgets.
- Images remain private draft illustrations. This workflow does not register
  media in the application, publish assets, alter catalog records, or change
  Docker container definitions.

## Calibrate before a large run

Begin with a bounded pilot of three to five source-supported minerals that
exercise different forms and light responses, such as a pale transparent
crystal, a dark metallic aggregate, and a matte fragment or powder. Use the
same standard and ordinary job/call limits. Compare the saved images together
for background, light direction, exposure, and framing before expanding the
queue. Pin one accepted image as an optional `style_only` reference for the
next run, with its hash and role recorded. If calibration changes the visual
standard, create a new spec version before bulk work. Do not scale an
uninspected style to the entire catalog.

## Inputs and scientific gate

The coordinator prepares an immutable, source-checked private appearance brief
for each job, using the single-image contract. This appearance check is distinct
from approval to publish scientific content. Include the exact existing public
ID, canonical name, and frozen population release ID;
source URLs and locators for the depicted habit, color, luster, opacity, and
surface texture; the chosen supported presentation; unresolved conflicts;
and reference-image rights, if references will be supplied to the tool.
Pin any common `style_only` reference by hash at run level. Per-mineral specimen
references are `appearance_only`; neither role substitutes for the other.

Do not use a name or chemical formula alone as an appearance brief. If the
available evidence does not support the depiction, mark `needs_research` and
skip generation. A rare mineral known only as microscopic grains must not
become an invented large display crystal. An AI illustration is never specimen
evidence, a locality photograph, or identification ground truth.

Reference photographs need explicit reuse rights and attribution records;
scientific text citations alone do not grant image rights. Do not hot-link
unreviewed images or give a worker a reference whose rights are unknown. Follow
the project rules in [VISION.md](VISION.md),
[INGESTION.md](INGESTION.md), and
[MINERAL_RECORD_ENRICHMENT_BACKLOG.md](MINERAL_RECORD_ENRICHMENT_BACKLOG.md).

## Private durable state

Keep working state under:

```text
C:/Work/Minerals/data/pilots/mineral-image-generation-v1/
  run.json
  queue.json
  events.jsonl
  protocol/
    MINERAL_IMAGE_SINGLE_IMAGE_V1.md
    MINERAL_IMAGE_AGENT_WORKFLOW_V1.md
  checkpoints/
  jobs/<job-id>/
    input/
      appearance-brief.json
      style-block.txt
      references.json
    attempts/001/
      assignment.json
      prompt.txt
      invocation.json
      tool-receipt.json
      result.json
      qa.json
      artifacts/<returned-image-filename>
    attempts/002/
    attempts/003/
    review.json
```

`data/pilots/` is ignored private state. These files are examples of the
required layout, not files already created by this document. Retain all
attempt outputs, including rejected candidates, for this pilot. Never overwrite
an earlier prompt, receipt, image, or review to make a later attempt look like
the first one.

Use a path-safe 64-character SHA-256 job ID derived from the pinned generation
inputs. Hash a compact UTF-8 JSON object with fixed key order:
`population_release_id`, `material_public_id`, `style_spec_id`,
`style_spec_sha256`, `appearance_brief_sha256`, and `references_sha256`.
Save those exact identity bytes and the unmodified values in metadata.
For example:

```text
job_id = <full-sha256-of-identity-json>
```

Never use the display name alone as identity, or truncate a hash without a
collision check. Check destination path lengths before dispatch. Save
immutable copies of both instruction documents under `protocol/`
when a run begins, and pin their SHA-256 hashes along with the extracted
style block, brief, prompt bytes, reference inputs, and every saved output.
Workers read those pinned copies. Later repository edits do not change an
active run's instructions without an explicit version migration.

The coordinator is the only writer of `run.json`, `queue.json`, `events.jsonl`,
central checkpoints, and `review.json`. One active coordinator owns a run.
Workers write only their assigned attempt directory. The coordinator writes its
`assignment.json` and rendered `prompt.txt` before dispatch; workers verify and
preserve those files unchanged. They report results to the coordinator and
never claim the next queue row themselves.

The coordinator must write a checkpoint before dispatch, reserve a call before
granting an invocation, and record each transition before the next dependent
action. Replace mutable checkpoint files atomically using a sibling temporary
file and a same-filesystem rename. Keep the append-only event history as well.
This is an operational discipline until a validated queue implementation exists;
do not describe the prose pseudocode below as an installed runner.

If a helper for hashes, pixel metadata, or queue validation is later implemented,
develop and run it in the project's existing managed Linux development
container. Small host file-inspection and copy operations are permitted; do not
install a host toolchain or create replacement containers for this workflow.

## Required metadata

Keep these fields in the durable run and job records:

| Record | Required information |
| --- | --- |
| Run | Run ID, user-authorized scope, UTC start time, total call budget, worker limit, protocol/document hashes, stop flag, global resource state, run status. |
| Job | Job ID, predecessor job if any, persistent `generation_budget_id`, exact public ID/name, style ID/hash, appearance brief hash, status, current assignment token, worker ID, cumulative reserved/started calls for that budget ID, attempt directories, chosen candidate, next action. |
| Assignment | Unique token, job ID, attempt number, allowed operation (`generate` or `edit`), expiry/check-in time, run/individual call reservation, immutable input paths/hashes, allowed output directory. |
| Invocation | Attempt number, operation, exact requested prompt/hash, revised prompt if exposed (otherwise null), actual tool arguments, UTC invocation preparation/start time, pending exec cell ID and owning agent/session if returned, outcome state, error category, completion time if known. |
| Tool receipt | Tool/provider name, requested model and returned model/version when exposed, returned local paths or output identifiers, request ID if available, error details, any Retry-After, saved artifact paths/hashes/byte counts/dimensions. |
| Provenance | `media_kind: synthetic`, `purpose: illustrative`, `scientific_evidence: false`, `public_registration_allowed: false`, reference roles/URLs/hashes/licenses/attribution, generation time, input/output hashes, caption/alt text, rights/review state. |
| Review | Worker QA findings, independent coordinator inspection, acceptance/rejection reason, candidate hash, reviewer identity and UTC review time, publication state. |

For the built-in tool, record `requested_model_selector: tool_default` and
`requested_model: null` when no model selector was offered. Record unavailable
returned model, version, seed, parameters,
latency, request ID, or license as `null` plus a short reason such as
`not_exposed_by_tool`; do not invent them. A requested square 1024 × 1024 image
is a prompt target, not proof of returned dimensions. Capture actual dimensions.
Do not assign a public license automatically. Missing metadata or rights review
must remain visible to the later media release gate.

## States and assignment safety

Use explicit job states:

| State | Meaning and next step |
| --- | --- |
| `needs_research` | No sufficiently supported depiction; research the brief before any image call. |
| `ready` | Inputs are complete, pinned, and within the authorized scope. |
| `assigned` | Coordinator reserved an attempt and assigned it to one worker. |
| `generating` | Invocation marker exists and tool call is in progress. |
| `pending_unknown` | It is uncertain whether a call completed or produced an output; reconcile, do not retry. |
| `needs_artifact_recovery` | A completed output is known but the workspace copy is missing; recover that output without generating again. |
| `candidate_saved` | Returned output is copied to the assigned private directory and receipt is durable. |
| `review_pending` | Worker QA is complete; coordinator must inspect the actual saved image. |
| `revision_ready` | A specific quality defect is recorded and another call is permitted. |
| `accepted_private` | Coordinator accepted one candidate; publication is still separate. |
| `paused_resource` | A quota, rate, or service restriction blocks further dispatch. |
| `failed_terminal` | Non-retryable failure or exhausted job call limit; retain all history. |
| `cancelled` | Coordinator or user cancelled future work; existing artifacts and uncertain outcomes remain recorded. |

The assignment token fences queue ownership. A stale worker may finish saving
its assigned artifact, but cannot authorize a new image call or change central
state. A late result is reconciled by the coordinator using the token, attempt
number, and hashes.

Expiry of a lease or silence from a worker is not evidence that generation
stopped. Never reassign generation based only on expiry. Inspect live agent
status, the attempt directory, and pending tool state. A replacement worker may
reconcile an old attempt but may not duplicate an unresolved invocation.

## Worker prompt template

Send this as the `spawn_agent` message for the first assignment or as a
`followup_task` message to an idle existing worker. Fill all placeholders before
dispatch. The input filenames and hashes must agree with `assignment.json`.

```text
Generate exactly one private mineral illustration for the assigned job.

Read and follow:
C:/Work/Minerals/data/pilots/mineral-image-generation-v1/protocol/MINERAL_IMAGE_SINGLE_IMAGE_V1.md
C:/Work/Minerals/data/pilots/mineral-image-generation-v1/protocol/MINERAL_IMAGE_AGENT_WORKFLOW_V1.md
Expected instruction-document hashes: <exact hashes>

Job ID: <job-id>
Mineral public ID: <exact-existing-public-id>
Canonical name: <canonical-name>
Assignment token: <unique-token>
Generation budget ID: <persistent-budget-id>
Attempt number: <001|002|003>
Allowed operation: <generate|edit>
Calls already started or unresolved for this job: <count>
Call reservation for this assignment: <reservation-id>
Authorized maximum calls for this job: 3, with no reset on continuation.

Read only these pinned inputs:
C:/Work/Minerals/data/pilots/mineral-image-generation-v1/jobs/<job-id>/input/appearance-brief.json
C:/Work/Minerals/data/pilots/mineral-image-generation-v1/jobs/<job-id>/input/style-block.txt
C:/Work/Minerals/data/pilots/mineral-image-generation-v1/jobs/<job-id>/input/references.json
Expected input hashes: <exact hashes>
Exact coordinator-rendered prompt path/hash:
C:/Work/Minerals/data/pilots/mineral-image-generation-v1/jobs/<job-id>/attempts/<attempt>/prompt.txt
<exact prompt hash>
For an edit, existing target path/hash: <path/hash or none>
Specific approved revision: <one defect/change or none>

Write only under:
C:/Work/Minerals/data/pilots/mineral-image-generation-v1/jobs/<job-id>/attempts/<attempt>/

Verify the prompt bytes/hash and pass that exact rendered text unchanged.
Do not fill slots, rewrite prompts, add style prose, or select model overrides.
Use the built-in image generation tool, with transparent_background=false.
Do not change the mineral, style, evidence, rights, or central queue. Do not
spawn agents, publish assets, or invoke a CLI/API fallback.

Before calling the image tool, verify the saved prompt and assignment, then
save input hashes, operation, and invocation marker. Make at most ONE image-tool
call for this assignment. Await that exact pending call to completion. A
timeout, silence, or unknown result is not permission to generate again.

Copy the returned image's original bytes into your assigned artifacts directory
without overwriting another attempt, inspect that actual saved image, and record
output hashes, paths, dimensions, tool metadata, and QA. Save the receipt and
worker result BEFORE reporting completion. Preserve unavailable values as
null with their reason. If unable to save a known output, report
needs_artifact_recovery and its specific blocker; if the invocation outcome is
uncertain, report pending_unknown. Do not regenerate to solve a save failure.

Return job ID, token, attempt, operation/outcome, saved candidate path/hash,
QA findings, counters, and next action. An accepted-looking image still needs
independent coordinator inspection and remains an AI-generated illustration.
```

Do not give a worker unrestricted permission to retry. The coordinator decides
each later attempt, records its reason, and issues a new assignment token. For
a quality correction, the coordinator renders and hashes a new complete prompt
using the same fixed block, appending the targeted rendering correction to
`species_constraints` and leaving all supported appearance assertions unchanged.
Save that correction separately in the assignment; do not rewrite the pinned
appearance brief. A new habit, color, variety, or other substantive appearance
requires a revised brief and replacement job under the same budget ID. The
worker does not improvise an edit instruction. Pin the exact existing target
image/hash for an edit.

Number attempts cumulatively for the persistent budget ID, even when a revised
brief creates a replacement job directory. For example, a replacement after
attempt 001 begins at 002, with only two calls left. Preserve links to earlier
attempt directories; copying their history must not add calls a second time.

## Tool invocation and output handling

Workers use the actual exposed `image_gen` tool through `functions.exec`.
The image tool can take minutes. Its current tool instructions prescribe the
following initial yield and the same yield for subsequent exec waits:

```javascript
// @exec: {"yield_time_ms": 120000, "max_output_tokens": 1000}
const result = await tools.image_gen__imagegen({
  prompt: "<exact text read from the coordinator-rendered prompt.txt>",
  transparent_background: false
});
store("<job-id>:<attempt>:completed-image-result", result);
generatedImage(result);
```

Persist the invocation marker before executing this cell. Use the current
tool documentation if its interface changes; do not invent arguments for
seed, model, concurrency, quality, size, billing, or destination. For a new
image with no input references, omit both `referenced_image_paths` and
`num_last_images_to_include`. For local references or edits, inspect each
image with `view_image` first and pass the exact local paths through
`referenced_image_paths`. Never combine the two reference mechanisms or rely
on another agent's recent-image context. Record each input's role.

Only call `functions.wait` when `functions.exec` returns `Script running with
cell ID ...`. Persist that ID and its owning agent/session, then wait on that
exact cell from the original session; do not invoke
another image tool call while it is pending. A yielded call is the same attempt,
not a failure or a new attempt. A resumed conversation cannot assume an old
cell ID is still valid or transferable to another agent: reconcile its status
and any returned output first. Ask the original live worker to await its call.

Do not print or store base64 data in ordinary logs. Inspect returned metadata
without dumping the full result. Built-in images may initially reside under
the actual returned `$CODEX_HOME/generated_images/...` path. Use that returned
path, not a guessed filename. Copy project-bound output to the private attempt
directory before reporting it, and verify that the bytes/hash match the
source. Do not treat the image tool as accepting an arbitrary output path.

The coordinator can continue handling results and give progress updates while
workers await their image calls. Its own agent-status waits should be bounded
so ongoing work receives a meaningful update at least every 60 seconds.
Use tool-specific image wait guidance without launching duplicate calls to
make a wait appear faster.

## QA, revisions, and retries

Worker QA checks the actual image against the pinned brief and the single-image
contract: identity and supported morphology; plausible texture, opacity,
color, and luster; square target and actual pixel dimensions; centered
three-quarter framing; neutral seamless background; uniform key/fill lighting
and soft contact shadow; sharp subject; no text, props, logos, decorative
sparkles, clipping, or extra specimen. The caption and alt text must identify
the output as an AI-generated illustration and avoid an invented locality,
specimen size, or photographed provenance.

The coordinator opens every candidate selected for acceptance with
`view_image`, independently verifies the same criteria, and records the image
hash reviewed. A worker's favorable text report is not visual inspection.
For a batch, also compare candidates for background, apparent exposure,
framing, and scale consistency. A visually convincing depiction does not
replace source-backed scientific review.

Classify any next action before reserving another call:

| Outcome | Action |
| --- | --- |
| Acceptable candidate | Mark `accepted_private`; record caption, provenance, and selected hash. No extra call. |
| Specific quality defect | Record the defect and choose a targeted edit or regeneration. This consumes one of the remaining three calls. Preserve the fixed style and supported mineral traits. |
| Confirmed transient tool failure | Record it as a failed call. If limits permit, retry only after the coordinator authorizes the next attempt and backoff. |
| Unknown completion/output | Mark `pending_unknown`, keep the call reserved, and reconcile. No blind retry. |
| Completed output cannot be copied | Mark `needs_artifact_recovery` and recover the original returned artifact. A file-save failure does not justify another image call. |
| Missing/unsupported morphology | Mark `needs_research`; do not request a prettier invented replacement. |
| Rights, safety, or unsupported-input rejection | Record the reason and stop that job for review. Do not evade the rejection by changing agents. |
| Quota/rate/service restriction | Pause new dispatch globally, preserve active jobs, and wait for the specified reset or recovery. More agents do not solve it. |
| Third call exhausted without an acceptable candidate | Mark `failed_terminal` with review findings and remaining research need. Continuation cannot create attempt four. |

For a confirmed transient failure, use bounded backoff of 30 seconds after the
first failed call and 60 seconds after the second, plus recorded jitter of
0–15 seconds. If the provider supplies a later `Retry-After`, honor it; never
cap it downward and call early. If the wait is long, persist `not_before_utc`
and leave the job paused rather than blocking in a long sleep. Keep short
agent waits at most 60 seconds. Actual provider quota/reset information takes
precedence over this transient-failure schedule.

Transport failure and quality revision are different reasons for an attempt;
neither grants additional calls. Count confirmed failures, edits, and original
generations together. Treat an invocation that might have been sent as
consumed until reconciliation proves it was never made. Preserve corrections
to counters as append-only events with evidence; do not silently decrement or
restart them.

## Coordinator pseudocode

This is a manual control loop using the exposed collaboration tools, not
executable repository code. Do not put collaboration calls inside
`functions.exec`.

```text
read run scope, ledger, pinned documents, and all existing job/attempt records
verify one coordinator owns this run; validate budgets and input hashes
call list_agents to identify active workers and actual occupied slots
reconcile every assigned/generating/pending_unknown or missing-artifact attempt
before dispatch
persist a checkpoint with queue state, budgets, workers, and next actions

while authorized work remains and stop flag is false:
    inspect saved worker results and reconcile incoming agent messages
    for each completed candidate:
        verify artifact exists, receipt/token/hashes agree, and counters agree
        inspect the actual saved image independently
        write acceptance, specific revision, research need, or terminal failure

    if a quota/rate restriction is reported:
        persist global paused_resource and not_before_utc/reset information
        stop new dispatch; tell active workers not to start another call

    if the authorized total-call budget is exhausted:
        persist run status budget_exhausted; drain and review active work
        leave unfinished jobs recorded; do not reset or enlarge the budget
        exit the dispatch loop with a precise handoff

    for each free worker slot, only when global dispatch is allowed:
        require no resource pause and available worker/global/image call budgets
        select one ready or revision_ready job with no unresolved invocation
        reserve one attempt/call and persist assignment + central checkpoint
        use spawn_agent once for a new worker, or followup_task for an idle one
        pass the fully filled worker prompt and exact assignment token

    use send_message only for coordination or a stop request to live workers
    use list_agents, durable results, and incoming updates to track progress
    use wait_agent in bounded intervals when there is no independent work
    checkpoint before yielding and provide a meaningful progress update

on stop, resource pause, or scope completion:
    stop dispatch; drain active calls where possible; reconcile returned files
    preserve unresolved tool/worker state and all counters
    write a final checkpoint and precise handoff/report
```

`wait_agent` announces mailbox updates; read the delivered messages and inspect
their saved results. Do not infer successful generation solely from an agent
becoming idle or exiting. Use `interrupt_agent` only for a necessary interruption;
an interrupted agent may still have an uncertain image invocation, which must
be retained and reconciled.

Do not use `create_thread` or `send_message_to_thread` for worker execution.
Those create or message user-visible separate chats and require the user's
explicit request for that action. Subagents and `followup_task` are the tools
for the current authorized parallel image task.

## Stop, continuation, and recovery

A graceful stop sets the coordinator stop flag, prevents new assignments, and
asks each worker to finish saving its active result without starting another
call. Drain active work when possible. If immediate interruption is necessary,
preserve every pending cell ID, worker ID, token, attempt, and invocation marker.
Do not interpret interruption as cancellation of a remote image operation.

Before continuing, read the last checkpoint and append-only events, inspect the
actual attempt directories, and list live agents. Resolve each pending attempt:

1. If its original exec cell is still valid in the original worker/session,
   have that worker await the same cell. Do not assume another agent can use it.
2. If a result was returned, locate and copy that output, write its receipt, and
   resume QA without another generation call.
3. If the worker is alive, send a status/reconciliation message; do not assign
   the same job to a second generator. Reuse it with `followup_task` when idle.
4. If an agent is unavailable but its artifact and receipt exist, adopt them
   only after token and hash verification, then review the saved image.
5. If a failure is confirmed, record its consumed call and follow the bounded
   retry/revision decision. If outcome remains unknown, leave the job blocked
   as `pending_unknown`; advance other independent jobs only within scope.

Never repeat all prompts on a fresh conversation to "start clean." Never infer
completion from a filename alone, delete failed attempts to recover a budget,
or promote a result automatically during recovery. Changing the brief/style
creates a new versioned job only through coordinator review, retaining the
same `generation_budget_id`, cumulative image-call allowance, and remaining
authorized run budget; it is not an attempt-limit workaround.

Use this continuation instruction with the current chat or coordinator agent:

```text
Continue the existing mineral image run under
C:/Work/Minerals/data/pilots/mineral-image-generation-v1/.
Follow the hashed copies of both image instruction documents under protocol/
recorded in the run. Read the latest checkpoint,
queue, event log, and attempt receipts before doing work. Preserve the
authorized scope, remaining total-call budget, and all per-job attempt counts.
List live agents and reconcile active/unknown calls before dispatch. Reuse
existing workers when possible. Do not regenerate saved outputs or unresolved
invocations, reset counters, publish assets, or switch billing/tool paths.
Resume only eligible ready/revision jobs within the remaining limits, then
report verified progress and the next action for every unresolved job.
```

## Completion report and handoff snapshot

Report counts separately: minerals in scope; briefs ready; `needs_research`;
assigned/in progress; unknown invocations; calls started or conservatively
consumed; confirmed failed calls; quality revisions; candidates saved; images
awaiting review; images accepted privately; terminal failures; and images
published. Publication remains zero for this workflow. Do not present multiple
candidates or multiple attempts as additional completed mineral records.
Count calls from distinct invocation/reservation records, not by adding the
cumulative counters of predecessor and replacement jobs. Keep one budget
ledger entry per `generation_budget_id`.

The checkpoint/handoff must contain the run ID and paths, document/input hashes,
scope, remaining total-call budget, next queue position, current worker IDs and
tokens, pending exec cell IDs, every job's cumulative call count, last confirmed
result path/hash, global not-before/reset state, and a concrete next action for
each unfinished job. Link accepted private images and unresolved review notes.
Record the UTC timestamp of the verified status.

The run is complete when every in-scope job has a documented terminal outcome
and no active or unknown invocation or unreviewed artifact-recovery task remains.
Completion can include explicit research gaps and failed images;
it does not mean every mineral has a valid
illustration. Publishing requires the separate reviewed media provenance,
labeling, rights, and catalog release workflow.
