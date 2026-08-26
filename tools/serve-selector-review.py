#!/usr/bin/env python3
"""Serve an assembled Minerals release for local Codex selector review.

The production entry intentionally has a strict CSP. This local-only server maps
the release root to ``selector-review.html``, prevents reuse of the HTML boot
graph, and redirects an unkeyed root request to a per-process review session.
That combination makes the Codex annotation layer receive a fresh document
without weakening exported production files.
"""

from __future__ import annotations

import argparse
import secrets
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlencode, urlsplit, urlunsplit


REVIEW_QUERY_KEY = "selector-review-session"
NO_STORE_SUFFIXES = {
    "",
    ".css",
    ".html",
    ".js",
    ".json",
    ".mjs",
    ".png",
    ".svg",
    ".webp",
}


class SelectorReviewHandler(SimpleHTTPRequestHandler):
    """Static handler that always uses the local selector-safe root entry."""

    review_entry = "selector-review.html"
    review_session = ""

    def _request_parts(self):
        return urlsplit(self.path)

    def _is_root_request(self) -> bool:
        return self._request_parts().path in {"/", "/index.html"}

    def _has_review_session(self) -> bool:
        values = parse_qs(self._request_parts().query).get(REVIEW_QUERY_KEY, [])
        return self.review_session in values

    def _redirect_to_review_session(self) -> None:
        parts = self._request_parts()
        query = parse_qs(parts.query, keep_blank_values=True)
        query[REVIEW_QUERY_KEY] = [self.review_session]
        location = urlunsplit(("", "", "/", urlencode(query, doseq=True), ""))
        self.send_response(302)
        self.send_header("Location", location)
        self.end_headers()

    def do_GET(self) -> None:  # noqa: N802 - stdlib handler API
        if self._is_root_request() and not self._has_review_session():
            self._redirect_to_review_session()
            return
        super().do_GET()

    def do_HEAD(self) -> None:  # noqa: N802 - stdlib handler API
        if self._is_root_request() and not self._has_review_session():
            self._redirect_to_review_session()
            return
        super().do_HEAD()

    def translate_path(self, path: str) -> str:
        parts = urlsplit(path)
        if parts.path in {"/", "/index.html"}:
            path = urlunsplit(("", "", f"/{self.review_entry}", parts.query, ""))
        return super().translate_path(path)

    def end_headers(self) -> None:
        suffix = Path(self._request_parts().path).suffix.lower()
        if self._is_root_request() or suffix in NO_STORE_SUFFIXES:
            self.send_header("Cache-Control", "no-store, max-age=0")
            self.send_header("Pragma", "no-cache")
            self.send_header("Expires", "0")
        self.send_header("X-Robots-Tag", "noindex, nofollow")
        super().end_headers()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", required=True, type=Path)
    parser.add_argument("--bind", default="127.0.0.1")
    parser.add_argument("--port", default=18965, type=int)
    parser.add_argument("--entry", default="selector-review.html")
    parser.add_argument("--session", default="")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    directory = args.directory.resolve()
    entry = directory / args.entry
    if not directory.is_dir():
        raise SystemExit(f"review directory does not exist: {directory}")
    if not entry.is_file():
        raise SystemExit(f"selector review entry does not exist: {entry}")

    session = args.session or secrets.token_hex(8)
    handler_type = type(
        "ConfiguredSelectorReviewHandler",
        (SelectorReviewHandler,),
        {"review_entry": args.entry, "review_session": session},
    )
    handler = partial(handler_type, directory=str(directory))
    server = ThreadingHTTPServer((args.bind, args.port), handler)
    print(
        f"Serving selector review at http://{args.bind}:{args.port}/"
        f"?{REVIEW_QUERY_KEY}={session}#/minerals",
        flush=True,
    )
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
