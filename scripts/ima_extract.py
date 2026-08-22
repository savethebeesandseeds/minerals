#!/usr/bin/env python3
"""Extract and reconcile an official IMA-CNMNC master-list PDF.

This tool deliberately runs two independent PDF engines. A release is emitted
only when both engines agree on every source column for every row. The output
is an archival intermediate; it is not itself accepted by the public server.
The Rust release builder performs the typed ingestion validation and canonical
hashing in a separate step.
"""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import importlib.metadata
import io
import json
import platform
import re
import sys
import unicodedata
from collections import Counter
from dataclasses import asdict, dataclass, replace
from pathlib import Path
from typing import Any, Iterable, Sequence

# Capture the parser bytes before importing either heavyweight PDF engine.
# The completed extraction also verifies that this path stayed unchanged.
EXECUTING_PARSER_PATH = Path(__file__).resolve(strict=True)
EXECUTING_PARSER_BYTES = EXECUTING_PARSER_PATH.read_bytes()

import pdfplumber
import pymupdf


FORMAT = "waajacu-ima-reconciliation-v2"
INDEX_FORMAT = "waajacu-ima-extraction-index-v1"
OVERRIDES_FORMAT = "waajacu-ima-extraction-overrides-v1"
EXPECTED_PYTHON_IMPLEMENTATION = "CPython"
EXPECTED_PYTHON_VERSION = "3.12.13"
EXPECTED_PAGE_COUNT = 243
EXPECTED_TABLE_PAGE_COUNT = 241
EXPECTED_VALID_SPECIES = 6_226
EXPECTED_TOTAL_ROWS = 6_227
EXPECTED_OFFICIAL_IMA_NUMBER_COUNT = 4_127
EXPECTED_STATUS_COUNTS = {
    "A": 4_293,
    "A ?": 6,
    "D": 1,
    "G": 1_129,
    "Q": 96,
    "Rd": 413,
    "Rn": 289,
}
EXPECTED_WHITESPACE_RESOLUTION_FIELDS = {
    "canonical_name": 20,
    "first_reference": 6,
    "formula": 62,
    "second_reference": 2,
}
EXPECTED_A_QUESTION_NAMES = {
    "Balipholite",
    "Calcjarlite",
    "Changbaiite",
    "Chelkarite",
    "Cuprostibite",
    "Daomanite",
}
EXPECTED_DISCREDITED_ROWS = {("Franklinphilite", "1990-050")}
EXPECTED_SOURCE_TRANSFORMATION_COUNT = 6
NORMALIZATION_POLICY = "nfc-collapse-whitespace-v2"
EXPECTED_OVERRIDE_REVIEW_POLICY = "artifact-bound-rendered-source-review-v1"
SOURCE_COLUMNS = (
    "canonical_name",
    "formula",
    "raw_status",
    "ima_number_year",
    "country",
    "first_reference",
    "second_reference",
)
# Six source rows carry the literal composite status ``A ?``. Geometry checks
# confirm that the question mark is inside the status cell, not spillover from
# the adjacent IMA-number/year cell. It is valid for the source-declared count,
# but the downstream adapter maps it to an explicit ``uncertain`` state rather
# than silently erasing the qualifier.
VALID_SOURCE_STATUSES = frozenset({"A", "A ?", "G", "Rd", "Rn", "Q"})
HIDDEN_SOURCE_STATUSES = frozenset({"D"})
SUPPORTED_SOURCE_STATUSES = VALID_SOURCE_STATUSES | HIDDEN_SOURCE_STATUSES
IMA_NUMBER_PATTERN = re.compile(r"^[0-9]{4}-[0-9]{3}[a-z]?$", re.IGNORECASE)
TABLE_SETTINGS = {
    "vertical_strategy": "lines",
    "horizontal_strategy": "lines",
}
INDEX_FILE_ROLES = {
    "audits/reviewed-whitespace-resolutions.json": "reviewed-whitespace-resolutions",
    "audits/source-transformations.json": "reviewed-source-transformations",
    "engines/pdfplumber.normalized.jsonl": "engine-normalized",
    "engines/pdfplumber.raw.jsonl": "engine-raw",
    "engines/pymupdf.normalized.jsonl": "engine-normalized",
    "engines/pymupdf.raw.jsonl": "engine-raw",
    "inputs/overrides.json": "reviewed-overrides",
    "inputs/source-metadata.json": "source-metadata",
    "parser/ima-requirements.txt": "parser-requirements",
    "parser/ima_extract.py": "parser-source",
    "reconciled.json": "reconciled-records",
    "reconciliation.json": "reconciliation-summary",
}



class ExtractionError(RuntimeError):
    pass


@dataclass(frozen=True)
class ExtractedRow:
    ordinal: int
    page: int
    page_row: int
    bbox: list[float]
    canonical_name: str
    formula: str
    raw_status: str
    ima_number_year: str
    country: str
    first_reference: str
    second_reference: str


@dataclass(frozen=True)
class RawExtractedRow:
    ordinal: int
    page: int
    page_row: int
    bbox: list[float]
    cell_bboxes: dict[str, list[float]]
    values: dict[str, str]


def sha256_bytes(data: bytes) -> str:
    return f"sha256:{hashlib.sha256(data).hexdigest()}"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while block := stream.read(1024 * 1024):
            digest.update(block)
    return f"sha256:{digest.hexdigest()}"


def normalize_text(text: str, *, field: str) -> str:
    text = unicodedata.normalize("NFC", text)
    if "\ufffd" in text:
        raise ExtractionError(f"unmapped replacement glyph in field {field!r}: {text!r}")
    text = re.sub(r"\s+", " ", text).strip()
    if any(
        unicodedata.category(character) in {"Cc", "Cf", "Cs"}
        for character in text
    ):
        raise ExtractionError(f"control, format, or surrogate character in field {field!r}")
    return text


def row_from_values(
    *,
    ordinal: int,
    page: int,
    page_row: int,
    bbox: Sequence[float],
    values: Sequence[str],
) -> ExtractedRow:
    if len(values) != len(SOURCE_COLUMNS):
        raise ExtractionError(
            f"page {page} row {page_row} has {len(values)} columns, expected 7"
        )
    normalized = {
        field: normalize_text(value, field=field)
        for field, value in zip(SOURCE_COLUMNS, values, strict=True)
    }
    return ExtractedRow(
        ordinal=ordinal,
        page=page,
        page_row=page_row,
        bbox=[round(float(value), 3) for value in bbox],
        **normalized,
    )


