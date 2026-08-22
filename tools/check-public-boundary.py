#!/usr/bin/env python3
"""Fail when Git tracks private service state, build output, or likely secrets."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import sqlite3
import subprocess
import sys
from urllib.parse import quote


MAX_SECRET_SCAN_BYTES = 2 * 1024 * 1024
PUBLIC_CATALOG_ROOT = "public-catalog"
PUBLIC_CATALOG_MANIFEST = f"{PUBLIC_CATALOG_ROOT}/catalog-manifest.json"
SIDECAR_VALIDATOR = Path(__file__).with_name("validate-public-catalog-sidecars.mjs")

FORBIDDEN_ROOTS = (
    ".archives",
    ".cloudflared",
    ".tmp",
    "dist",
    "public-dist",
    "public-releases",
    "target",
    "tmp",
)

FORBIDDEN_DATA_ROOTS = (
    "data/.report-work",
    "data/backups",
    "data/images",
    "data/reports",
)

SECRET_PATTERNS = (
    (
        "private key",
        re.compile(
            rb"-----BEGIN (?:[A-Z0-9 ]+ )?PRIVATE KEY-----",
            re.IGNORECASE,
        ),
    ),
    (
        "OpenAI API key",
        re.compile(rb"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}"),
    ),
    (
        "GitHub access token",
        re.compile(
            rb"\b(?:gh[opsur]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,})"
        ),
    ),
    (
        "AWS access-key ID",
        re.compile(rb"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b"),
    ),
    (
        "Google API key",
        re.compile(rb"\bAIza[0-9A-Za-z_-]{35}\b"),
    ),
    (
        "live Stripe secret key",
        re.compile(rb"\bsk_live_[0-9A-Za-z]{20,}\b"),
    ),
    (
        "Slack access token",
        re.compile(rb"\bxox[baprs]-[0-9A-Za-z-]{20,}\b"),
    ),
)

SECRET_ASSIGNMENT = re.compile(
    rb"(?m)^\s*(?:export\s+)?"
    rb"(ADMIN_PASSWORD|OPENAI_API_KEY|INGESTION_API_TOKEN|"
    rb"CLOUDFLARE_API_TOKEN|CF_API_TOKEN|AWS_SECRET_ACCESS_KEY|"
    rb"SESSION_SECRET)\s*[:=]\s*"
    rb"[\"']?([^\s#\"']{12,})"
)

PLACEHOLDER_MARKERS = (
    b"changeme",
    b"example",
    b"fixture",
    b"optional",
    b"placeholder",
    b"redacted",
    b"replace",
    b"unset",
    b"your-",
    b"xxxx",
)


class BoundaryError(RuntimeError):
    """A Git query required by the boundary check failed."""


def git(repo: Path, *arguments: str, input_bytes: bytes | None = None) -> bytes:
    command = [
        "git",
        "-c",
        f"safe.directory={repo.as_posix()}",
        *arguments,
    ]
    completed = subprocess.run(
        command,
        cwd=repo,
        input=input_bytes,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        detail = completed.stderr.decode("utf-8", "replace").strip()
        raise BoundaryError(f"{' '.join(command[:2] + list(arguments))}: {detail}")
    return completed.stdout


def repository_root() -> Path:
    candidate = Path.cwd().resolve()
    raw = git(candidate, "rev-parse", "--show-toplevel")
    return Path(raw.decode("utf-8", "surrogateescape").strip()).resolve()


def tracked_paths(repo: Path) -> list[str]:
    raw = git(repo, "ls-files", "--cached", "-z")
    return sorted(
        item.decode("utf-8", "surrogateescape").replace("\\", "/")
        for item in raw.split(b"\0")
        if item
    )


def historical_paths(repo: Path) -> list[str]:
    raw = git(repo, "log", "--all", "--format=", "--name-only", "-z")
    return sorted(
        {
            item.decode("utf-8", "surrogateescape").replace("\\", "/")
            for item in raw.split(b"\0")
            if item
        }
    )


def below(path: str, root: str) -> bool:
    return path == root or path.startswith(f"{root}/")


def is_public_catalog_database_path(path: str) -> bool:
    return bool(
        re.fullmatch(
            rf"{re.escape(PUBLIC_CATALOG_ROOT)}/data/"
            r"catalog-[0-9a-f]{64}\.sqlite3(?:\.(?:br|gz))?",
            path,
        )
    )


def public_catalog_snapshot_findings(
    repo: Path, paths: list[str], *, validate_compression: bool = True
) -> list[tuple[str, str]]:
    actual = {path for path in paths if below(path, PUBLIC_CATALOG_ROOT)}
    raw_databases = sorted(
        path
        for path in actual
        if re.fullmatch(
            rf"{re.escape(PUBLIC_CATALOG_ROOT)}/data/"
            r"catalog-[0-9a-f]{64}\.sqlite3",
            path,
        )
    )
    if len(raw_databases) != 1:
        return [
            (
                PUBLIC_CATALOG_ROOT,
                "must track exactly one sanitized raw catalog database",
            )
        ]

    raw = raw_databases[0]
    expected = {PUBLIC_CATALOG_MANIFEST, raw, f"{raw}.br", f"{raw}.gz"}
    if actual != expected:
        return [
            (
                PUBLIC_CATALOG_ROOT,
                "must contain exactly the manifest and one matching raw/br/gz catalog set",
            )
        ]

    findings: list[tuple[str, str]] = []
    try:
        for relative in sorted(expected):
            candidate = repo.joinpath(*PurePosixPath(relative).parts)
            if candidate.is_symlink() or not candidate.is_file():
                findings.append((relative, "worktree artifact is not a regular file"))
                continue
            indexed = git(repo, "cat-file", "blob", f":{relative}")
            if candidate.read_bytes() != indexed:
                findings.append(
                    (
                        relative,
                        "worktree bytes differ from the staged Git blob; refusing to scan a different file",
                    )
                )
        if findings:
            return findings

        manifest_bytes = git(repo, "cat-file", "blob", f":{PUBLIC_CATALOG_MANIFEST}")
        manifest = json.loads(manifest_bytes)
        database = manifest["database"]
        relative_database = raw.removeprefix(f"{PUBLIC_CATALOG_ROOT}/")
        digest = raw.removeprefix(f"{PUBLIC_CATALOG_ROOT}/data/catalog-").removesuffix(
            ".sqlite3"
        )
        if database["path"] != relative_database:
            findings.append(
                (PUBLIC_CATALOG_MANIFEST, "database path does not name the tracked raw file")
            )
        if database["sha256"] != f"sha256:{digest}":
            findings.append(
                (PUBLIC_CATALOG_MANIFEST, "database SHA-256 does not match its filename")
            )
        raw_bytes = git(repo, "cat-file", "blob", f":{raw}")
        if len(raw_bytes) != database["bytes"]:
            findings.append((raw, "database byte length does not match the manifest"))
        if hashlib.sha256(raw_bytes).hexdigest() != digest:
            findings.append((raw, "database content does not match its SHA-256 filename"))
        if validate_compression:
            node = os.environ.get("WAAJACU_NODE", "node")
            try:
                completed = subprocess.run(
                    [node, str(SIDECAR_VALIDATOR), str(repo / PUBLIC_CATALOG_ROOT)],
                    cwd=repo,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    check=False,
                    text=True,
                )
            except FileNotFoundError as error:
                raise BoundaryError(
                    "Node.js is required to validate public catalog Brotli and gzip sidecars"
                ) from error
            if completed.returncode != 0:
                detail = completed.stderr.strip() or "sidecar validator returned no detail"
                raise BoundaryError(detail)
    except BoundaryError as error:
        findings.append((PUBLIC_CATALOG_ROOT, f"catalog validation failed: {error}"))
    except (KeyError, TypeError, json.JSONDecodeError) as error:
        findings.append((PUBLIC_CATALOG_MANIFEST, f"cannot validate catalog manifest: {error}"))
    return findings


def forbidden_path_reason(path: str) -> str | None:
    normalized = PurePosixPath(path).as_posix()
    while normalized.startswith("./"):
        normalized = normalized[2:]
    parts = PurePosixPath(normalized).parts
    if not normalized or any(part in {".", "..", ""} for part in parts):
        return "unsafe repository path"

    if below(normalized, PUBLIC_CATALOG_ROOT):
        if normalized == PUBLIC_CATALOG_MANIFEST or is_public_catalog_database_path(
            normalized
        ):
            return None
        return "unexpected tracked public-catalog path"

    for root in (*FORBIDDEN_ROOTS, *FORBIDDEN_DATA_ROOTS):
        if below(normalized, root):
            return f"private/runtime/build path ({root})"

    if "target" in parts or ".tmp" in parts or "__pycache__" in parts:
        return "generated build or interpreter output"

    filename = parts[-1]
    if filename.startswith(".env") and normalized not in {".env", ".env.example"}:
        return "local environment or service-secret file"

    lowered = normalized.lower()
    if re.fullmatch(
        r"data/.*\.(?:db|sqlite|sqlite3)(?:[-.](?:shm|wal|journal|backup|bak))?",
        lowered,
    ):
        return "operational database or journal"
    if re.fullmatch(r"data/\.registry-ready-.*\.tmp", lowered):
        return "registry publication scratch file"
    if lowered.startswith("data/minerals/") and re.search(r"/report\.[^/]+$", lowered):
        return "generated mineral report"
    if re.search(
        r"(?:^|/)catalog-[0-9a-f]{64}\.sqlite3(?:\.(?:br|gz))?$",
        lowered,
    ):
        return "generated public catalog database (publish as a release asset)"
    if filename.lower() == "minerals.db" or re.search(
        r"\.(?:db|sqlite|sqlite3)-(?:shm|wal)$", filename.lower()
    ):
        return "operational database or journal"
    if filename.lower().endswith((".p12", ".pfx")):
        return "private key container"
    if normalized == "waajacu-public-catalog-pages.tar.gz":
        return "generated Pages release archive"
    return None


def shannon_entropy(value: bytes) -> float:
    if not value:
        return 0.0
    counts = {byte: value.count(byte) for byte in set(value)}
    return -sum(
        (count / len(value)) * math.log2(count / len(value))
        for count in counts.values()
    )


def secret_reasons(content: bytes) -> set[str]:
    reasons = {
        label for label, pattern in SECRET_PATTERNS if pattern.search(content)
    }
    for match in SECRET_ASSIGNMENT.finditer(content):
        name = match.group(1)
        value = match.group(2)
        minimum_length = 12 if name == b"ADMIN_PASSWORD" else 24
        if len(value) < minimum_length:
            continue
        lowered = value.lower()
        if lowered.startswith((b"${", b"$env:", b"{{")):
            continue
        if any(marker in lowered for marker in PLACEHOLDER_MARKERS):
            continue
        if shannon_entropy(value) >= 3.5:
            reasons.add(f"literal value assigned to {name.decode('ascii')}")
    return reasons


def scan_tracked_secrets(repo: Path, paths: list[str]) -> list[tuple[str, str]]:
    findings: list[tuple[str, str]] = []
    for path in paths:
        try:
            size_raw = git(repo, "cat-file", "-s", f":{path}")
            size = int(size_raw.strip())
            if size <= 0 or size > MAX_SECRET_SCAN_BYTES:
                continue
            content = git(repo, "cat-file", "blob", f":{path}")
        except (BoundaryError, ValueError):
            # Submodules and unusual index entries are not blobs to secret-scan.
            continue
        if b"\0" in content:
            continue
        for reason in sorted(secret_reasons(content)):
            findings.append((path, reason))
    return findings


def scan_public_catalog_text(
    repo: Path, paths: list[str]
) -> list[tuple[str, str]]:
    databases = [
        path
        for path in paths
        if is_public_catalog_database_path(path) and path.endswith(".sqlite3")
    ]
    findings: list[tuple[str, str]] = []
    for relative in databases:
        candidate = repo.joinpath(*PurePosixPath(relative).parts)
        if candidate.is_symlink() or not candidate.is_file():
            raise BoundaryError(
                f"public catalog database is not a regular file: {relative}"
            )
        database_path = candidate.resolve()
        try:
            database_path.relative_to(repo)
        except ValueError as error:
            raise BoundaryError(
                f"public catalog database escapes the repository: {relative}"
            ) from error
        uri = f"file:{quote(database_path.as_posix(), safe='/:')}?mode=ro&immutable=1"
        try:
            connection = sqlite3.connect(uri, uri=True)
            connection.execute("PRAGMA query_only = ON")
            tables = list(
                connection.execute(
                    "SELECT name, sql FROM sqlite_schema "
                    "WHERE type = 'table' AND name NOT LIKE 'sqlite_%' "
                    "ORDER BY name"
                )
            )
            for table, schema_sql in tables:
                if isinstance(schema_sql, str):
                    for reason in sorted(secret_reasons(schema_sql.encode("utf-8"))):
                        findings.append((f"{relative}:schema {table}", reason))
                escaped_table = str(table).replace('"', '""')
                columns = [
                    row[1]
                    for row in connection.execute(
                        f'PRAGMA table_xinfo("{escaped_table}")'
                    )
                ]
                if not columns:
                    continue
                selection = ", ".join(
                    f'"{column.replace(chr(34), chr(34) * 2)}"'
                    for column in columns
                )
                for row_number, row in enumerate(
                    connection.execute(
                        f'SELECT {selection} FROM "{escaped_table}"'
                    ),
                    start=1,
                ):
                    for column, value in zip(columns, row):
                        if isinstance(value, str):
                            content = value.encode("utf-8")
                        elif isinstance(value, bytes):
                            content = value
                        else:
                            continue
                        for reason in sorted(secret_reasons(content)):
                            findings.append(
                                (
                                    f"{relative}:{table}.{column} row {row_number}",
                                    reason,
                                )
                            )
            connection.close()
        except sqlite3.Error as error:
            raise BoundaryError(
                f"failed to scan public catalog text in {relative}: {error}"
            ) from error
    return findings


def historical_blob_candidates(
    repo: Path,
) -> tuple[list[tuple[str, str, int]], list[tuple[str, str]]]:
    raw_objects = git(repo, "rev-list", "--objects", "--all")
    object_paths: dict[str, str] = {}
    for line in raw_objects.splitlines():
        if not line:
            continue
        fields = line.split(b" ", 1)
        object_id = fields[0].decode("ascii")
        path = (
            fields[1].decode("utf-8", "surrogateescape")
            if len(fields) == 2
            else "<unnamed>"
        )
        object_paths.setdefault(object_id, path)

    if not object_paths:
        return [], []
    request = "".join(f"{object_id}\n" for object_id in object_paths).encode("ascii")
    metadata = git(
        repo,
        "cat-file",
        "--batch-check=%(objectname) %(objecttype) %(objectsize)",
        input_bytes=request,
    )
    candidates: list[tuple[str, str, int]] = []
    oversized: list[tuple[str, str]] = []
    for line in metadata.splitlines():
        fields = line.decode("ascii").split()
        if len(fields) != 3 or fields[1] != "blob":
            continue
        size = int(fields[2])
        if 0 < size <= MAX_SECRET_SCAN_BYTES:
            candidates.append((fields[0], object_paths[fields[0]], size))
        elif size > MAX_SECRET_SCAN_BYTES:
            path = object_paths[fields[0]]
            if (
                forbidden_path_reason(path) is None
                and not is_public_catalog_database_path(path)
            ):
                oversized.append(
                    (
                        f"{path} @ {fields[0][:12]}",
                        f"blob exceeds the {MAX_SECRET_SCAN_BYTES}-byte secret-scan limit",
                    )
                )
    return candidates, oversized


def scan_historical_secrets(repo: Path) -> list[tuple[str, str]]:
    candidates, findings = historical_blob_candidates(repo)
    if not candidates:
        return findings

    request = "".join(f"{object_id}\n" for object_id, _, _ in candidates).encode(
        "ascii"
    )
    batch = git(repo, "cat-file", "--batch", input_bytes=request)
    offset = 0
    for expected_id, path, expected_size in candidates:
        header_end = batch.find(b"\n", offset)
        if header_end < 0:
            raise BoundaryError("git cat-file --batch returned a truncated header")
        header = batch[offset:header_end].decode("ascii").split()
        if (
            len(header) != 3
            or header[0] != expected_id
            or header[1] != "blob"
            or int(header[2]) != expected_size
        ):
            raise BoundaryError("git cat-file --batch returned unexpected metadata")
        content_start = header_end + 1
        content_end = content_start + expected_size
        if content_end >= len(batch) or batch[content_end : content_end + 1] != b"\n":
            raise BoundaryError("git cat-file --batch returned truncated blob data")
        content = batch[content_start:content_end]
        offset = content_end + 1
        if b"\0" in content:
            continue
        for reason in sorted(secret_reasons(content)):
            findings.append((f"{path} @ {expected_id[:12]}", reason))
    if offset != len(batch):
        raise BoundaryError("git cat-file --batch returned unexpected trailing data")
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Verify that tracked content stays on the public side of the "
            "repository boundary."
        )
    )
    parser.add_argument(
        "--history",
        action="store_true",
        help="also reject forbidden paths reachable anywhere in Git history",
    )
    arguments = parser.parse_args()

    try:
        repo = repository_root()
        paths = tracked_paths(repo)
        path_findings = [
            (path, reason)
            for path in paths
            if (reason := forbidden_path_reason(path)) is not None
        ]
        path_findings.extend(public_catalog_snapshot_findings(repo, paths))
        secret_findings = scan_tracked_secrets(repo, paths)
        secret_findings.extend(scan_public_catalog_text(repo, paths))

        history_findings: list[tuple[str, str]] = []
        historical_secret_findings: list[tuple[str, str]] = []
        if arguments.history:
            history_findings = [
                (path, reason)
                for path in historical_paths(repo)
                if (reason := forbidden_path_reason(path)) is not None
            ]
            historical_secret_findings = scan_historical_secrets(repo)
    except BoundaryError as error:
        print(f"public-boundary check could not run: {error}", file=sys.stderr)
        return 2

    if (
        path_findings
        or secret_findings
        or history_findings
        or historical_secret_findings
    ):
        print("Public repository boundary check failed:", file=sys.stderr)
        for path, reason in path_findings:
            print(f"  tracked path: {path} ({reason})", file=sys.stderr)
        for path, reason in secret_findings:
            print(f"  likely tracked secret: {path} ({reason})", file=sys.stderr)
        for path, reason in history_findings:
            print(f"  historical path: {path} ({reason})", file=sys.stderr)
        for blob, reason in historical_secret_findings:
            print(f"  likely historical secret: {blob} ({reason})", file=sys.stderr)
        print(
            "Keep service state and credentials in ignored local storage; "
            "publish only the validated public catalog snapshot and app assets.",
            file=sys.stderr,
        )
        return 1

    history_note = " and reachable history" if arguments.history else ""
    print(f"Public repository boundary passed for {len(paths)} tracked paths{history_note}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
