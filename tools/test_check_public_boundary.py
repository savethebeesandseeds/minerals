from __future__ import annotations

import importlib.util
import gzip
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import unittest


MODULE_PATH = Path(__file__).with_name("check-public-boundary.py")
SPEC = importlib.util.spec_from_file_location("check_public_boundary", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
BOUNDARY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BOUNDARY)


class PublicBoundaryTests(unittest.TestCase):
    def test_compressed_only_catalog_has_the_same_staged_identity_checks(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            raw_bytes = b"sanitized compressed-only catalog fixture"
            paths = self.stage_compressed_catalog(repo, raw_bytes)
            self.assertEqual(BOUNDARY.public_catalog_snapshot_findings(repo, paths), [])
            gzip_path = repo / paths[-1]
            changed = bytearray(gzip_path.read_bytes())
            changed[0] ^= 1
            gzip_path.write_bytes(changed)
            mismatch = BOUNDARY.public_catalog_snapshot_findings(repo, paths)
            self.assertTrue(any("worktree bytes differ" in reason for _, reason in mismatch), mismatch)

    def test_compressed_only_catalog_requires_both_sidecars_and_no_extra_worktree_files(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            raw_bytes = b"sanitized compressed-only catalog fixture"
            paths = self.stage_compressed_catalog(repo, raw_bytes)
            self.assertTrue(BOUNDARY.public_catalog_snapshot_findings(repo, paths[:-1]))
            raw = paths[-1].removesuffix(".gz")
            (repo / raw).write_bytes(raw_bytes)
            findings = BOUNDARY.public_catalog_snapshot_findings(repo, paths)
            self.assertTrue(any("unexpected or missing entry" in reason for _, reason in findings), findings)

    def test_compressed_catalog_text_and_schema_are_secret_scanned_without_printing_values(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            row_token = "sk-" + "A1b2C3d4E5f6G7h8I9j0K1l2M3n4P5q6"
            schema_token = "sk-" + "Z9y8X7w6V5u4T3s2R1q0P9o8N7m6L5k4"
            connection = sqlite3.connect(":memory:")
            connection.execute("CREATE TABLE evidence (summary TEXT)")
            connection.execute("INSERT INTO evidence VALUES (?)", (row_token,))
            connection.execute(f"CREATE TABLE notes (summary TEXT DEFAULT '{schema_token}')")
            raw_bytes = connection.serialize()
            connection.close()
            paths = self.stage_compressed_catalog(repo, raw_bytes)
            self.assertEqual(BOUNDARY.public_catalog_snapshot_findings(repo, paths), [])
            findings = BOUNDARY.scan_public_catalog_text(repo, paths)
            self.assertEqual(len(findings), 2, findings)
            self.assertTrue(any("schema notes" in relative for relative, _ in findings), findings)
            self.assertNotIn(row_token, repr(findings))
            self.assertNotIn(schema_token, repr(findings))

    def test_compressed_catalog_decoder_is_bounded_and_refuses_trailing_streams(self) -> None:
        raw = b"x" * 4096
        encoded = gzip.compress(raw)
        self.assertEqual(BOUNDARY.decode_catalog_gzip(encoded, len(raw)), raw)
        for candidate, size in ((encoded, 4), (encoded + b"PRIVATE-TRAILER", len(raw)), (encoded + gzip.compress(b""), len(raw)), (encoded[:-4], len(raw))):
            with self.subTest(size=size, bytes=len(candidate)):
                with self.assertRaises(BOUNDARY.BoundaryError):
                    BOUNDARY.decode_catalog_gzip(candidate, size)
        for size in (True, 0, -1, BOUNDARY.MAX_DATABASE_BYTES + 1):
            with self.subTest(size=size):
                with self.assertRaises(BOUNDARY.BoundaryError):
                    BOUNDARY.decode_catalog_gzip(encoded, size)

    def test_private_pilot_artifacts_cannot_enter_the_public_repository(self) -> None:
        for relative in (
            "data/pilots/cod-crystallography-v1/metadata-discovery-execution-index.json",
            "data/pilots/cod-crystallography-v1/objects/sha256/ab/raw-cif",
            "data/pilots/another-source/review.json",
        ):
            with self.subTest(relative=relative):
                self.assertIsNotNone(BOUNDARY.forbidden_path_reason(relative))
        self.assertIsNone(
            BOUNDARY.forbidden_path_reason("schemas/pilots/cod-evidence-v1.schema.json")
        )

    def test_public_catalog_snapshot_is_tied_to_one_manifest_named_triplet(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            self.run_git(repo, "init", "--initial-branch=main")
            raw_bytes = b"sanitized public sqlite fixture"
            digest = hashlib.sha256(raw_bytes).hexdigest()
            raw = f"public-catalog/data/catalog-{digest}.sqlite3"
            paths = [
                "public-catalog/catalog-manifest.json",
                raw,
                f"{raw}.br",
                f"{raw}.gz",
            ]
            (repo / "public-catalog/data").mkdir(parents=True)
            (repo / raw).write_bytes(raw_bytes)
            (repo / f"{raw}.br").write_bytes(b"br")
            (repo / f"{raw}.gz").write_bytes(b"gz")
            (repo / "public-catalog/catalog-manifest.json").write_text(
                json.dumps(
                    {
                        "database": {
                            "path": raw.removeprefix("public-catalog/"),
                            "sha256": f"sha256:{digest}",
                            "bytes": len(raw_bytes),
                        }
                    }
                ),
                encoding="utf-8",
            )
            self.run_git(repo, "add", "public-catalog")

            self.assertEqual(
                BOUNDARY.public_catalog_snapshot_findings(
                    repo, paths, validate_compression=False
                ),
                [],
            )
            self.assertTrue(
                BOUNDARY.public_catalog_snapshot_findings(
                    repo, paths[:-1], validate_compression=False
                )
            )
            (repo / raw).write_bytes(b"different safe worktree copy")
            mismatch = BOUNDARY.public_catalog_snapshot_findings(
                repo, paths, validate_compression=False
            )
            self.assertTrue(
                any("worktree bytes differ" in reason for _, reason in mismatch),
                mismatch,
            )

    def test_only_the_explicit_public_catalog_snapshot_paths_are_allowed(self) -> None:
        digest = "a" * 64
        self.assertIsNone(
            BOUNDARY.forbidden_path_reason("public-catalog/catalog-manifest.json")
        )
        for suffix in ("", ".br", ".gz"):
            self.assertIsNone(
                BOUNDARY.forbidden_path_reason(
                    f"public-catalog/data/catalog-{digest}.sqlite3{suffix}"
                )
            )
        self.assertIsNotNone(
            BOUNDARY.forbidden_path_reason("public-catalog/private-notes.txt")
        )
        self.assertIsNotNone(
            BOUNDARY.forbidden_path_reason(
                f"downloads/catalog-{digest}.sqlite3"
            )
        )
        for near_miss in (
            f"public-catalog/data/catalog-{'A' * 64}.sqlite3",
            f"public-catalog/data/catalog-{digest}.sqlite3-wal",
            f"public-catalog/data/catalog-{digest}.sqlite3.backup",
        ):
            self.assertIsNotNone(BOUNDARY.forbidden_path_reason(near_miss))

    def test_public_catalog_text_is_scanned_without_printing_the_value(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            relative = (
                "public-catalog/data/catalog-" + "b" * 64 + ".sqlite3"
            )
            database = repo / relative
            database.parent.mkdir(parents=True)
            token = "sk-" + "A1b2C3d4E5f6G7h8I9j0K1l2M3n4P5q6"
            connection = sqlite3.connect(database)
            connection.execute("CREATE TABLE evidence (summary TEXT)")
            connection.execute("INSERT INTO evidence VALUES (?)", (token,))
            connection.commit()
            connection.close()

            findings = BOUNDARY.scan_public_catalog_text(repo, [relative])
            self.assertEqual(len(findings), 1)
            self.assertIn("OpenAI API key", findings[0][1])
            self.assertNotIn(token, repr(findings))

    def test_unexpected_public_catalog_tables_are_also_secret_scanned(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            relative = (
                "public-catalog/data/catalog-" + "c" * 64 + ".sqlite3"
            )
            database = repo / relative
            database.parent.mkdir(parents=True)
            token = "sk-" + "Z9y8X7w6V5u4T3s2R1q0P9o8N7m6L5k4"
            connection = sqlite3.connect(database)
            connection.execute("CREATE TABLE admin (password BLOB)")
            connection.execute("INSERT INTO admin VALUES (?)", (token.encode(),))
            connection.commit()
            connection.close()

            findings = BOUNDARY.scan_public_catalog_text(repo, [relative])
            self.assertEqual(len(findings), 1)
            self.assertIn("admin.password", findings[0][0])
            self.assertNotIn(token, repr(findings))

    def test_admin_password_literal_is_detected_but_placeholder_is_allowed(self) -> None:
        name = b"ADMIN_" + b"PASSWORD"
        literal = name + b"=N7p4R8t6W3x5"
        self.assertIn(
            "literal value assigned to ADMIN_PASSWORD",
            BOUNDARY.secret_reasons(literal),
        )
        self.assertEqual(
            BOUNDARY.secret_reasons(name + b"=replace-with-a-random-secret"),
            set(),
        )

    def test_historical_secret_is_found_after_removal_from_the_tip(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            self.run_git(repo, "init", "--initial-branch=main")
            self.run_git(repo, "config", "user.name", "Boundary Test")
            self.run_git(repo, "config", "user.email", "boundary@example.invalid")

            token = "sk-" + "A1b2C3d4E5f6G7h8I9j0K1l2M3n4P5q6"
            (repo / "settings.txt").write_text(
                "OPENAI_" + "API_KEY=" + token + "\n",
                encoding="utf-8",
            )
            self.run_git(repo, "add", "settings.txt")
            self.run_git(repo, "commit", "-m", "temporary secret")

            (repo / "settings.txt").write_text("safe=true\n", encoding="utf-8")
            self.run_git(repo, "add", "settings.txt")
            self.run_git(repo, "commit", "-m", "remove secret")

            self.assertEqual(
                BOUNDARY.scan_tracked_secrets(repo, BOUNDARY.tracked_paths(repo)),
                [],
            )
            findings = BOUNDARY.scan_historical_secrets(repo)
            self.assertTrue(
                any(reason == "OpenAI API key" for _, reason in findings),
                findings,
            )

    @staticmethod
    def stage_compressed_catalog(repo: Path, raw_bytes: bytes) -> list[str]:
        PublicBoundaryTests.run_git(repo, "init", "--initial-branch=main")
        digest = hashlib.sha256(raw_bytes).hexdigest()
        raw = f"public-catalog/data/catalog-{digest}.sqlite3"
        paths = ["public-catalog/catalog-manifest.json", f"{raw}.br", f"{raw}.gz"]
        (repo / "public-catalog/data").mkdir(parents=True)
        node = os.environ.get("WAAJACU_NODE", "node")
        brotli = subprocess.run(
            [node, "-e", "process.stdout.write(require('node:zlib').brotliCompressSync(require('node:fs').readFileSync(0)))"],
            input=raw_bytes,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=True,
        ).stdout
        (repo / f"{raw}.br").write_bytes(brotli)
        (repo / f"{raw}.gz").write_bytes(gzip.compress(raw_bytes))
        (repo / paths[0]).write_text(json.dumps({"database": {"path": raw.removeprefix("public-catalog/"), "sha256": f"sha256:{digest}", "bytes": len(raw_bytes)}}), encoding="utf-8")
        PublicBoundaryTests.run_git(repo, "add", "public-catalog")
        return paths

    @staticmethod
    def run_git(repo: Path, *arguments: str) -> None:
        subprocess.run(
            ["git", "-c", f"safe.directory={repo.as_posix()}", *arguments],
            cwd=repo,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )


if __name__ == "__main__":
    unittest.main()