def pdfplumber_cell_text(
    page_characters: Sequence[dict[str, Any]],
    bbox: Sequence[float],
    field: str,
) -> str:
    # ``Page.crop(...).chars`` retains glyphs that merely touch a boundary.
    # On this ruled PDF that can pull a header or the preceding wrapped line
    # into the next cell. Assign each glyph by its center point instead.
    left, top, right, bottom = bbox
    characters = [
        character["text"]
        for character in page_characters
        if left <= (character["x0"] + character["x1"]) / 2 < right
        and top <= (character["top"] + character["bottom"]) / 2 < bottom
    ]
    return "".join(characters)


def extract_with_pdfplumber(
    pdf_path: Path,
) -> tuple[list[ExtractedRow], list[RawExtractedRow], dict[str, Any]]:
    rows: list[ExtractedRow] = []
    raw_rows: list[RawExtractedRow] = []
    with pdfplumber.open(pdf_path) as document:
        if len(document.pages) != EXPECTED_PAGE_COUNT:
            raise ExtractionError(
                f"pdfplumber saw {len(document.pages)} pages, expected {EXPECTED_PAGE_COUNT}"
            )
        foreword = "\n".join(
            (document.pages[index].extract_text() or "") for index in (0, 1)
        )
        for page_index, page in enumerate(document.pages[2:], start=3):
            tables = page.find_tables(TABLE_SETTINGS)
            if len(tables) != 1:
                raise ExtractionError(
                    f"pdfplumber page {page_index} has {len(tables)} tables, expected 1"
                )
            table = tables[0]
            page_characters = page.chars
            for raw_page_row, table_row in enumerate(table.rows, start=1):
                cells = table_row.cells
                if len(cells) != 7 or any(cell is None for cell in cells):
                    raise ExtractionError(
                        f"pdfplumber page {page_index} row {raw_page_row} has broken geometry"
                    )
                raw_values = [
                    pdfplumber_cell_text(page_characters, cell, field)
                    for cell, field in zip(cells, SOURCE_COLUMNS, strict=True)
                ]
                candidate = row_from_values(
                    ordinal=len(rows) + 1,
                    page=page_index,
                    page_row=raw_page_row,
                    bbox=table_row.bbox,
                    values=raw_values,
                )
                if candidate.canonical_name == "Name":
                    if page_index != 3 or raw_page_row != 1:
                        raise ExtractionError(
                            f"unexpected table header on page {page_index} row {raw_page_row}"
                        )
                    continue
                ordinal = len(rows) + 1
                if candidate.ordinal != ordinal:
                    raise ExtractionError("pdfplumber ordinal drift")
                rows.append(candidate)
                raw_rows.append(
                    RawExtractedRow(
                        ordinal=ordinal,
                        page=page_index,
                        page_row=raw_page_row,
                        bbox=[round(float(value), 3) for value in table_row.bbox],
                        cell_bboxes={
                            field: [round(float(value), 3) for value in cell]
                            for field, cell in zip(SOURCE_COLUMNS, cells, strict=True)
                        },
                        values=dict(zip(SOURCE_COLUMNS, raw_values, strict=True)),
                    )
                )
    return rows, raw_rows, parse_foreword(foreword)


def pymupdf_page_characters(page: Any) -> list[tuple[str, Sequence[float]]]:
    """Return positioned glyphs once, preserving PDF content-stream order.

    Calling ``get_text`` separately for all 43,000+ cells is needlessly slow
    and memory-heavy. Filtering one page-level raw stream by cell geometry is
    equivalent while keeping script glyphs in their source order.
    """

    raw = page.get_text("rawdict", sort=False)
    characters: list[tuple[str, Sequence[float]]] = []
    for block in raw.get("blocks", []):
        for line in block.get("lines", []):
            for span in line.get("spans", []):
                characters.extend(
                    (character["c"], character["bbox"])
                    for character in span.get("chars", [])
                )
    return characters


def pymupdf_cell_text(
    page_characters: Sequence[tuple[str, Sequence[float]]],
    bbox: Sequence[float],
    field: str,
) -> str:
    left, top, right, bottom = bbox
    characters = [
        value
        for value, character_bbox in page_characters
        if left <= (character_bbox[0] + character_bbox[2]) / 2 < right
        and top <= (character_bbox[1] + character_bbox[3]) / 2 < bottom
    ]
    return "".join(characters)


