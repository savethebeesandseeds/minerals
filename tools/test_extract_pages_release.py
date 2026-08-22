from __future__ import annotations

import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest import mock


MODULE_PATH = Path(__file__).with_name("extract-pages-release.py")
SPEC = importlib.util.spec_from_file_location("extract_pages_release", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"could not load {MODULE_PATH}")
EXTRACTOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EXTRACTOR)


def add_file(archive: tarfile.TarFile, name: str, contents: bytes = b"x") -> None:
    member = tarfile.TarInfo(name)
    member.size = len(contents)
    archive.addfile(member, io.BytesIO(contents))


def add_required_files(archive: tarfile.TarFile) -> None:
    add_file(archive, "index.html", b"<!doctype html>")
    add_file(archive, "catalog-manifest.json", b"{}")


class ExtractPagesReleaseTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.archive_path = self.root / "release.tar.gz"
        self.output_path = self.root / "release"

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def assert_rejected(self, message: str) -> None:
        with self.assertRaisesRegex(SystemExit, message):
            EXTRACTOR.extract_release(self.archive_path, self.output_path)
        self.assertFalse(self.output_path.exists())

    def test_extracts_regular_rooted_release(self) -> None:
        with tarfile.open(self.archive_path, "w:gz") as archive:
            add_required_files(archive)
            add_file(archive, "data/catalog.sqlite3", b"SQLite format 3\0")

        EXTRACTOR.extract_release(self.archive_path, self.output_path)

        self.assertEqual(
            (self.output_path / "index.html").read_bytes(), b"<!doctype html>"
        )
        self.assertEqual(
            (self.output_path / "data/catalog.sqlite3").read_bytes(),
            b"SQLite format 3\0",
        )

    def test_rejects_parent_traversal(self) -> None:
        with tarfile.open(self.archive_path, "w:gz") as archive:
            add_required_files(archive)
            add_file(archive, "../escape")

        self.assert_rejected("unsafe path")
        self.assertFalse((self.root / "escape").exists())

    def test_rejects_symbolic_and_hard_links(self) -> None:
        for entry_type in (tarfile.SYMTYPE, tarfile.LNKTYPE):
            with self.subTest(entry_type=entry_type):
                with tarfile.open(self.archive_path, "w:gz") as archive:
                    add_required_files(archive)
                    member = tarfile.TarInfo("data/catalog.sqlite3")
                    member.type = entry_type
                    member.linkname = "../../private/minerals.db"
                    archive.addfile(member)

                self.assert_rejected("link or special entry")
                self.archive_path.unlink()

    def test_rejects_duplicate_paths(self) -> None:
        with tarfile.open(self.archive_path, "w:gz") as archive:
            add_required_files(archive)
            add_file(archive, "index.html", b"replacement")

        self.assert_rejected("duplicate entry")

    def test_rejects_extracted_size_over_limit(self) -> None:
        with tarfile.open(self.archive_path, "w:gz") as archive:
            add_required_files(archive)

        with mock.patch.object(EXTRACTOR, "MAX_EXTRACTED_BYTES", 1):
            self.assert_rejected("expands beyond")


if __name__ == "__main__":
    unittest.main()
