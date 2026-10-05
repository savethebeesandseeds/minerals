#!/usr/bin/env python3
"""Prepare a private, catalog-wide research queue from manually researched batches.

This is a research workbench, not a scientific importer or publication adapter.
Run inside the existing admin container; uses only the Python standard library.
"""

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import time
import urllib.parse
import urllib.request


ROOT = Path("/workspace")
WORK = ROOT / "data/pilots/mineral-record-research-v1"


def encode(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode()


def digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def artifact(path):
    data = path.read_bytes()
    return {"path": str(path.relative_to(ROOT)), "sha256": digest(data), "bytes": len(data)}


def save_object(data):
    value = digest(data)
    path = WORK / "objects/sha256" / value[7:]
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        require(path.read_bytes() == data, "content-addressed object mismatch")
    else:
        with path.open("xb") as stream:
            stream.write(data)
    return {"path": str(path.relative_to(ROOT)), "sha256": value, "bytes": len(data)}


def replace_generated(path, data):
    save_object(data)
    if path.exists() and path.read_bytes() == data:
        return
    temporary = path.with_suffix(path.suffix + ".tmp")
    with temporary.open("wb") as stream:
        stream.write(data)
    os.replace(temporary, path)


def load_batches():
    paths = sorted((WORK / "batches").glob("batch-*.json"))
    require(paths, "no manually researched batches found")
    batches = [(path, json.loads(path.read_bytes())) for path in paths]
    batch_ids = [value["batch_id"] for _, value in batches]
    require(len(batch_ids) == len(set(batch_ids)), "duplicate batch ID")
    for _, batch in batches:
        require(batch["format"] == "waajacu-private-mineral-research-batch-v1", "wrong batch format")
        require(batch["review_state"] == "draft" and batch["public_ingestion_allowed"] is False,
                "research batches must remain private drafts")
    return batches


def read_receipt(source_id):
    require(source_id.replace("-", "").isalnum(), "invalid source ID")
    path = WORK / "sources" / (source_id + ".json")
    receipt = json.loads(path.read_bytes())
    require(receipt["source_id"] == source_id, "source receipt identity mismatch")
    final_url = urllib.parse.urlparse(receipt["final_url"])
    require(final_url.scheme == "https" and final_url.hostname in {"www.nps.gov", "home.nps.gov"},
            "source receipt URL is outside the reviewed NPS hosts")
    obj = receipt["body"]
    body_path = ROOT / obj["path"]
    require(body_path.parent == WORK / "objects/sha256", "source object outside workbench")
    require(artifact(body_path) == obj, "source evidence hash mismatch")
    require(receipt["http_status"] == 200, "source capture was not successful")
    return path, receipt


class NpsRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        parsed = urllib.parse.urlparse(newurl)
        require(parsed.scheme == "https" and parsed.hostname in {"www.nps.gov", "home.nps.gov"},
                "source redirected outside the reviewed NPS hosts")
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def capture(batches):
    sources = {}
    for _, batch in batches:
        for source in batch["sources"]:
            prior = sources.setdefault(source["source_id"], source)
            require(prior == source, "conflicting source definitions")
    (WORK / "sources").mkdir(parents=True, exist_ok=True)
    opener = urllib.request.build_opener(NpsRedirects())
    previous_start = None
    for source_id, source in sources.items():
        receipt_path = WORK / "sources" / (source_id + ".json")
        if receipt_path.exists():
            _, existing = read_receipt(source_id)
            require(existing["requested_url"] == source["url"], "source URL changed")
            continue
        parsed = urllib.parse.urlparse(source["url"])
        require(parsed.scheme == "https" and parsed.hostname == "www.nps.gov",
                "capture is bounded to the manually reviewed NPS pages")
        if previous_start is not None:
            time.sleep(max(0, 12 - (time.monotonic() - previous_start)))
        previous_start = time.monotonic()
        request = urllib.request.Request(source["url"], headers={
            "User-Agent": "Waajacu-Mineral-Record-Research/1 (+https://waajacu.org/)",
            "Accept": "text/html", "Accept-Encoding": "identity",
        })
        with opener.open(request, timeout=60) as response:
            require(response.status == 200, "unsuccessful source capture")
            require(response.headers.get_content_type() == "text/html", "expected HTML evidence")
            body = response.read(2 * 1024 * 1024 + 1)
            require(len(body) <= 2 * 1024 * 1024, "source capture exceeded two MiB")
            receipt = {
                "source_id": source_id, "requested_url": source["url"],
                "final_url": response.url, "http_status": response.status,
                "retrieved_at": datetime.now(timezone.utc).isoformat(),
                "content_type": response.headers.get("Content-Type"), "body": save_object(body),
                "capture_scope": "HTML evidence only; no linked images, assets, or crawling",
            }
        with receipt_path.open("xb") as stream:
            stream.write(encode(receipt))
        print("Captured", source_id, receipt["body"]["sha256"], flush=True)


def prepare(batches):
    manifest_path = ROOT / "public-catalog/catalog-manifest.json"
    manifest = json.loads(manifest_path.read_bytes())
    require(manifest["mineral_count"] == 6226, "fixed population changed")
    database = ROOT / "public-catalog" / manifest["database"]["path"]
    require(digest(database.read_bytes()) == manifest["database"]["sha256"], "catalog hash mismatch")
    registry_path = ROOT / "schemas/mineral-record-questionnaire-v1.json"
    registry = json.loads(registry_path.read_bytes())
    questions = {key: module_name for module_name, module in registry["modules"].items()
                 for key in module["questions"]}
    profile_keys = sorted(key for key, module in questions.items() if module != "protected_identity")
    with sqlite3.connect(database.as_uri() + "?mode=ro&immutable=1", uri=True) as connection:
        population = {row[0]: {"public_id": row[0], "canonical_name": row[1], "formula": row[2]}
                      for row in connection.execute("SELECT public_id, canonical_name, formula FROM minerals")}
    require(len(population) == 6226, "population IDs are not unique")
    drafted = {}
    source_inputs = {}
    source_definitions = {}
    claim_ids = set()
    for _, batch in batches:
        require(batch["population_release_id"] == manifest["release_id"], "batch release mismatch")
        sources = {source["source_id"]: source for source in batch["sources"]}
        require(len(sources) == len(batch["sources"]), "duplicate source ID in batch")
        for source_id, source in sources.items():
            prior = source_definitions.setdefault(source_id, source)
            require(prior == source, "conflicting source definitions")
            path, receipt = read_receipt(source_id)
            require(receipt["requested_url"] == source["url"], "source URL mismatch")
            source_inputs[source_id] = artifact(path)
        for record in batch["records"]:
            public_id = record["public_id"]
            require(public_id in population and public_id not in drafted, "unknown/duplicate mineral")
            require(record["canonical_name"] == population[public_id]["canonical_name"], "identity mismatch")
            own_claim_ids = set()
            for claim in record["draft_claims"]:
                require(claim["claim_id"] not in claim_ids, "duplicate claim ID")
                claim_ids.add(claim["claim_id"])
                own_claim_ids.add(claim["claim_id"])
                require(claim["question_key"] in profile_keys, "unknown or protected question key")
                require(claim["review_state"] == "draft", "claim incorrectly marked reviewed")
                require(claim["source_id"] in sources, "unknown claim source")
                require(claim["locator"].strip() and claim["statement"].strip(), "empty source locator/statement")
                require(claim["question_key"] in sources[claim["source_id"]]["candidate_question_keys"],
                        "claim outside proposed source scope")
            summary = record["editorial_draft"]
            require(summary["review_state"] == "draft", "editorial draft incorrectly reviewed")
            require(len(set(summary["supporting_claim_ids"])) >= 2, "editorial draft needs two supporting claims")
            require(set(summary["supporting_claim_ids"]) <= own_claim_ids, "editorial support from another mineral")
            drafted[public_id] = {"batch_id": batch["batch_id"], "record": record}
    order = list(drafted) + sorted((key for key in population if key not in drafted),
                                  key=lambda key: (population[key]["canonical_name"].casefold(), key))
    entries = []
    for ordinal, public_id in enumerate(order, 1):
        draft = drafted.get(public_id)
        proposed_keys = sorted({claim["question_key"] for claim in draft["record"]["draft_claims"]}) if draft else []
        entries.append({
            "ordinal": ordinal, **population[public_id],
            "research_status": "draft_prepared" if draft else "not_started",
            "batch_id": draft["batch_id"] if draft else None,
            "proposed_question_keys": proposed_keys,
            "remaining_profile_question_count": len(set(profile_keys) - set(proposed_keys)),
            "publication_ready": False,
        })
    queue = {
        "format": "waajacu-private-mineral-research-queue-v1", "population_release_id": manifest["release_id"],
        "scope": "All existing public mineral identities; no identity or published content changes",
        "state_semantics": "Workflow progress only; these are not questionnaire coverage or applicability states",
        "ordering": "Manually researched batches first, then case-folded name and public ID",
        "profile_question_keys": profile_keys, "counts": dict(Counter(item["research_status"] for item in entries)),
        "total_records": len(entries), "draft_claim_count": len(claim_ids), "published_record_count": 0,
        "entries": entries,
    }
    queue_bytes = encode(queue)
    queue_sha = digest(queue_bytes)
    queue_object = {"path": str((WORK / "objects/sha256" / queue_sha[7:]).relative_to(ROOT)),
                    "sha256": queue_sha, "bytes": len(queue_bytes)}
    research_manifest = {
        "format": "waajacu-private-mineral-research-manifest-v1", "public_ingestion_allowed": False,
        "catalog_manifest": artifact(manifest_path), "catalog_database": artifact(database),
        "preparer": artifact(Path(__file__).resolve()),
        "questionnaire_registry": artifact(registry_path), "batches": [artifact(path) for path, _ in batches],
        "source_receipts": source_inputs, "queue": queue_object,
        "limits": ["Unreviewed research proposals, not normalized scientific claims",
                   "NPS educational text still needs source-class and field-mapping review under SCI1",
                   "Editorial drafts require resolved reviewed claims before public use",
                   "Safety, sensitive localities, and identity conflicts require independent review"],
    }
    lines = ["# Mineral record research progress", "", "Private research drafts; no public records changed.", "",
             f"Population: {len(entries):,}. Drafts prepared: {len(drafted)}. Remaining: {len(entries)-len(drafted):,}.",
             f"Source-linked candidate claims: {len(claim_ids)}. Published: 0.", "",
             "Research status records work performed, not scientific completeness or approval.", "",
             "NPS educational pages are useful starting evidence. SCI1 source fit, field mappings, and independent",
             "scientific corroboration remain pending. Summaries below are draft synthesis, not reviewed claims.", ""]
    for _, batch in batches:
        for record in batch["records"]:
            lines += ["## " + record["canonical_name"], "", record["editorial_draft"]["text"], "",
                      "Candidate facts:", ""]
            for claim in record["draft_claims"]:
                source = source_definitions[claim["source_id"]]
                lines.append(f"- `{claim['question_key']}`: {claim['statement']} [{source['title']}]({source['url']}) — {claim['locator']}.")
            lines += ["", "Next research:", ""] + ["- " + note for note in record["next_research"]] + [""]
    return {
        WORK / "research-queue.json": queue_bytes,
        WORK / "research-manifest.json": encode(research_manifest),
        WORK / "research-progress.md": ("\n".join(lines) + "\n").encode(),
    }, queue


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("capture", "prepare", "verify"))
    arguments = parser.parse_args()
    require(Path("/.dockerenv").exists() and os.environ.get("MINERALS_MODE") == "admin",
            "run in the existing managed admin container")
    batches = load_batches()
    if arguments.command == "capture":
        capture(batches)
        return
    outputs, queue = prepare(batches)
    for path, data in outputs.items():
        if arguments.command == "verify":
            require(path.read_bytes() == data, "generated research artifact differs: " + path.name)
        else:
            replace_generated(path, data)
    print(json.dumps({"operation": arguments.command, "total_records": queue["total_records"],
                      "counts": queue["counts"], "draft_claim_count": queue["draft_claim_count"],
                      "public_ingestion_allowed": False}))


if __name__ == "__main__":
    main()