def extract_with_pymupdf(
    pdf_path: Path,
) -> tuple[list[ExtractedRow], list[RawExtractedRow], dict[str, Any]]:
    rows: list[ExtractedRow] = []
    raw_rows: list[RawExtractedRow] = []
    with pymupdf.open(pdf_path) as document:
        if document.page_count != EXPECTED_PAGE_COUNT:
            raise ExtractionError(
                f"PyMuPDF saw {document.page_count} pages, expected {EXPECTED_PAGE_COUNT}"
            )
        foreword = "\n".join(document[index].get_text("text") for index in (0, 1))
        for page_number in range(3, document.page_count + 1):
            page = document[page_number - 1]
            # PyMuPDF 1.28.2 emits an advisory on stdout from this API. Keep
            # stdout machine-readable; the exact runtime is in the index.
            with contextlib.redirect_stdout(io.StringIO()):
                found = page.find_tables(strategy="lines")
            if len(found.tables) != 1:
                raise ExtractionError(
                    f"PyMuPDF page {page_number} has {len(found.tables)} tables, expected 1"
                )
            table = found.tables[0]
            if table.col_count != 7:
                raise ExtractionError(
                    f"PyMuPDF page {page_number} has {table.col_count} columns, expected 7"
                )
            page_characters = pymupdf_page_characters(page)
            for raw_page_row, table_row in enumerate(table.rows, start=1):
                cells = table_row.cells
                if len(cells) != 7 or any(cell is None for cell in cells):
                    # PyMuPDF extends the final page's ruled table through the
                    # release-note footer, producing two merged pseudo-rows.
                    # pdfplumber correctly stops at the last mineral row. Keep
                    # this exception artifact-specific and fail on every other
                    # incomplete row.
                    footer = page.get_text(
                        "text", clip=pymupdf.Rect(table_row.bbox), sort=False
                    )
                    if (
                        page_number == EXPECTED_PAGE_COUNT
                        and raw_page_row in (18, 19)
                        and (
                            not footer.strip()
                            or "preceding release (May 2026)" in footer
                            or footer.strip() in {"(", ")", "(\n)"}
                        )
                    ):
                        continue
                    raise ExtractionError(
                        f"PyMuPDF page {page_number} row {raw_page_row} has broken geometry"
                    )
                raw_values = [
                    pymupdf_cell_text(page_characters, cell, field)
                    for cell, field in zip(cells, SOURCE_COLUMNS, strict=True)
                ]
                candidate = row_from_values(
                    ordinal=len(rows) + 1,
                    page=page_number,
                    page_row=raw_page_row,
                    bbox=table_row.bbox,
                    values=raw_values,
                )
                if candidate.canonical_name == "Name":
                    if page_number != 3 or raw_page_row != 1:
                        raise ExtractionError(
                            f"unexpected table header on page {page_number} row {raw_page_row}"
                        )
                    continue
                ordinal = len(rows) + 1
                if candidate.ordinal != ordinal:
                    raise ExtractionError("PyMuPDF ordinal drift")
                rows.append(candidate)
                raw_rows.append(
                    RawExtractedRow(
                        ordinal=ordinal,
                        page=page_number,
                        page_row=raw_page_row,
                        bbox=[round(float(value), 3) for value in table_row.bbox],
                        cell_bboxes={
                            field: [round(float(value), 3) for value in cell]
                            for field, cell in zip(SOURCE_COLUMNS, cells, strict=True)
                        },
                        values=dict(zip(SOURCE_COLUMNS, raw_values, strict=True)),
                    )
                )
    return rows, raw_rows, parse_foreword(foreword)


def parse_foreword(text: str) -> dict[str, Any]:
    normalized = unicodedata.normalize("NFC", text)
    release_match = re.search(r"Updated:\s*(July\s+2026)", normalized, re.IGNORECASE)
    count_match = re.search(r"([0-9]{4})\s+currently valid species", normalized)
    if not release_match or not count_match:
        raise ExtractionError("could not recover release label and declared count from foreword")
    if "Creative Commons Attribution-ShareAlike 3.0 Unported License" not in normalized:
        raise ExtractionError("expected CC BY-SA 3.0 license statement is absent")
    return {
        "release_label": release_match.group(1),
        "declared_valid_species": int(count_match.group(1)),
        "license_spdx": "CC-BY-SA-3.0",
    }


def require_exact_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    actual = set(value)
    if actual != expected:
        raise ExtractionError(
            f"{label} keys differ: missing={sorted(expected - actual)!r}, "
            f"unknown={sorted(actual - expected)!r}"
        )


def load_overrides(data: bytes, artifact_sha256: str) -> dict[str, Any]:
    try:
        value = json.loads(data.decode("utf-8"))
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        raise ExtractionError(f"invalid override JSON: {error}") from error
    if not isinstance(value, dict):
        raise ExtractionError("override root must be an object")
    require_exact_keys(
        value,
        {
            "artifact_sha256",
            "format",
            "formula_transformations",
            "review_policy",
            "whitespace_resolutions",
        },
        "override root",
    )
    if value["format"] != OVERRIDES_FORMAT:
        raise ExtractionError("unsupported override format")
    if value["artifact_sha256"] != artifact_sha256:
        raise ExtractionError("override artifact hash does not match the source")
    if value["review_policy"] != EXPECTED_OVERRIDE_REVIEW_POLICY:
        raise ExtractionError("unsupported override review policy")
    whitespace = value["whitespace_resolutions"]
    transformations = value["formula_transformations"]
    if not isinstance(whitespace, list) or not isinstance(transformations, list):
        raise ExtractionError("override resolution collections must be arrays")

    seen_whitespace: set[tuple[int, str]] = set()
    for index, entry in enumerate(whitespace):
        if not isinstance(entry, dict):
            raise ExtractionError(f"whitespace override {index} must be an object")
        require_exact_keys(
            entry,
            {
                "field",
                "ordinal",
                "page",
                "page_row",
                "pdfplumber",
                "pymupdf",
                "resolved",
            },
            f"whitespace override {index}",
        )
        key = (entry["ordinal"], entry["field"])
        if (
            not isinstance(entry["ordinal"], int)
            or entry["ordinal"] < 1
            or entry["field"] not in SOURCE_COLUMNS
            or key in seen_whitespace
        ):
            raise ExtractionError(f"invalid or duplicate whitespace override {key!r}")
        if (
            not isinstance(entry["page"], int)
            or entry["page"] < 3
            or not isinstance(entry["page_row"], int)
            or entry["page_row"] < 1
        ):
            raise ExtractionError(f"invalid whitespace override locator {key!r}")
        for field in ("pdfplumber", "pymupdf", "resolved"):
            if not isinstance(entry[field], str):
                raise ExtractionError(f"whitespace override {key!r} has non-text {field}")
            if normalize_text(entry[field], field=entry["field"]) != entry[field]:
                raise ExtractionError(
                    f"whitespace override {key!r} {field} is not normalized"
                )
        if entry["pdfplumber"] == entry["pymupdf"]:
            raise ExtractionError(f"override {key!r} does not resolve a disagreement")
        if "".join(entry["pdfplumber"].split()) != "".join(
            entry["pymupdf"].split()
        ):
            raise ExtractionError(f"override {key!r} is not whitespace-only")
        if "".join(entry["resolved"].split()) != "".join(
            entry["pdfplumber"].split()
        ):
            raise ExtractionError(f"override {key!r} changes non-whitespace glyphs")
        seen_whitespace.add(key)

    seen_transformations: set[int] = set()
    for index, entry in enumerate(transformations):
        if not isinstance(entry, dict):
            raise ExtractionError(f"formula transformation {index} must be an object")
        require_exact_keys(
            entry,
            {
                "canonical_name",
                "ordinal",
                "page",
                "page_row",
                "raw_formula",
                "replacements",
                "resolved_formula",
            },
            f"formula transformation {index}",
        )
        ordinal = entry["ordinal"]
        if not isinstance(ordinal, int) or ordinal < 1 or ordinal in seen_transformations:
            raise ExtractionError(f"invalid or duplicate formula transformation {ordinal!r}")
        if (
            not isinstance(entry["page"], int)
            or entry["page"] < 3
            or not isinstance(entry["page_row"], int)
            or entry["page_row"] < 1
            or not isinstance(entry["canonical_name"], str)
            or not entry["canonical_name"]
            or not isinstance(entry["raw_formula"], str)
            or not isinstance(entry["resolved_formula"], str)
        ):
            raise ExtractionError(f"formula transformation {ordinal} has invalid fields")
        if normalize_text(entry["raw_formula"], field="formula") != entry["raw_formula"]:
            raise ExtractionError(f"formula transformation {ordinal} raw formula is not normalized")
        if (
            normalize_text(entry["resolved_formula"], field="formula")
            != entry["resolved_formula"]
        ):
            raise ExtractionError(
                f"formula transformation {ordinal} resolved formula is not normalized"
            )
        if not formula_has_private_use_or_cyrillic(entry["raw_formula"]):
            raise ExtractionError(
                f"formula transformation {ordinal} does not resolve a reviewed unsafe glyph"
            )
        if formula_has_private_use_or_cyrillic(entry["resolved_formula"]):
            raise ExtractionError(
                f"formula transformation {ordinal} leaves an unsafe glyph"
            )
        if not isinstance(entry["replacements"], list) or not entry["replacements"]:
            raise ExtractionError(f"formula transformation {ordinal} has no replacements")
        transformed_formula = entry["raw_formula"]
        for replacement_index, replacement in enumerate(entry["replacements"]):
            if not isinstance(replacement, dict):
                raise ExtractionError(
                    f"formula transformation {ordinal} replacement {replacement_index} is invalid"
                )
            require_exact_keys(
                replacement,
                {"count", "from", "to"},
                f"formula transformation {ordinal} replacement {replacement_index}",
            )
            if (
                not isinstance(replacement["count"], int)
                or replacement["count"] < 1
                or not isinstance(replacement["from"], str)
                or len(replacement["from"]) != 1
                or not isinstance(replacement["to"], str)
                or len(replacement["to"]) != 1
            ):
                raise ExtractionError(
                    f"formula transformation {ordinal} has an invalid replacement"
                )
            if not formula_has_private_use_or_cyrillic(replacement["from"]):
                raise ExtractionError(
                    f"formula transformation {ordinal} replaces an ordinary source glyph"
                )
            if (
                "\ufffd" in replacement["to"]
                or formula_has_private_use_or_cyrillic(replacement["to"])
            ):
                raise ExtractionError(
                    f"formula transformation {ordinal} replacement remains unsafe"
                )
            if transformed_formula.count(replacement["from"]) != replacement["count"]:
                raise ExtractionError(
                    f"formula transformation {ordinal} replacement count is stale"
                )
            transformed_formula = transformed_formula.replace(
                replacement["from"], replacement["to"]
            )
        if normalize_text(transformed_formula, field="formula") != entry["resolved_formula"]:
            raise ExtractionError(
                f"formula transformation {ordinal} operations do not produce the resolution"
            )
        seen_transformations.add(ordinal)
    return value


