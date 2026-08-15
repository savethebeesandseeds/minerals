import copy
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import ima_extract as ima


ARTIFACT_SHA = "sha256:" + "a" * 64


def row(formula: str) -> ima.ExtractedRow:
    return ima.ExtractedRow(
        ordinal=1,
        page=3,
        page_row=2,
        bbox=[1.0, 2.0, 3.0, 4.0],
        canonical_name="Testite",
        formula=formula,
        raw_status="A",
        ima_number_year="2026-001",
        country="Testland",
        first_reference="Reference",
        second_reference="",
    )


def overrides(whitespace=None, transformations=None):
    return {
        "artifact_sha256": ARTIFACT_SHA,
        "format": ima.OVERRIDES_FORMAT,
        "formula_transformations": transformations or [],
        "review_policy": ima.EXPECTED_OVERRIDE_REVIEW_POLICY,
        "whitespace_resolutions": whitespace or [],
    }


def reconciliation_contract(whitespace_fields, transformations=0):
    return mock.patch.multiple(
        ima,
        EXPECTED_TOTAL_ROWS=1,
        EXPECTED_VALID_SPECIES=1,
        EXPECTED_OFFICIAL_IMA_NUMBER_COUNT=1,
        EXPECTED_STATUS_COUNTS={"A": 1},
        EXPECTED_WHITESPACE_RESOLUTION_FIELDS=whitespace_fields,
        EXPECTED_A_QUESTION_NAMES=set(),
        EXPECTED_DISCREDITED_ROWS=set(),
        EXPECTED_SOURCE_TRANSFORMATION_COUNT=transformations,
    )


class ReconciliationTests(unittest.TestCase):
    def test_exact_whitespace_override_is_consumed(self):
        left = row("Fe2O3")
        right = row("Fe 2O3")
        resolution = {
            "field": "formula",
            "ordinal": 1,
            "page": 3,
            "page_row": 2,
            "pdfplumber": left.formula,
            "pymupdf": right.formula,
            "resolved": left.formula,
        }
        foreword = {
            "release_label": "July 2026",
            "declared_valid_species": 1,
            "license_spdx": "CC-BY-SA-3.0",
        }
        with reconciliation_contract({"formula": 1}):
            rows, summary, reviewed, transformed = ima.reconcile(
                ARTIFACT_SHA,
                [left],
                [right],
                foreword,
                foreword,
                overrides([resolution]),
            )
        self.assertEqual(rows[0].formula, "Fe2O3")
        self.assertEqual(summary["reviewed_whitespace_resolution_count"], 1)
        self.assertEqual(reviewed, [resolution])
        self.assertEqual(transformed, [])

    def test_non_whitespace_engine_mismatch_fails(self):
        foreword = {
            "release_label": "July 2026",
            "declared_valid_species": 1,
            "license_spdx": "CC-BY-SA-3.0",
        }
        with reconciliation_contract({}):
            with self.assertRaisesRegex(ima.ExtractionError, "engines disagree"):
                ima.reconcile(
                    ARTIFACT_SHA,
                    [row("Fe2O3")],
                    [row("Fe2O4")],
                    foreword,
                    foreword,
                    overrides(),
                )

    def test_unused_whitespace_override_fails(self):
        same = row("Fe2O3")
        stale = {
            "field": "formula",
            "ordinal": 1,
            "page": 3,
            "page_row": 2,
            "pdfplumber": "Fe2O3",
            "pymupdf": "Fe 2O3",
            "resolved": "Fe2O3",
        }
        foreword = {
            "release_label": "July 2026",
            "declared_valid_species": 1,
            "license_spdx": "CC-BY-SA-3.0",
        }
        with reconciliation_contract({}):
            with self.assertRaisesRegex(ima.ExtractionError, "were unused"):
                ima.reconcile(
                    ARTIFACT_SHA,
                    [same],
                    [same],
                    foreword,
                    foreword,
                    overrides([stale]),
                )

    def test_unreviewed_private_use_formula_fails(self):
        unsafe = row("Fe\uf0a3O")
        foreword = {
            "release_label": "July 2026",
            "declared_valid_species": 1,
            "license_spdx": "CC-BY-SA-3.0",
        }
        with reconciliation_contract({}):
            with self.assertRaisesRegex(ima.ExtractionError, "unsafe glyph"):
                ima.reconcile(
                    ARTIFACT_SHA,
                    [unsafe],
                    [unsafe],
                    foreword,
                    foreword,
                    overrides(),
                )

    def test_replacement_character_is_never_normalized(self):
        with self.assertRaisesRegex(ima.ExtractionError, "replacement glyph"):
            ima.normalize_text("Fe\ufffdO", field="formula")


