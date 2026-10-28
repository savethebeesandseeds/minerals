# Record descriptions from stored facts

Write one catalog description for every mineral from facts already on the record.
Do not search, open papers, or add sources. Do not claim `content-queue` for
this work. Content research and COD extraction stay on their own queues.

The queue is seeded from the live `materials` table. One job is one mineral.
Order is:

1. `missing` — no description text yet. Compose one.
2. `thin` — the text is under 120 characters, or the published description has
   no source URLs. Compose a replacement. The previous text stays in private
   `research_draft_descriptions`.
3. `retain` — a cited description of at least 120 characters already exists.
   Copy that text and its source URLs unchanged.

```powershell
docker compose exec -T --user 0:0 admin bash tools/container-task.sh description-queue next --worker description-session-1
docker compose exec -T --user 0:0 admin bash tools/container-task.sh description-queue complete --worker description-session-1 --token <token> --review-file <path>
docker compose exec -T --user 0:0 admin bash tools/container-task.sh description-queue status
```

`next` reserves one mineral and returns a flat packet: identity, IMA country and
references, up to 24 reviewed claim values, a COD count, and the allowed URL
set. Calling `next` again with the same worker resumes that claim. The review
file path in the packet is where the worker writes:

```json
{
  "material_public_id": "mat_…",
  "canonical_name": "Name",
  "action": "compose",
  "text": "Name forms sharp blue crystals and is found in the granite of …",
  "source_urls": []
}
```

`action` is `compose` or `retain`, as the packet says. A composed description
is a short header, two or three sentences and 40–700 characters. It names the
mineral and talks about its color, the sharpness or habit of its crystals, and
the places it is found. Hardness and luster appear only when a reviewed claim
gives them. `discovery_country` may be used as the place it was described.
Leave out any of those that the packet does not contain. Do not fill the gap
by listing IMA status, references, the formula, COD counts, or a sentence that
says the catalog lacks a field.

`source_urls` must be chosen from `known.allowed_urls`. A mineral with no
stored URL still gets a description; its list is empty.

`complete` checks the token, writes `materials.description`, and sets
`public_research.description` to that text with `CC-BY-4.0` on the summary
wording. Cited sources keep their own terms. Existing `public_research`
publication status, review status, observations, and COD records stay in
place. The save uses the same private writer lock as content save and a daily
database checkpoint kept under this queue. It does not rebuild the public
catalog snapshot.