def formula_has_private_use_or_cyrillic(value: str) -> bool:
    return any(
        unicodedata.category(character) == "Co"
        or unicodedata.name(character, "").startswith("CYRILLIC ")
        for character in value
    )


def reconcile(
    artifact_sha256: str,
    pdfplumber_rows: Sequence[ExtractedRow],
    pymupdf_rows: Sequence[ExtractedRow],
    pdfplumber_foreword: dict[str, Any],
    pymupdf_foreword: dict[str, Any],
    overrides: dict[str, Any],
) -> tuple[
    list[ExtractedRow],
    dict[str, Any],
    list[dict[str, Any]],
    list[dict[str, Any]],
]:
    if pdfplumber_foreword != pymupdf_foreword:
        raise ExtractionError(
            f"foreword disagreement: {pdfplumber_foreword!r} != {pymupdf_foreword!r}"
        )
    if pdfplumber_foreword["declared_valid_species"] != EXPECTED_VALID_SPECIES:
        raise ExtractionError(
            "source-declared valid-species count changed; update and review the adapter contract"
        )
    if len(pdfplumber_rows) != len(pymupdf_rows):
        raise ExtractionError(
            f"extractor row-count disagreement: {len(pdfplumber_rows)} != {len(pymupdf_rows)}"
        )
    if len(pdfplumber_rows) != EXPECTED_TOTAL_ROWS:
        raise ExtractionError(
            f"source contains {len(pdfplumber_rows)} rows, expected {EXPECTED_TOTAL_ROWS}"
        )
    disagreements: list[dict[str, Any]] = []
    reviewed_whitespace: list[dict[str, Any]] = []
    resolved_rows: list[ExtractedRow] = []
    whitespace_by_key = {
        (entry["ordinal"], entry["field"]): entry
        for entry in overrides["whitespace_resolutions"]
    }
    consumed_whitespace: set[tuple[int, str]] = set()
    for left, right in zip(pdfplumber_rows, pymupdf_rows, strict=True):
        differing_fields: list[str] = []
        resolved_fields: dict[str, str] = {}
        for field in SOURCE_COLUMNS:
            left_value = getattr(left, field)
            right_value = getattr(right, field)
            if left_value == right_value:
                continue
            key = (left.ordinal, field)
            resolution = whitespace_by_key.get(key)
            if resolution is None:
                differing_fields.append(field)
                continue
            if (
                resolution["page"] != left.page
                or resolution["page_row"] != left.page_row
                or resolution["pdfplumber"] != left_value
                or resolution["pymupdf"] != right_value
            ):
                raise ExtractionError(
                    f"reviewed whitespace resolution no longer matches row {left.ordinal} {field}"
                )
            resolved_fields[field] = normalize_text(
                resolution["resolved"], field=field
            )
            consumed_whitespace.add(key)
            reviewed_whitespace.append(dict(resolution))
        if left.page != right.page or left.page_row != right.page_row:
            differing_fields.append("locator")
        if differing_fields:
            disagreements.append(
                {
                    "ordinal": left.ordinal,
                    "fields": differing_fields,
                    "pdfplumber": asdict(left),
                    "pymupdf": asdict(right),
                }
            )
        else:
            resolved_rows.append(replace(left, **resolved_fields))
    if disagreements:
        sample = json.dumps(disagreements[:3], ensure_ascii=False, sort_keys=True)
        raise ExtractionError(
            f"the two PDF engines disagree on {len(disagreements)} rows; sample={sample}"
        )
    unused_whitespace = sorted(set(whitespace_by_key) - consumed_whitespace)
    if unused_whitespace:
        raise ExtractionError(
            f"{len(unused_whitespace)} reviewed whitespace resolutions were unused: "
            f"{unused_whitespace[:10]!r}"
        )

    transformations_by_ordinal = {
        entry["ordinal"]: entry for entry in overrides["formula_transformations"]
    }
    consumed_transformations: set[int] = set()
    reviewed_transformations: list[dict[str, Any]] = []
    transformed_rows: list[ExtractedRow] = []
    for row in resolved_rows:
        transformation = transformations_by_ordinal.get(row.ordinal)
        if transformation is None:
            transformed = row
        else:
            if (
                transformation["page"] != row.page
                or transformation["page_row"] != row.page_row
                or transformation["canonical_name"] != row.canonical_name
                or transformation["raw_formula"] != row.formula
            ):
                raise ExtractionError(
                    f"formula transformation no longer matches row {row.ordinal}"
                )
            formula = row.formula
            for operation in transformation["replacements"]:
                if formula.count(operation["from"]) != operation["count"]:
                    raise ExtractionError(
                        f"formula transformation count changed for row {row.ordinal}"
                    )
                formula = formula.replace(operation["from"], operation["to"])
            formula = normalize_text(formula, field="formula")
            if formula != transformation["resolved_formula"]:
                raise ExtractionError(
                    f"formula transformation result changed for row {row.ordinal}"
                )
            transformed = replace(row, formula=formula)
            consumed_transformations.add(row.ordinal)
            reviewed_transformations.append(dict(transformation))
        if "\ufffd" in transformed.formula or formula_has_private_use_or_cyrillic(
            transformed.formula
        ):
            raise ExtractionError(
                f"formula for row {row.ordinal} retains an unsafe glyph after review"
            )
        transformed_rows.append(transformed)
    unused_transformations = sorted(
        set(transformations_by_ordinal) - consumed_transformations
    )
    if unused_transformations:
        raise ExtractionError(
            f"reviewed formula transformations were unused: {unused_transformations!r}"
        )

    names = [row.canonical_name for row in transformed_rows]
    if any(not name for name in names):
        raise ExtractionError("the source contains an empty mineral name")
    normalized_names = [unicodedata.normalize("NFC", name).casefold() for name in names]
    duplicates = sorted(
        name for name, count in Counter(normalized_names).items() if count > 1
    )
    if duplicates:
        raise ExtractionError(f"duplicate canonical mineral names: {duplicates[:10]!r}")

    status_counts = Counter(row.raw_status for row in transformed_rows)
    unknown_statuses = sorted(set(status_counts) - SUPPORTED_SOURCE_STATUSES)
    if unknown_statuses:
        raise ExtractionError(f"unsupported source statuses: {unknown_statuses!r}")
    if dict(status_counts) != EXPECTED_STATUS_COUNTS:
        raise ExtractionError(
            f"source status counts changed: {dict(sorted(status_counts.items()))!r}"
        )
    a_question_names = {
        row.canonical_name for row in transformed_rows if row.raw_status == "A ?"
    }
    if a_question_names != EXPECTED_A_QUESTION_NAMES:
        raise ExtractionError(
            f"ambiguous A ? source rows changed: {sorted(a_question_names)!r}"
        )
    discredited_rows = {
        (row.canonical_name, row.ima_number_year)
        for row in transformed_rows
        if row.raw_status == "D"
    }
    if discredited_rows != EXPECTED_DISCREDITED_ROWS:
        raise ExtractionError(
            f"discredited source rows changed: {sorted(discredited_rows)!r}"
        )
    valid_count = sum(status_counts[status] for status in VALID_SOURCE_STATUSES)
    if valid_count != EXPECTED_VALID_SPECIES:
        raise ExtractionError(
            f"extracted {valid_count} valid species, expected {EXPECTED_VALID_SPECIES}"
        )

    official_numbers = [
        row.ima_number_year
        for row in transformed_rows
        if IMA_NUMBER_PATTERN.fullmatch(row.ima_number_year)
    ]
    duplicate_numbers = sorted(
        value for value, count in Counter(official_numbers).items() if count > 1
    )
    if duplicate_numbers:
        raise ExtractionError(f"duplicate official IMA numbers: {duplicate_numbers[:10]!r}")
    if len(official_numbers) != EXPECTED_OFFICIAL_IMA_NUMBER_COUNT:
        raise ExtractionError(
            f"official IMA number count changed: {len(official_numbers)}"
        )

    missing_formula_count = sum(not row.formula for row in transformed_rows)
    if missing_formula_count:
        raise ExtractionError("the source contains an empty mineral formula")
    if any(not row.ima_number_year for row in transformed_rows):
        raise ExtractionError("the source contains an empty IMA number/year cell")
    reviewed_whitespace_fields = dict(
        sorted(Counter(event["field"] for event in reviewed_whitespace).items())
    )
    if reviewed_whitespace_fields != EXPECTED_WHITESPACE_RESOLUTION_FIELDS:
        raise ExtractionError(
            f"reviewed whitespace set changed: {reviewed_whitespace_fields!r}"
        )
    if len(reviewed_transformations) != EXPECTED_SOURCE_TRANSFORMATION_COUNT:
        raise ExtractionError(
            f"reviewed source transformation count changed: {len(reviewed_transformations)}"
        )
    summary = {
        "format": FORMAT,
        "artifact_sha256": artifact_sha256,
        "page_count": EXPECTED_PAGE_COUNT,
        "table_page_count": EXPECTED_TABLE_PAGE_COUNT,
        "release_label": pdfplumber_foreword["release_label"],
        "license_spdx": pdfplumber_foreword["license_spdx"],
        "declared_valid_species": pdfplumber_foreword["declared_valid_species"],
        "total_rows": len(transformed_rows),
        "valid_species": valid_count,
        "hidden_historical_rows": len(transformed_rows) - valid_count,
        "status_counts": dict(sorted(status_counts.items())),
        "official_ima_number_count": len(official_numbers),
        "missing_formula_count": missing_formula_count,
        "extractor_disagreement_count": 0,
        "reviewed_whitespace_resolution_count": len(reviewed_whitespace),
        "reviewed_whitespace_resolution_fields": reviewed_whitespace_fields,
        "extractor_versions": {
            "pdfplumber": pdfplumber.__version__,
            "pymupdf": pymupdf.__version__,
            "python": platform.python_version(),
        },
        "formula_replacement_glyph_count": 0,
        "formula_private_use_count": 0,
        "formula_cyrillic_count": 0,
        "normalization_policy": NORMALIZATION_POLICY,
        "override_review_policy": overrides["review_policy"],
        "source_transformation_count": len(reviewed_transformations),
        "source_transformation_fields": {"formula": len(reviewed_transformations)},
    }
    return transformed_rows, summary, reviewed_whitespace, reviewed_transformations