class OverrideValidationTests(unittest.TestCase):
    def test_tracked_override_set_is_valid_and_complete(self):
        path = Path(__file__).with_name("ima-2026-07-overrides.json")
        loaded = ima.load_overrides(path.read_bytes(), json.loads(path.read_text())["artifact_sha256"])
        self.assertEqual(len(loaded["whitespace_resolutions"]), 90)
        self.assertEqual(len(loaded["formula_transformations"]), 6)

    def test_formula_override_cannot_replace_an_ordinary_glyph(self):
        value = overrides(
            transformations=[
                {
                    "canonical_name": "Testite",
                    "ordinal": 1,
                    "page": 3,
                    "page_row": 2,
                    "raw_formula": "FeO",
                    "replacements": [{"count": 1, "from": "O", "to": "S"}],
                    "resolved_formula": "FeS",
                }
            ]
        )
        with self.assertRaisesRegex(ima.ExtractionError, "unsafe glyph"):
            ima.load_overrides(json.dumps(value).encode(), ARTIFACT_SHA)


class ArchiveContractTests(unittest.TestCase):
    def build_index(self, root: Path):
        artifact = root / "artifact.pdf"
        artifact.write_bytes(b"%PDF-test")
        output = root / "output"
        output.mkdir()
        counts = {"format": ima.FORMAT, "total_rows": 0}
        for relative in ima.INDEX_FILE_ROLES:
            path = output / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            if relative == "reconciled.json":
                data = {"format": ima.FORMAT, "rows": [], "summary": counts}
                path.write_text(json.dumps(data), encoding="utf-8")
            elif relative == "reconciliation.json":
                path.write_text(json.dumps(counts), encoding="utf-8")
            else:
                path.write_bytes(relative.encode())
        files = {
            relative: ima.indexed_file(output / relative, role)
            for relative, role in ima.INDEX_FILE_ROLES.items()
        }
        index = {
            "artifact": {
                "bytes": artifact.stat().st_size,
                "page_count": ima.EXPECTED_PAGE_COUNT,
                "sha256": ima.sha256_file(artifact),
            },
            "counts": counts,
            "files": files,
            "format": ima.INDEX_FORMAT,
            "policies": {},
            "reconciliation_format": ima.FORMAT,
            "runtime": {},
        }
        index_path = output / "extraction-index.json"
        index_path.write_text(json.dumps(index), encoding="utf-8")
        return artifact, index_path

    def test_index_detects_file_tampering(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifact, index_path = self.build_index(root)
            ima.verify_extraction_index(index_path, artifact)
            (index_path.parent / "engines/pdfplumber.raw.jsonl").write_bytes(b"tampered")
            with self.assertRaisesRegex(ima.ExtractionError, "indexed file changed"):
                ima.verify_extraction_index(index_path, artifact)

    def test_index_cannot_self_declare_a_smaller_file_set(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifact, index_path = self.build_index(root)
            index = json.loads(index_path.read_text(encoding="utf-8"))
            removed = "audits/source-transformations.json"
            del index["files"][removed]
            (index_path.parent / removed).unlink()
            index_path.write_text(json.dumps(index), encoding="utf-8")
            with self.assertRaisesRegex(ima.ExtractionError, "12-file contract"):
                ima.verify_extraction_index(index_path, artifact)

    def test_raw_stream_preserves_cell_text_and_bboxes(self):
        raw = ima.RawExtractedRow(
            ordinal=1,
            page=3,
            page_row=2,
            bbox=[1.0, 2.0, 3.0, 4.0],
            cell_bboxes={"formula": [1.1, 2.1, 3.1, 4.1]},
            values={"formula": "Fe  \uf0a3O"},
        )
        decoded = json.loads(ima.json_line_bytes([raw]))
        self.assertEqual(decoded["values"]["formula"], "Fe  \uf0a3O")
        self.assertEqual(decoded["cell_bboxes"]["formula"], [1.1, 2.1, 3.1, 4.1])

    def test_parser_snapshot_matches_the_executing_path(self):
        self.assertEqual(
            ima.sha256_bytes(ima.EXECUTING_PARSER_BYTES),
            ima.sha256_file(ima.EXECUTING_PARSER_PATH),
        )


if __name__ == "__main__":
    unittest.main()
