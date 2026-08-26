#!/usr/bin/env python3
"""Verify that the live Pages origin serves the exact committed public files."""

from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import sys
import time
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlsplit, urlunsplit
from urllib.request import Request, urlopen


PUBLIC_APP_FILES = (
    "index.html",
    "app.css",
    "app.js",
    "app-core.mjs",
    "webmcp.mjs",
    "catalog-worker.js",
    "THIRD_PARTY_NOTICES.md",
    "assets/atlas-chemical-family-v2.png",
    "assets/atlas-crystal-system-v2.png",
    "assets/atlas-method-v2.png",
    "assets/atlas-mountain-v2.png",
    "assets/atlas-place-origin-v2.png",
    "assets/atlas-quartz-v2.png",
    "assets/atlas-source-v2.png",
    "assets/waajacu-minerals-social.png",
    "vendor/sqlite/index.mjs",
    "vendor/sqlite/sqlite3.wasm",
    "vendor/sqlite/LICENSE.txt",
    "map/map-loader.js",
    "map/map.css",
    "map/minerals_map.wasm",
)
MANIFEST_FILE = "catalog-manifest.json"


class VerificationError(RuntimeError):
    """The live site cannot be proven equal to the committed snapshot."""


def require_directory(path: Path, label: str) -> Path:
    if path.is_symlink() or not path.is_dir():
        raise VerificationError(f"{label} must be a real directory: {path}")
    return path.resolve()


def require_file(root: Path, relative: str) -> Path:
    pure = PurePosixPath(relative)
    if pure.is_absolute() or not pure.parts or any(
        part in {"", ".", ".."} for part in pure.parts
    ):
        raise VerificationError(f"unsafe expected public path: {relative}")
    candidate = root.joinpath(*pure.parts)
    if candidate.is_symlink() or not candidate.is_file():
        raise VerificationError(f"missing regular public file: {candidate}")
    resolved = candidate.resolve()
    try:
        resolved.relative_to(root)
    except ValueError as error:
        raise VerificationError(f"public file escapes its root: {candidate}") from error
    return resolved


def load_expected_files(app_root: Path, catalog_root: Path) -> dict[str, bytes]:
    app_root = require_directory(app_root, "public app root")
    catalog_root = require_directory(catalog_root, "public catalog root")
    expected = {
        relative: require_file(app_root, relative).read_bytes()
        for relative in PUBLIC_APP_FILES
    }
    expected[""] = expected["index.html"]

    manifest_path = require_file(catalog_root, MANIFEST_FILE)
    manifest_bytes = manifest_path.read_bytes()
    try:
        manifest = json.loads(manifest_bytes)
        database_path = manifest["database"]["path"]
    except (KeyError, TypeError, json.JSONDecodeError) as error:
        raise VerificationError("public catalog manifest is malformed") from error
    if not isinstance(database_path, str) or not re.fullmatch(
        r"data/catalog-[0-9a-f]{64}\.sqlite3", database_path
    ):
        raise VerificationError("public catalog manifest has an unsafe database path")

    expected[MANIFEST_FILE] = manifest_bytes
    expected[database_path] = require_file(catalog_root, database_path).read_bytes()
    for suffix in (".br", ".gz"):
        relative = f"{database_path}{suffix}"
        expected[relative] = require_file(catalog_root, relative).read_bytes()
    return expected


def normalized_base_url(value: str) -> str:
    parsed = urlsplit(value)
    if parsed.scheme != "https" or not parsed.netloc:
        raise VerificationError("the live Pages base URL must be HTTPS")
    if parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise VerificationError("the live Pages base URL cannot contain credentials or a query")
    path = parsed.path.rstrip("/") + "/"
    return urlunsplit((parsed.scheme, parsed.netloc, path, "", ""))


def public_url(base_url: str, relative: str) -> str:
    parsed = urlsplit(base_url)
    encoded_path = "/".join(quote(part, safe="") for part in PurePosixPath(relative).parts)
    path = f"{parsed.path}{encoded_path}"
    return urlunsplit((parsed.scheme, parsed.netloc, path, "", ""))


def validate_live_response_headers(url: str, headers: dict[str, str]) -> None:
    normalized = {name.lower(): value for name, value in headers.items()}
    path = urlsplit(url).path
    media_type = normalized.get("content-type", "").split(";", 1)[0].strip().lower()
    if path.endswith(".mjs"):
        if media_type not in {
            "text/javascript",
            "application/javascript",
            "text/ecmascript",
            "application/ecmascript",
        }:
            raise VerificationError(
                f"WebMCP module has a non-JavaScript Content-Type for {url}: "
                f"{media_type or 'missing'}"
            )
        if re.search(
            r"(?:^|,)\s*immutable\s*(?:,|$)",
            normalized.get("cache-control", ""),
            re.IGNORECASE,
        ):
            raise VerificationError(
                f"stable-named WebMCP module must not use immutable caching: {url}"
            )

    if path.endswith("/") or path.endswith("/index.html"):
        if media_type != "text/html":
            raise VerificationError(
                f"catalog document has an invalid Content-Type for {url}: "
                f"{media_type or 'missing'}"
            )
        origin_agent_cluster = normalized.get("origin-agent-cluster")
        if origin_agent_cluster is not None and origin_agent_cluster.strip() != "?1":
            raise VerificationError(
                f"catalog document opts out of the WebMCP origin agent cluster: {url}"
            )
        permissions_policy = normalized.get("permissions-policy", "")
        tools = re.search(
            r"(?:^|,)\s*tools\s*=\s*(\([^)]*\))",
            permissions_policy,
            re.IGNORECASE,
        )
        if tools and re.sub(r"\s+", "", tools.group(1)).lower() != "(self)":
            raise VerificationError(
                f"catalog document does not limit WebMCP tools to self: {url}"
            )


