from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import time
import unittest


MODULE_PATH = Path(__file__).with_name("verify-pages-live.py")
SPEC = importlib.util.spec_from_file_location("verify_pages_live", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)


class PagesLiveVerificationTests(unittest.TestCase):
    def test_loads_exact_app_manifest_and_compressed_catalog_files(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            app = root / "app"
            catalog = root / "catalog"
            app.mkdir()
            catalog.mkdir()
            for relative in VERIFY.PUBLIC_APP_FILES:
                path = app.joinpath(*Path(relative).parts)
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(f"app:{relative}".encode())

            digest = "a" * 64
            database = f"data/catalog-{digest}.sqlite3"
            manifest = {"database": {"path": database}}
            (catalog / "data").mkdir()
            (catalog / "catalog-manifest.json").write_text(
                json.dumps(manifest), encoding="utf-8"
            )
            (catalog / database).write_bytes(b"sqlite")
            (catalog / f"{database}.br").write_bytes(b"brotli")
            (catalog / f"{database}.gz").write_bytes(b"gzip")

            expected = VERIFY.load_expected_files(app, catalog)
            self.assertEqual(
                set(expected),
                {
                    *VERIFY.PUBLIC_APP_FILES,
                    "",
                    "catalog-manifest.json",
                    database,
                    f"{database}.br",
                    f"{database}.gz",
                },
            )
            self.assertEqual(expected[""], expected["index.html"])
            self.assertEqual(expected[database], b"sqlite")

    def test_base_url_requires_https_without_query_or_fragment(self) -> None:
        self.assertEqual(
            VERIFY.normalized_base_url("https://minerals.example.test/catalog"),
            "https://minerals.example.test/catalog/",
        )
        for invalid in (
            "http://minerals.example.test/",
            "https://minerals.example.test/?preview=2",
            "https://minerals.example.test/#/about",
            "https://user:secret@minerals.example.test/",
        ):
            with self.assertRaises(VERIFY.VerificationError):
                VERIFY.normalized_base_url(invalid)

    def test_public_url_keeps_catalog_subpath_without_a_cache_buster(self) -> None:
        url = VERIFY.public_url(
            "https://example.test/minerals/",
            "assets/logo transparent.png",
        )
        self.assertEqual(
            url,
            "https://example.test/minerals/assets/logo%20transparent.png",
        )
        self.assertEqual(
            VERIFY.public_url("https://example.test/minerals/", ""),
            "https://example.test/minerals/",
        )

    def test_verify_once_checks_canonical_urls_and_reports_each_mismatch(self) -> None:
        expected = {"": b"root", "app.js": b"current"}
        seen: list[str] = []
        original = VERIFY.fetch_bytes

        def fake_fetch(url: str, maximum_bytes: int, timeout_seconds: float) -> bytes:
            seen.append(url)
            self.assertGreater(timeout_seconds, 0)
            return b"root" if url.endswith("/catalog/") else b"old"

        VERIFY.fetch_bytes = fake_fetch
        try:
            mismatches = VERIFY.verify_once(
                "https://example.test/catalog/",
                expected,
                time.monotonic() + 5,
            )
        finally:
            VERIFY.fetch_bytes = original

        self.assertEqual(
            set(seen),
            {
                "https://example.test/catalog/",
                "https://example.test/catalog/app.js",
            },
        )
        self.assertEqual(len(mismatches), 1)
        self.assertTrue(mismatches[0].startswith("app.js:"), mismatches)
        self.assertTrue(all("?" not in url for url in seen))


if __name__ == "__main__":
    unittest.main()