def json_line_bytes(rows: Iterable[Any]) -> bytes:
    return b"".join(
        (
            json.dumps(asdict(row), ensure_ascii=False, sort_keys=True, separators=(",", ":"))
            + "\n"
        ).encode("utf-8")
        for row in rows
    )


def write_new_file(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with path.open("xb") as stream:
            stream.write(data)
            stream.flush()
    except FileExistsError as error:
        raise ExtractionError(f"refusing to overwrite existing output: {path}") from error


def validate_source_metadata(
    data: bytes, artifact_sha256: str, artifact_bytes: int
) -> dict[str, Any]:
    try:
        value = json.loads(data.decode("utf-8"))
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        raise ExtractionError(f"invalid source metadata JSON: {error}") from error
    if not isinstance(value, dict):
        raise ExtractionError("source metadata root must be an object")
    require_exact_keys(
        value,
        {
            "artifact",
            "attribution",
            "dataset_key",
            "format",
            "landing_page",
            "retrieved_at",
            "source_key",
        },
        "source metadata root",
    )
    if value["format"] != "waajacu-source-artifact-v1":
        raise ExtractionError("unsupported source metadata format")
    if value["dataset_key"] != "ima.cnmnc.master_list" or value["source_key"] != "ima.cnmnc":
        raise ExtractionError("source metadata identifies the wrong dataset")
    artifact = value["artifact"]
    attribution = value["attribution"]
    if not isinstance(artifact, dict) or not isinstance(attribution, dict):
        raise ExtractionError("source metadata artifact/attribution must be objects")
    require_exact_keys(
        artifact,
        {"bytes", "content_type", "etag", "last_modified", "sha256", "url"},
        "source metadata artifact",
    )
    require_exact_keys(
        attribution,
        {
            "changes_notice",
            "creator",
            "derived_output_license_spdx",
            "license_spdx",
            "license_url",
            "no_endorsement_notice",
            "source_title",
        },
        "source metadata attribution",
    )
    if artifact["sha256"] != artifact_sha256 or artifact["bytes"] != artifact_bytes:
        raise ExtractionError("source metadata artifact digest/size mismatch")
    if artifact["content_type"] != "application/pdf":
        raise ExtractionError("source metadata content type is not application/pdf")
    if attribution["license_spdx"] != "CC-BY-SA-3.0" or attribution[
        "derived_output_license_spdx"
    ] != "CC-BY-SA-3.0":
        raise ExtractionError("source metadata has an unexpected data license")
    for label in (
        "creator",
        "source_title",
        "license_url",
        "changes_notice",
        "no_endorsement_notice",
    ):
        if not isinstance(attribution[label], str) or not attribution[label].strip():
            raise ExtractionError(f"source metadata attribution.{label} is empty")
    return value


def validate_python_runtime(
    implementation: str | None = None, version: str | None = None
) -> None:
    actual_implementation = (
        platform.python_implementation() if implementation is None else implementation
    )
    actual_version = platform.python_version() if version is None else version
    if (
        actual_implementation != EXPECTED_PYTHON_IMPLEMENTATION
        or actual_version != EXPECTED_PYTHON_VERSION
    ):
        raise ExtractionError(
            "extractor requires "
            f"{EXPECTED_PYTHON_IMPLEMENTATION} {EXPECTED_PYTHON_VERSION}; "
            f"running {actual_implementation} {actual_version}"
        )


def pinned_runtime(requirements_bytes: bytes) -> dict[str, Any]:
    # Check the interpreter before consulting or invoking either extraction
    # engine. Package pins alone do not capture Python ABI/runtime behavior.
    validate_python_runtime()
    try:
        requirements_text = requirements_bytes.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ExtractionError(f"requirements are not UTF-8: {error}") from error
    packages: dict[str, str] = {}
    for line_number, raw_line in enumerate(
        requirements_text.splitlines(), start=1
    ):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.count("==") != 1:
            raise ExtractionError(
                f"requirements line {line_number} is not an exact pin"
            )
        name, expected = line.split("==", 1)
        if not name or not expected or name in packages:
            raise ExtractionError(f"invalid duplicate requirement {name!r}")
        actual = importlib.metadata.version(name)
        if actual != expected:
            raise ExtractionError(
                f"runtime package {name} is {actual}, expected pinned {expected}"
            )
        packages[name] = expected
    if packages.get("pdfplumber") != pdfplumber.__version__:
        raise ExtractionError("pdfplumber runtime metadata disagreement")
    if packages.get("PyMuPDF") != pymupdf.__version__:
        raise ExtractionError("PyMuPDF runtime metadata disagreement")
    return {
        "python": EXPECTED_PYTHON_VERSION,
        "pdfplumber": pdfplumber.__version__,
        "pymupdf": pymupdf.__version__,
        "pinned_packages": dict(sorted(packages.items(), key=lambda item: item[0].casefold())),
    }


def indexed_file(path: Path, role: str) -> dict[str, Any]:
    return {"role": role, "sha256": sha256_file(path), "bytes": path.stat().st_size}


def verify_extraction_index(
    index_path: Path,
    artifact_path: Path,
    expected_snapshots: dict[str, str] | None = None,
) -> None:
    index_path = index_path.resolve(strict=True)
    artifact_path = artifact_path.resolve(strict=True)
    if index_path.name != "extraction-index.json":
        raise ExtractionError("extraction index has an unexpected filename")
    index = json.loads(index_path.read_text(encoding="utf-8"))
    if not isinstance(index, dict):
        raise ExtractionError("extraction index root must be an object")
    require_exact_keys(
        index,
        {
            "artifact",
            "counts",
            "files",
            "format",
            "policies",
            "reconciliation_format",
            "runtime",
        },
        "extraction index",
    )
    if index["format"] != INDEX_FORMAT or index["reconciliation_format"] != FORMAT:
        raise ExtractionError("unsupported extraction index format")
    if index["artifact"] != {
        "sha256": sha256_file(artifact_path),
        "bytes": artifact_path.stat().st_size,
        "page_count": EXPECTED_PAGE_COUNT,
    }:
        raise ExtractionError("extraction index artifact does not match")
    if not isinstance(index["counts"], dict):
        raise ExtractionError("extraction index counts must be an object")
    if not isinstance(index["runtime"], dict) or not isinstance(index["policies"], dict):
        raise ExtractionError("extraction index runtime and policies must be objects")
    if not isinstance(index["files"], dict):
        raise ExtractionError("extraction index files must be an object")
    if set(index["files"]) != set(INDEX_FILE_ROLES):
        raise ExtractionError(
            "extraction index does not contain the exact required 12-file contract"
        )
    root = index_path.parent.resolve(strict=True)
    expected_paths = set(INDEX_FILE_ROLES)
    symlinks = [
        path.relative_to(root).as_posix()
        for path in root.rglob("*")
        if path.is_symlink()
    ]
    if symlinks:
        raise ExtractionError(f"extraction output contains symlinks: {symlinks!r}")
    actual_paths = {
        path.relative_to(root).as_posix()
        for path in root.rglob("*")
        if path.is_file() and path != index_path
    }
    if actual_paths != expected_paths:
        raise ExtractionError(
            f"extraction index file set differs: missing={sorted(expected_paths-actual_paths)!r}, "
            f"unknown={sorted(actual_paths-expected_paths)!r}"
        )
    for relative, metadata in index["files"].items():
        if "\\" in relative or relative.startswith("/") or ".." in Path(relative).parts:
            raise ExtractionError(f"unsafe indexed path: {relative!r}")
        if not isinstance(metadata, dict):
            raise ExtractionError(f"index file metadata is not an object: {relative}")
        require_exact_keys(metadata, {"bytes", "role", "sha256"}, f"index file {relative}")
        if metadata["role"] != INDEX_FILE_ROLES[relative]:
            raise ExtractionError(f"indexed file has the wrong role: {relative}")
        if (
            not isinstance(metadata["bytes"], int)
            or metadata["bytes"] < 0
            or not isinstance(metadata["sha256"], str)
            or not re.fullmatch(r"sha256:[0-9a-f]{64}", metadata["sha256"])
        ):
            raise ExtractionError(f"indexed file metadata is invalid: {relative}")
        path = root / Path(relative)
        if path.stat().st_size != metadata["bytes"] or sha256_file(path) != metadata["sha256"]:
            raise ExtractionError(f"indexed file changed: {relative}")
    if expected_snapshots:
        for relative, expected_hash in expected_snapshots.items():
            metadata = index["files"].get(relative)
            if metadata is None or metadata["sha256"] != expected_hash:
                raise ExtractionError(f"indexed snapshot does not match execution input: {relative}")

    reconciled = json.loads((root / "reconciled.json").read_text(encoding="utf-8"))
    if not isinstance(reconciled, dict):
        raise ExtractionError("reconciled document root must be an object")
    require_exact_keys(reconciled, {"format", "rows", "summary"}, "reconciled document")
    if reconciled["format"] != FORMAT or reconciled["summary"] != index["counts"]:
        raise ExtractionError("index counts do not match the reconciled summary")
    if (
        not isinstance(reconciled["rows"], list)
        or len(reconciled["rows"]) != index["counts"].get("total_rows")
    ):
        raise ExtractionError("reconciled row count does not match index counts")
    reconciliation = json.loads(
        (root / "reconciliation.json").read_text(encoding="utf-8")
    )
    if not isinstance(reconciliation, dict):
        raise ExtractionError("reconciliation summary must be an object")
    for key, value in index["counts"].items():
        if reconciliation.get(key) != value:
            raise ExtractionError(
                f"reconciliation summary does not match index counts at {key!r}"
            )


def run_extract(args: argparse.Namespace) -> None:
    pdf_path = args.pdf.resolve(strict=True)
    source_metadata_path = args.source_metadata.resolve(strict=True)
    overrides_path = args.overrides.resolve(strict=True)
    parser_path = EXECUTING_PARSER_PATH
    parser_bytes = EXECUTING_PARSER_BYTES
    requirements_path = parser_path.with_name("ima-requirements.txt").resolve(strict=True)
    requirements_bytes = requirements_path.read_bytes()
    source_metadata_bytes = source_metadata_path.read_bytes()
    overrides_bytes = overrides_path.read_bytes()
    output = args.output.resolve()
    if output.exists() and any(output.iterdir()):
        raise ExtractionError(f"output directory is not empty: {output}")
    output.mkdir(parents=True, exist_ok=True)

    artifact_sha256 = sha256_file(pdf_path)
    if artifact_sha256 != args.expected_sha256.lower():
        raise ExtractionError(
            f"artifact digest mismatch: {artifact_sha256} != {args.expected_sha256.lower()}"
    )
    artifact_bytes = pdf_path.stat().st_size
    validate_source_metadata(source_metadata_bytes, artifact_sha256, artifact_bytes)
    overrides = load_overrides(overrides_bytes, artifact_sha256)
    runtime = pinned_runtime(requirements_bytes)

    pdfplumber_rows, pdfplumber_raw_rows, pdfplumber_foreword = extract_with_pdfplumber(
        pdf_path
    )
    pymupdf_rows, pymupdf_raw_rows, pymupdf_foreword = extract_with_pymupdf(pdf_path)
    rows, summary, reviewed_whitespace, reviewed_transformations = reconcile(
        artifact_sha256,
        pdfplumber_rows,
        pymupdf_rows,
        pdfplumber_foreword,
        pymupdf_foreword,
        overrides,
    )

    snapshots = {
        parser_path: parser_bytes,
        requirements_path: requirements_bytes,
        source_metadata_path: source_metadata_bytes,
        overrides_path: overrides_bytes,
    }
    changed_inputs = [
        str(path) for path, snapshot in snapshots.items() if path.read_bytes() != snapshot
    ]
    if changed_inputs:
        raise ExtractionError(
            f"parser or reviewed inputs changed during extraction: {changed_inputs!r}"
        )

    files_to_write: list[tuple[str, str, bytes]] = [
        (
            "inputs/source-metadata.json",
            "source-metadata",
            source_metadata_bytes,
        ),
        ("inputs/overrides.json", "reviewed-overrides", overrides_bytes),
        ("parser/ima_extract.py", "parser-source", parser_bytes),
        (
            "parser/ima-requirements.txt",
            "parser-requirements",
            requirements_bytes,
        ),
        (
            "engines/pdfplumber.raw.jsonl",
            "engine-raw",
            json_line_bytes(pdfplumber_raw_rows),
        ),
        (
            "engines/pdfplumber.normalized.jsonl",
            "engine-normalized",
            json_line_bytes(pdfplumber_rows),
        ),
        (
            "engines/pymupdf.raw.jsonl",
            "engine-raw",
            json_line_bytes(pymupdf_raw_rows),
        ),
        (
            "engines/pymupdf.normalized.jsonl",
            "engine-normalized",
            json_line_bytes(pymupdf_rows),
        ),
        (
            "audits/reviewed-whitespace-resolutions.json",
            "reviewed-whitespace-resolutions",
            (
                json.dumps(
                    {
                        "artifact_sha256": artifact_sha256,
                        "events": reviewed_whitespace,
                        "format": FORMAT,
                        "review_policy": overrides["review_policy"],
                    },
                    ensure_ascii=False,
                    indent=2,
                    sort_keys=True,
                )
                + "\n"
            ).encode("utf-8"),
        ),
        (
            "audits/source-transformations.json",
            "reviewed-source-transformations",
            (
                json.dumps(
                    {
                        "artifact_sha256": artifact_sha256,
                        "events": reviewed_transformations,
                        "format": FORMAT,
                        "review_policy": overrides["review_policy"],
                    },
                    ensure_ascii=False,
                    indent=2,
                    sort_keys=True,
                )
                + "\n"
            ).encode("utf-8"),
        ),
    ]
    normalized_stream_hashes = {
        "pdfplumber": sha256_bytes(files_to_write[5][2]),
        "pymupdf": sha256_bytes(files_to_write[7][2]),
    }
    raw_stream_hashes = {
        "pdfplumber": sha256_bytes(files_to_write[4][2]),
        "pymupdf": sha256_bytes(files_to_write[6][2]),
    }
    reconciled_bytes = (
        json.dumps(
            {"format": FORMAT, "summary": summary, "rows": [asdict(row) for row in rows]},
            ensure_ascii=False,
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode("utf-8")
    reconciliation = dict(summary)
    reconciliation["engine_normalized_stream_sha256"] = normalized_stream_hashes
    reconciliation["engine_raw_stream_sha256"] = raw_stream_hashes
    reconciliation["overrides_sha256"] = sha256_bytes(overrides_bytes)
    reconciliation["source_metadata_sha256"] = sha256_bytes(source_metadata_bytes)
    reconciliation_bytes = (
        json.dumps(reconciliation, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    ).encode("utf-8")
    files_to_write.extend(
        [
            ("reconciled.json", "reconciled-records", reconciled_bytes),
            ("reconciliation.json", "reconciliation-summary", reconciliation_bytes),
        ]
    )
    roles = {relative: role for relative, role, _ in files_to_write}
    if roles != INDEX_FILE_ROLES or len(roles) != len(files_to_write):
        raise ExtractionError("parser output list differs from the required 12-file contract")
    for relative, role, data in files_to_write:
        write_new_file(output / Path(relative), data)

    indexed_files = {
        relative: indexed_file(output / Path(relative), roles[relative])
        for relative in sorted(roles)
    }
    index = {
        "format": INDEX_FORMAT,
        "reconciliation_format": FORMAT,
        "artifact": {
            "sha256": artifact_sha256,
            "bytes": artifact_bytes,
            "page_count": EXPECTED_PAGE_COUNT,
        },
        "runtime": runtime,
        "policies": {
            "normalization": NORMALIZATION_POLICY,
            "override_review": overrides["review_policy"],
        },
        "counts": summary,
        "files": indexed_files,
    }
    index_path = output / "extraction-index.json"
    write_new_file(
        index_path,
        (json.dumps(index, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode(
            "utf-8"
        ),
    )
    verify_extraction_index(
        index_path,
        pdf_path,
        {
            "inputs/overrides.json": sha256_bytes(overrides_bytes),
            "inputs/source-metadata.json": sha256_bytes(source_metadata_bytes),
            "parser/ima-requirements.txt": sha256_bytes(requirements_bytes),
            "parser/ima_extract.py": sha256_bytes(parser_bytes),
        },
    )
    print(
        json.dumps(
            {
                "extraction_index": index_path.as_posix(),
                "extraction_index_sha256": sha256_file(index_path),
                "summary": summary,
            },
            ensure_ascii=False,
            sort_keys=True,
        )
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Dual-extract and reconcile an official IMA-CNMNC master-list PDF"
    )
    parser.add_argument("--pdf", type=Path, required=True)
    parser.add_argument("--source-metadata", type=Path, required=True)
    parser.add_argument("--overrides", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--expected-sha256",
        required=True,
        help="artifact hash, formatted sha256:<64 lowercase hex characters>",
    )
    return parser


def main() -> int:
    try:
        args = build_parser().parse_args()
        if not re.fullmatch(r"sha256:[0-9a-f]{64}", args.expected_sha256):
            raise ExtractionError("--expected-sha256 has an invalid format")
        run_extract(args)
        return 0
    except (ExtractionError, OSError, ValueError) as error:
        print(f"IMA extraction failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