def fetch_bytes(url: str, maximum_bytes: int, timeout_seconds: float) -> bytes:
    request = Request(
        url,
        headers={
            "Accept-Encoding": "identity",
            "User-Agent": "Waajacu-Pages-Verification/1",
        },
    )
    with urlopen(request, timeout=max(1.0, min(90.0, timeout_seconds))) as response:
        if response.status != 200:
            raise VerificationError(f"unexpected HTTP status {response.status} for {url}")
        response_headers = {
            name.lower(): ", ".join(response.headers.get_all(name) or [])
            for name in response.headers.keys()
        }
        validate_live_response_headers(url, response_headers)
        content = response.read(maximum_bytes + 1)
    if len(content) > maximum_bytes:
        raise VerificationError(f"live response is larger than expected for {url}")
    return content


def verify_once(
    base_url: str, expected: dict[str, bytes], deadline: float
) -> list[str]:
    mismatches: list[str] = []

    def fetch_one(relative: str, wanted: bytes) -> tuple[str, bytes | None, str | None]:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return relative, None, "overall verification deadline expired"
        url = public_url(base_url, relative)
        try:
            received = fetch_bytes(url, len(wanted), remaining)
        except (HTTPError, URLError, TimeoutError, VerificationError) as error:
            return relative, None, str(error)
        return relative, received, None

    with ThreadPoolExecutor(max_workers=min(8, len(expected))) as executor:
        futures = {
            executor.submit(fetch_one, relative, wanted): (relative, wanted)
            for relative, wanted in expected.items()
        }
        for future in as_completed(futures):
            relative, wanted = futures[future]
            display = relative or "/"
            try:
                _, received, error = future.result()
            except Exception as error:  # pragma: no cover - defensive worker boundary
                mismatches.append(f"{display}: {error}")
                continue
            if error is not None or received is None:
                mismatches.append(f"{display}: {error}")
                continue
            if received != wanted:
                wanted_hash = hashlib.sha256(wanted).hexdigest()
                received_hash = hashlib.sha256(received).hexdigest()
                mismatches.append(
                    f"{display}: expected {len(wanted)} bytes sha256:{wanted_hash}, "
                    f"received {len(received)} bytes sha256:{received_hash}"
                )
    return mismatches


def verify_until_live(
    base_url: str,
    expected: dict[str, bytes],
    commit: str,
    timeout_seconds: int,
) -> None:
    if not re.fullmatch(r"[0-9a-f]{40,64}", commit):
        raise VerificationError("commit must be 40 to 64 lowercase hexadecimal characters")
    if timeout_seconds < 1 or timeout_seconds > 1800:
        raise VerificationError("timeout must be between 1 and 1800 seconds")
    base_url = normalized_base_url(base_url)
    deadline = time.monotonic() + timeout_seconds
    last_mismatches: list[str] = []
    while True:
        last_mismatches = verify_once(base_url, expected, deadline)
        if not last_mismatches:
            print(
                f"Live Pages matches commit {commit} for {len(expected)} public files."
            )
            return
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        print(
            f"Live Pages has not converged to {commit} yet "
            f"({len(last_mismatches)} mismatches); retrying.",
            file=sys.stderr,
        )
        time.sleep(min(15, remaining))

    details = "\n".join(f"  {item}" for item in last_mismatches[:8])
    raise VerificationError(
        f"live Pages did not match commit {commit} within {timeout_seconds} seconds:\n{details}"
    )


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list-app-files", action="store_true")
    parser.add_argument("--base-url")
    parser.add_argument("--app-root", type=Path)
    parser.add_argument("--catalog-root", type=Path)
    parser.add_argument("--commit")
    parser.add_argument("--timeout-seconds", type=int, default=900)
    arguments = parser.parse_args()
    if arguments.list_app_files:
        if any(
            value is not None
            for value in (
                arguments.base_url,
                arguments.app_root,
                arguments.catalog_root,
                arguments.commit,
            )
        ):
            parser.error("--list-app-files cannot be combined with verification options")
        return arguments
    missing = [
        name
        for name in ("base_url", "app_root", "catalog_root", "commit")
        if getattr(arguments, name) is None
    ]
    if missing:
        parser.error("missing required verification options: " + ", ".join(missing))
    return arguments


def main() -> int:
    arguments = parse_arguments()
    if arguments.list_app_files:
        print("\n".join(PUBLIC_APP_FILES))
        return 0
    try:
        expected = load_expected_files(arguments.app_root, arguments.catalog_root)
        verify_until_live(
            arguments.base_url,
            expected,
            arguments.commit,
            arguments.timeout_seconds,
        )
    except VerificationError as error:
        print(f"live Pages verification failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
