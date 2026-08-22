use crate::registry::{
    canonical_mineral_chunk_hash, canonical_mineral_manifest_hash, canonical_mineral_records_hash,
    MineralArtifactDescriptor, MineralDatasetDescriptor, MineralDatasetManifest,
    MineralIngestionChunk, MineralIngestionItem, MineralIngestionPolicy, MineralOfficialFacts,
    MineralParserDescriptor, MineralReleaseDescriptor, MineralRetrievalDescriptor,
    MineralSnapshotKind, MineralSourceAttribution, MineralSourceDescriptor,
    MAX_MINERAL_INGESTION_CHUNK_ITEMS, MINERAL_INGESTION_SCHEMA_VERSION,
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, NaiveDate};
use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::net::IpAddr;
use std::path::{Component, Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

pub const IMA_RECONCILIATION_FORMAT: &str = "waajacu-ima-reconciliation-v2";
pub const IMA_EXTRACTION_INDEX_FORMAT: &str = "waajacu-ima-extraction-index-v1";
pub const IMA_IDENTITY_LEDGER_FORMAT: &str = "waajacu-ima-identity-ledger-v1";
pub const IMA_IDENTITY_OVERRIDES_FORMAT: &str = "waajacu-ima-identity-overrides-v1";
pub const IMA_RELEASE_BUNDLE_FORMAT: &str = "waajacu-ima-release-bundle-v2";
pub const IMA_DATASET_KEY: &str = "ima.cnmnc.master_list";
pub const IMA_SOURCE_KEY: &str = "ima.cnmnc";
pub const IMA_DATASET_TITLE: &str = "IMA-CNMNC List of Mineral Names";
pub const IMA_SOURCE_LICENSE: &str = "CC-BY-SA-3.0";
pub const IMA_ADAPTER_NAME: &str = "waajacu_ima_release_adapter";
pub const IMA_ADAPTER_VERSION: &str = "2.1.0";
pub const LEGACY_PHENAKITE_SLUG: &str = "mineral.silicates.0x5b6b8000";
pub const IMA_EXPECTED_ARTIFACT_SHA256: &str =
    "sha256:60bcc403b0c23f06b8089edeed03785c4d042d4dc1a40e4da86af31e28210680";
pub const IMA_EXPECTED_ARTIFACT_BYTES: u64 = 3_180_658;
pub const IMA_EXPECTED_OVERRIDE_SHA256: &str =
    "sha256:7477913721a94f807e70996c268cefa7c34e6290a273536f09e413971e09560a";
pub const IMA_EXPECTED_EXTRACTION_INDEX_SHA256: &str =
    "sha256:76f8e93eacbbf2c51e1530f0e6eadad85af234428c0535220cf0fdb3fa8e8b1e";
pub const IMA_EXPECTED_SOURCE_METADATA_SHA256: &str =
    "sha256:d08bc9bbe89156ef401d73f918500fd2d71292bb94123489cbab13927a367c63";
pub const IMA_EXPECTED_PARSER_SOURCE_SHA256: &str =
    "sha256:814810789114d46806d295b3b9a798defefe22dc28174ac295aa4f1d2d606936";
pub const IMA_EXPECTED_RECONCILED_FILE_SHA256: &str =
    "sha256:f3584590afe7750d84f8dc411777311589e75a626736e316ea4bbfe57e57a157";
pub const IMA_EXPECTED_RECONCILIATION_REPORT_SHA256: &str =
    "sha256:e4faba49533845345993bff111377078feae441f09399bb689af32bf756efa6a";
pub const IMA_NORMALIZATION_POLICY: &str = "nfc-collapse-whitespace-v2";
pub const IMA_OVERRIDE_REVIEW_POLICY: &str = "artifact-bound-rendered-source-review-v1";
pub const IMA_ATTRIBUTION_PARTY: &str = "International Mineralogical Association, Commission on New Minerals, Nomenclature and Classification (IMA-CNMNC)";
pub const IMA_LICENSE_URL: &str = "https://creativecommons.org/licenses/by-sa/3.0/";
const EXTRACTION_INDEX_NAME: &str = "extraction-index.json";
const EXTRACTION_ARCHIVE_DIR: &str = "private/extraction";
const EXTRACTION_INDEX_ARCHIVE_FILE: &str = "private/extraction/extraction-index.json";
const ARTIFACT_ARCHIVE_FILE: &str = "private/source/ima-master-list.pdf";
const RECONCILIATION_ARCHIVE_FILE: &str = "private/extraction/reconciled.json";
const IDENTITY_LEDGER_ARCHIVE_FILE: &str = "private/identity-ledger.json";
const AUDIT_ARCHIVE_FILE: &str = "private/audit-rows.jsonl";
const SOURCE_METADATA_RELATIVE: &str = "inputs/source-metadata.json";
const SOURCE_OVERRIDES_RELATIVE: &str = "inputs/overrides.json";
const PARSER_SOURCE_RELATIVE: &str = "parser/ima_extract.py";
const PARSER_REQUIREMENTS_RELATIVE: &str = "parser/ima-requirements.txt";
const PDFPLUMBER_RAW_RELATIVE: &str = "engines/pdfplumber.raw.jsonl";
const PDFPLUMBER_NORMALIZED_RELATIVE: &str = "engines/pdfplumber.normalized.jsonl";
const PYMUPDF_RAW_RELATIVE: &str = "engines/pymupdf.raw.jsonl";
const PYMUPDF_NORMALIZED_RELATIVE: &str = "engines/pymupdf.normalized.jsonl";
const WHITESPACE_AUDIT_RELATIVE: &str = "audits/reviewed-whitespace-resolutions.json";
const TRANSFORMATION_AUDIT_RELATIVE: &str = "audits/source-transformations.json";
const RECONCILED_RELATIVE: &str = "reconciled.json";
const RECONCILIATION_REPORT_RELATIVE: &str = "reconciliation.json";
const SOURCE_COLUMNS: [&str; 7] = [
    "canonical_name",
    "formula",
    "raw_status",
    "ima_number_year",
    "country",
    "first_reference",
    "second_reference",
];

const INDEXED_EXTRACTION_FILES: [(&str, &str); 12] = [
    (SOURCE_METADATA_RELATIVE, "source-metadata"),
    (SOURCE_OVERRIDES_RELATIVE, "reviewed-overrides"),
    (PARSER_SOURCE_RELATIVE, "parser-source"),
    (PARSER_REQUIREMENTS_RELATIVE, "parser-requirements"),
    (PDFPLUMBER_RAW_RELATIVE, "engine-raw"),
    (PDFPLUMBER_NORMALIZED_RELATIVE, "engine-normalized"),
    (PYMUPDF_RAW_RELATIVE, "engine-raw"),
    (PYMUPDF_NORMALIZED_RELATIVE, "engine-normalized"),
    (WHITESPACE_AUDIT_RELATIVE, "reviewed-whitespace-resolutions"),
    (
        TRANSFORMATION_AUDIT_RELATIVE,
        "reviewed-source-transformations",
    ),
    (RECONCILED_RELATIVE, "reconciled-records"),
    (RECONCILIATION_REPORT_RELATIVE, "reconciliation-summary"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaReconciledDocument {
    pub format: String,
    pub summary: ImaReconciliationSummary,
    pub rows: Vec<ImaExtractedRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaReconciliationSummary {
    pub format: String,
    pub artifact_sha256: String,
    pub page_count: usize,
    pub table_page_count: usize,
    pub release_label: String,
    pub license_spdx: String,
    pub declared_valid_species: usize,
    pub total_rows: usize,
    pub valid_species: usize,
    pub hidden_historical_rows: usize,
    pub status_counts: BTreeMap<String, usize>,
    pub official_ima_number_count: usize,
    pub missing_formula_count: usize,
    pub extractor_disagreement_count: usize,
    pub reviewed_whitespace_resolution_count: usize,
    pub reviewed_whitespace_resolution_fields: BTreeMap<String, usize>,
    pub extractor_versions: ImaExtractorVersions,
    pub formula_replacement_glyph_count: usize,
    pub formula_private_use_count: usize,
    pub formula_cyrillic_count: usize,
    pub normalization_policy: String,
    pub override_review_policy: String,
    pub source_transformation_count: usize,
    pub source_transformation_fields: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaExtractorVersions {
    pub pdfplumber: String,
    pub pymupdf: String,
    pub python: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaExtractedRow {
    pub ordinal: usize,
    pub page: usize,
    pub page_row: usize,
    pub bbox: Vec<f64>,
    pub canonical_name: String,
    pub formula: String,
    pub raw_status: String,
    pub ima_number_year: String,
    pub country: String,
    pub first_reference: String,
    pub second_reference: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImaRawExtractedRow {
    ordinal: usize,
    page: usize,
    page_row: usize,
    bbox: Vec<f64>,
    cell_bboxes: BTreeMap<String, Vec<f64>>,
    values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaExtractionIndex {
    pub format: String,
    pub reconciliation_format: String,
    pub artifact: ImaExtractionArtifact,
    pub runtime: ImaExtractionRuntime,
    pub policies: ImaExtractionPolicies,
    pub counts: ImaReconciliationSummary,
    pub files: BTreeMap<String, ImaIndexedFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaExtractionArtifact {
    pub sha256: String,
    pub bytes: u64,
    pub page_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaExtractionRuntime {
    pub python: String,
    pub pdfplumber: String,
    pub pymupdf: String,
    pub pinned_packages: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaExtractionPolicies {
    pub normalization: String,
    pub override_review: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaIndexedFile {
    pub role: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaSourceMetadata {
    pub format: String,
    pub dataset_key: String,
    pub source_key: String,
    pub landing_page: String,
    pub retrieved_at: String,
    pub artifact: ImaSourceArtifactMetadata,
    pub attribution: ImaSourceAttributionMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaSourceArtifactMetadata {
    pub bytes: u64,
    pub content_type: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub sha256: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaSourceAttributionMetadata {
    pub creator: String,
    pub source_title: String,
    pub license_spdx: String,
    pub license_url: String,
    pub changes_notice: String,
    pub no_endorsement_notice: String,
    pub derived_output_license_spdx: String,
}

#[derive(Debug, Clone)]
pub struct ImaVerifiedExtraction {
    pub document: ImaReconciledDocument,
    pub index: ImaExtractionIndex,
    pub source_metadata: ImaSourceMetadata,
    extraction_root: PathBuf,
    artifact_path: PathBuf,
    extraction_index_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImaExtractionOverrides {
    format: String,
    artifact_sha256: String,
    review_policy: String,
    whitespace_resolutions: Vec<ImaWhitespaceResolution>,
    formula_transformations: Vec<ImaFormulaTransformation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImaWhitespaceResolution {
    field: String,
    ordinal: usize,
    page: usize,
    page_row: usize,
    pdfplumber: String,
    pymupdf: String,
    resolved: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImaFormulaTransformation {
    canonical_name: String,
    ordinal: usize,
    page: usize,
    page_row: usize,
    raw_formula: String,
    replacements: Vec<ImaFormulaReplacement>,
    resolved_formula: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImaFormulaReplacement {
    count: usize,
    from: String,
    to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImaReviewedEvents<T> {
    artifact_sha256: String,
    events: Vec<T>,
    format: String,
    review_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaIdentityLedger {
    pub format: String,
    pub dataset_key: String,
    pub revision: u64,
    pub parent_sha256: Option<String>,
    pub entries: Vec<ImaIdentityEntry>,
}

impl ImaIdentityLedger {
    pub fn empty() -> Self {
        Self {
            format: IMA_IDENTITY_LEDGER_FORMAT.to_string(),
            dataset_key: IMA_DATASET_KEY.to_string(),
            revision: 0,
            parent_sha256: None,
            entries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaIdentityEntry {
    pub source_record_id: String,
    pub slug: String,
    pub canonical_names: Vec<String>,
    pub ima_numbers: Vec<String>,
    pub first_seen_release: String,
    pub first_seen_artifact_sha256: String,
    pub first_seen_source_locator: String,
    pub bootstrap_adoption: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaIdentityOverrides {
    pub format: String,
    pub dataset_key: String,
    pub entries: Vec<ImaIdentityOverride>,
}

impl ImaIdentityOverrides {
    pub fn empty() -> Self {
        Self {
            format: IMA_IDENTITY_OVERRIDES_FORMAT.to_string(),
            dataset_key: IMA_DATASET_KEY.to_string(),
            entries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaIdentityOverride {
    pub canonical_name: String,
    pub slug: String,
    #[serde(default)]
    pub source_record_id: Option<String>,
    pub adopt_existing_route: bool,
}

#[derive(Debug, Clone)]
pub struct ImaBundleBuildOptions {
    pub extraction_index_path: PathBuf,
    pub artifact_path: PathBuf,
    pub ledger_path: PathBuf,
    pub output: PathBuf,
    pub released_at: String,
    pub base_batch_id: Option<String>,
    pub chunk_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaReleaseBundleIndex {
    pub format: String,
    pub chunk_size: usize,
    pub artifact_sha256: String,
    pub artifact_file: String,
    pub artifact_file_sha256: String,
    pub extraction_index_file: String,
    pub extraction_index_file_sha256: String,
    pub source_metadata_file_sha256: String,
    pub source_overrides_file_sha256: String,
    pub source_transformations_file_sha256: String,
    pub parser_source_file_sha256: String,
    pub reconciliation_sha256: String,
    pub reconciliation_file: String,
    pub reconciliation_file_sha256: String,
    pub identity_ledger_sha256: String,
    pub identity_ledger_file: String,
    pub identity_ledger_file_sha256: String,
    pub audit_file: String,
    pub audit_file_sha256: String,
    pub manifest_sha256: String,
    pub manifest_file_sha256: String,
    pub records_sha256: String,
    pub chunks: Vec<ImaReleaseChunkIndex>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaReleaseChunkIndex {
    pub chunk_index: usize,
    pub content_sha256: String,
    pub file: String,
    pub file_sha256: String,
    pub item_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImaAuditRow {
    ordinal: usize,
    physical_pdf_page: usize,
    page_row: usize,
    bbox: Vec<f64>,
    source_locator: String,
    source_record_id: String,
    slug: String,
    bootstrap_adoption: bool,
    canonical_name: String,
    formula: String,
    raw_status: String,
    raw_ima_number_year: String,
    ima_number_year_kind: String,
    official_ima_number: Option<String>,
    country: String,
    first_reference: String,
    second_reference: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImaStatusMapping {
    pub nomenclature_status: &'static str,
    pub is_valid_species: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IngestionMutationResponse {
    pub batch_id: String,
    pub status: String,
    pub manifest_hash: String,
    pub report_hash: Option<String>,
    pub received_chunk_count: usize,
    pub expected_chunk_count: usize,
    pub received_record_count: usize,
    pub expected_record_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImaStageOutcome {
    pub batch_id: String,
    pub status: String,
    pub manifest_hash: String,
    pub report_hash: Option<String>,
    pub received_chunk_count: usize,
    pub expected_chunk_count: usize,
    pub received_record_count: usize,
    pub expected_record_count: usize,
}

impl From<IngestionMutationResponse> for ImaStageOutcome {
    fn from(value: IngestionMutationResponse) -> Self {
        Self {
            batch_id: value.batch_id,
            status: value.status,
            manifest_hash: value.manifest_hash,
            report_hash: value.report_hash,
            received_chunk_count: value.received_chunk_count,
            expected_chunk_count: value.expected_chunk_count,
            received_record_count: value.received_record_count,
            expected_record_count: value.expected_record_count,
        }
    }
}

fn load_reconciled_document(path: &Path) -> Result<ImaReconciledDocument> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read reconciliation {}", path.display()))?;
    let document: ImaReconciledDocument = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid reconciliation JSON in {}", path.display()))?;
    validate_reconciled_document(&document)?;
    Ok(document)
}

pub fn load_verified_extraction(
    extraction_index_path: &Path,
    artifact_path: &Path,
) -> Result<ImaVerifiedExtraction> {
    if extraction_index_path
        .file_name()
        .and_then(|value| value.to_str())
        != Some(EXTRACTION_INDEX_NAME)
    {
        bail!("the extraction trust boundary requires a file named {EXTRACTION_INDEX_NAME}");
    }
    reject_symlink_or_non_file(extraction_index_path, "extraction index")?;
    reject_symlink_or_non_file(artifact_path, "source PDF artifact")?;
    let extraction_root = extraction_index_path
        .parent()
        .context("extraction index has no parent directory")?
        .to_path_buf();
    let index_bytes = fs::read(extraction_index_path).with_context(|| {
        format!(
            "failed to read extraction index {}",
            extraction_index_path.display()
        )
    })?;
    let index: ImaExtractionIndex = serde_json::from_slice(&index_bytes).with_context(|| {
        format!(
            "invalid extraction index JSON in {}",
            extraction_index_path.display()
        )
    })?;
    let extraction_index_sha256 = sha256_bytes(&index_bytes);
    if extraction_index_sha256 != IMA_EXPECTED_EXTRACTION_INDEX_SHA256 {
        bail!("extraction index bytes do not match the audited July 2026 package");
    }
    validate_extraction_index_contract(&index)?;
    validate_indexed_extraction_files(&extraction_root, extraction_index_path, &index)?;

    let artifact_metadata = fs::metadata(artifact_path)
        .with_context(|| format!("failed to inspect artifact {}", artifact_path.display()))?;
    if artifact_metadata.len() != index.artifact.bytes
        || sha256_file(artifact_path)? != index.artifact.sha256
    {
        bail!("source PDF does not match the artifact bound into the extraction index");
    }

    let document_path = extraction_root.join(RECONCILED_RELATIVE);
    let document = load_reconciled_document(&document_path)?;
    validate_official_release_contract(&document)?;
    if document.summary != index.counts {
        bail!("extraction index counts do not exactly match reconciled.summary");
    }

    let source_metadata_path = extraction_root.join(SOURCE_METADATA_RELATIVE);
    let source_metadata = load_source_metadata(&source_metadata_path)?;
    validate_source_metadata(&source_metadata, &index)?;
    let overrides: ImaExtractionOverrides =
        read_json(&extraction_root.join(SOURCE_OVERRIDES_RELATIVE))?;
    validate_extraction_overrides(&overrides, &index)?;

    verify_reconciliation_report(&extraction_root, &document, &index)?;
    recompute_reconciled_rows(&extraction_root, &document, &overrides)?;

    Ok(ImaVerifiedExtraction {
        document,
        index,
        source_metadata,
        extraction_root,
        artifact_path: artifact_path.to_path_buf(),
        extraction_index_sha256,
    })
}

pub fn load_identity_ledger(path: &Path) -> Result<ImaIdentityLedger> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read ledger {}", path.display()))?;
    let ledger: ImaIdentityLedger = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid identity ledger JSON in {}", path.display()))?;
    validate_identity_ledger(&ledger)?;
    Ok(ledger)
}

pub fn load_identity_overrides(path: &Path) -> Result<ImaIdentityOverrides> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read identity overrides {}", path.display()))?;
    let overrides: ImaIdentityOverrides = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid identity overrides JSON in {}", path.display()))?;
    validate_identity_overrides(&overrides)?;
    Ok(overrides)
}

pub fn map_ima_status(raw: &str) -> Result<ImaStatusMapping> {
    let mapping = match raw {
        "A" => ImaStatusMapping {
            nomenclature_status: "approved",
            is_valid_species: true,
        },
        "A?" | "A ?" => ImaStatusMapping {
            nomenclature_status: "uncertain",
            is_valid_species: true,
        },
        "G" => ImaStatusMapping {
            nomenclature_status: "grandfathered",
            is_valid_species: true,
        },
        "Rd" => ImaStatusMapping {
            nomenclature_status: "redefined",
            is_valid_species: true,
        },
        "Rn" => ImaStatusMapping {
            nomenclature_status: "renamed",
            is_valid_species: true,
        },
        "Q" => ImaStatusMapping {
            nomenclature_status: "questionable",
            is_valid_species: true,
        },
        "D" => ImaStatusMapping {
            nomenclature_status: "discredited",
            is_valid_species: false,
        },
        _ => bail!("unsupported raw IMA status '{raw}'"),
    };
    Ok(mapping)
}

pub fn is_official_ima_number(value: &str) -> bool {
    let bytes = value.as_bytes();
    if !matches!(bytes.len(), 8 | 9)
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || bytes[4] != b'-'
        || !bytes[5..8].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    bytes.len() == 8 || bytes[8].is_ascii_alphabetic()
}

pub fn official_ima_number(value: &str) -> Option<String> {
    let trimmed = value.trim();
    is_official_ima_number(trimmed).then(|| trimmed.to_ascii_lowercase())
}

pub fn classify_ima_number_year(value: &str) -> Result<&'static str> {
    let trimmed = value.trim();
    if is_official_ima_number(trimmed) {
        return Ok("official_ima_number");
    }
    if trimmed.len() == 4 && trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok("year");
    }
    let special = trimmed.strip_suffix(" s.p.").unwrap_or("");
    if special.len() == 4 && special.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok("special_procedure_year");
    }
    if trimmed == "?" {
        return Ok("unknown");
    }
    if trimmed.strip_suffix(" ?").is_some_and(is_four_digit_year) {
        return Ok("uncertain_year");
    }
    if trimmed
        .strip_suffix(" s.p.?")
        .or_else(|| trimmed.strip_suffix(" s.p. ?"))
        .is_some_and(is_four_digit_year)
    {
        return Ok("uncertain_special_procedure_year");
    }
    if is_placeholder_ima_number(trimmed) {
        return Ok("placeholder_ima_number");
    }
    if trimmed
        .strip_suffix(" ?")
        .is_some_and(is_placeholder_ima_number)
    {
        return Ok("uncertain_placeholder_ima_number");
    }
    bail!("unsupported raw IMA No. / Year value '{value}'")
}

fn is_four_digit_year(value: &str) -> bool {
    value.len() == 4 && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_placeholder_ima_number(value: &str) -> bool {
    value.len() == 8
        && value.as_bytes()[..4]
            .iter()
            .all(|byte| byte.is_ascii_digit())
        && &value[4..] == "-xxx"
}

pub fn initialize_identity_ledger(document: &ImaReconciledDocument) -> Result<ImaIdentityLedger> {
    initialize_identity_ledger_with_overrides(document, &ImaIdentityOverrides::empty())
}

pub fn initialize_identity_ledger_with_overrides(
    document: &ImaReconciledDocument,
    overrides: &ImaIdentityOverrides,
) -> Result<ImaIdentityLedger> {
    if document.rows.is_empty() {
        bail!("cannot initialize an identity ledger from an empty release");
    }
    validate_identity_overrides(overrides)?;
    if document
        .rows
        .iter()
        .any(|row| normalize_identity_name(&row.canonical_name) == "phenakite")
        && !overrides.entries.iter().any(|entry| {
            normalize_identity_name(&entry.canonical_name) == "phenakite"
                && entry.adopt_existing_route
                && entry.slug == LEGACY_PHENAKITE_SLUG
        })
    {
        bail!(
            "Phenakite must explicitly adopt existing route '{}' through an identity override",
            LEGACY_PHENAKITE_SLUG
        );
    }
    evolve_identity_ledger_internal(document, &ImaIdentityLedger::empty(), true, Some(overrides))
}

pub fn evolve_identity_ledger(
    document: &ImaReconciledDocument,
    previous: &ImaIdentityLedger,
    allow_new_identities: bool,
) -> Result<ImaIdentityLedger> {
    evolve_identity_ledger_internal(document, previous, allow_new_identities, None)
}

fn evolve_identity_ledger_internal(
    document: &ImaReconciledDocument,
    previous: &ImaIdentityLedger,
    allow_new_identities: bool,
    bootstrap_overrides: Option<&ImaIdentityOverrides>,
) -> Result<ImaIdentityLedger> {
    validate_reconciled_document(document)?;
    validate_identity_ledger(previous)?;
    let parent_sha256 = canonical_value_sha256(previous)?;
    let release_version = release_version(&document.summary.release_label)?;

    let mut entries = previous.entries.clone();
    let mut by_number = HashMap::<String, usize>::new();
    let mut by_name = HashMap::<String, usize>::new();
    for (index, entry) in entries.iter().enumerate() {
        for number in &entry.ima_numbers {
            if by_number
                .insert(number.to_ascii_lowercase(), index)
                .is_some()
            {
                bail!("identity ledger contains a duplicate IMA number '{number}'");
            }
        }
        for name in &entry.canonical_names {
            let normalized = normalize_identity_name(name);
            if by_name.insert(normalized, index).is_some() {
                bail!("identity ledger contains a duplicate canonical name '{name}'");
            }
        }
    }

    let mut matched_entries = HashSet::new();
    let mut unresolved = Vec::new();
    let overrides_by_name = bootstrap_overrides
        .map(|overrides| {
            overrides
                .entries
                .iter()
                .map(|entry| (normalize_identity_name(&entry.canonical_name), entry))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let mut used_overrides = HashSet::new();
    for row in &document.rows {
        let number_match = official_ima_number(&row.ima_number_year)
            .and_then(|number| by_number.get(&number).copied());
        let name_match = by_name
            .get(&normalize_identity_name(&row.canonical_name))
            .copied();
        let resolved = match (number_match, name_match) {
            (Some(left), Some(right)) if left != right => bail!(
                "row '{}' maps to different ledger identities by name and IMA number",
                row.canonical_name
            ),
            (Some(index), _) | (_, Some(index)) => Some(index),
            (None, None) => None,
        };

        let index = if let Some(index) = resolved {
            index
        } else if allow_new_identities {
            let override_entry = overrides_by_name
                .get(&normalize_identity_name(&row.canonical_name))
                .copied();
            if let Some(value) = override_entry {
                if value.canonical_name != row.canonical_name {
                    bail!(
                        "identity override name '{}' must exactly match source name '{}'",
                        value.canonical_name,
                        row.canonical_name
                    );
                }
                used_overrides.insert(normalize_identity_name(&value.canonical_name));
            }
            let source_record_id = override_entry
                .and_then(|entry| entry.source_record_id.clone())
                .map(Ok)
                .unwrap_or_else(|| unique_opaque_source_id(&entries))?;
            let locator = source_locator(&document.summary.artifact_sha256, row);
            let slug = override_entry
                .map(|entry| entry.slug.clone())
                .unwrap_or_else(|| stable_slug(&row.canonical_name, &source_record_id));
            let entry = ImaIdentityEntry {
                source_record_id,
                slug,
                canonical_names: vec![row.canonical_name.clone()],
                ima_numbers: official_ima_number(&row.ima_number_year)
                    .into_iter()
                    .collect(),
                first_seen_release: release_version.clone(),
                first_seen_artifact_sha256: document.summary.artifact_sha256.clone(),
                first_seen_source_locator: locator,
                bootstrap_adoption: override_entry.is_some(),
            };
            let index = entries.len();
            for number in &entry.ima_numbers {
                by_number.insert(number.clone(), index);
            }
            by_name.insert(normalize_identity_name(&row.canonical_name), index);
            entries.push(entry);
            index
        } else {
            unresolved.push(row.canonical_name.clone());
            continue;
        };

        if !matched_entries.insert(index) {
            bail!(
                "more than one current row resolves to source identity '{}'",
                entries[index].source_record_id
            );
        }
        if !entries[index].canonical_names.iter().any(|name| {
            normalize_identity_name(name) == normalize_identity_name(&row.canonical_name)
        }) {
            entries[index]
                .canonical_names
                .push(row.canonical_name.clone());
        }
        if let Some(number) = official_ima_number(&row.ima_number_year) {
            if !entries[index]
                .ima_numbers
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&number))
            {
                entries[index].ima_numbers.push(number);
            }
        }
        entries[index]
            .canonical_names
            .sort_by_key(|name| normalize_identity_name(name));
        entries[index].ima_numbers.sort();
    }

    if let Some(overrides) = bootstrap_overrides {
        let unused = overrides
            .entries
            .iter()
            .filter(|entry| {
                !used_overrides.contains(&normalize_identity_name(&entry.canonical_name))
            })
            .map(|entry| entry.canonical_name.as_str())
            .collect::<Vec<_>>();
        if !unused.is_empty() {
            bail!(
                "identity overrides do not match release rows: {}",
                unused.join(", ")
            );
        }
    }

    if !unresolved.is_empty() {
        bail!(
            "{} release rows have no stable ledger identity; explicitly review them before allowing new identities: {}",
            unresolved.len(),
            unresolved.into_iter().take(12).collect::<Vec<_>>().join(", ")
        );
    }
    entries.sort_by(|left, right| left.source_record_id.cmp(&right.source_record_id));
    let next = ImaIdentityLedger {
        format: IMA_IDENTITY_LEDGER_FORMAT.to_string(),
        dataset_key: IMA_DATASET_KEY.to_string(),
        revision: previous
            .revision
            .checked_add(1)
            .context("identity ledger revision overflow")?,
        parent_sha256: Some(parent_sha256),
        entries,
    };
    validate_identity_ledger(&next)?;
    Ok(next)
}

pub fn write_identity_ledger(path: &Path, ledger: &ImaIdentityLedger) -> Result<()> {
    validate_identity_ledger(ledger)?;
    // Serialize before creating the destination so serialization failure cannot
    // leave an empty path that blocks a corrected retry.
    let mut bytes = serde_json::to_vec_pretty(ledger)?;
    bytes.push(b'\n');
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            bail!("refusing to overwrite identity ledger {}", path.display())
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to create identity ledger {}", path.display()))
        }
    };

    let write_result = (|| -> Result<()> {
        file.write_all(&bytes)
            .with_context(|| format!("failed to write identity ledger {}", path.display()))?;
        file.flush()
            .with_context(|| format!("failed to flush identity ledger {}", path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync identity ledger {}", path.display()))?;
        Ok(())
    })();
    if let Err(error) = write_result {
        drop(file);
        if let Err(cleanup_error) = fs::remove_file(path) {
            if cleanup_error.kind() != std::io::ErrorKind::NotFound {
                return Err(error).context(format!(
                    "also failed to remove partial identity ledger {}: {cleanup_error}",
                    path.display()
                ));
            }
        }
        return Err(error);
    }
    Ok(())
}

pub fn build_release_bundle(options: &ImaBundleBuildOptions) -> Result<ImaReleaseBundleIndex> {
    validate_build_options(options)?;
    let verified =
        load_verified_extraction(&options.extraction_index_path, &options.artifact_path)?;
    let document = &verified.document;
    let ledger = load_identity_ledger(&options.ledger_path)?;
    let reconciliation_sha256 = canonical_value_sha256(document)?;
    let identity_ledger_sha256 = canonical_value_sha256(&ledger)?;
    let release_version = release_version(&document.summary.release_label)?;
    validate_release_dates(
        &release_version,
        &options.released_at,
        &verified.source_metadata.retrieved_at,
    )?;
    let resolved = resolve_release_rows(document, &ledger)?;

    if options.output.exists() && fs::read_dir(&options.output)?.next().is_some() {
        bail!(
            "output directory is not empty: {}",
            options.output.display()
        );
    }
    let chunks_dir = options.output.join("chunks");
    let private_dir = options.output.join("private");
    let extraction_archive_dir = options.output.join(EXTRACTION_ARCHIVE_DIR);
    let source_archive_dir = options.output.join("private/source");
    fs::create_dir_all(&chunks_dir)
        .with_context(|| format!("failed to create {}", chunks_dir.display()))?;
    fs::create_dir_all(&private_dir)
        .with_context(|| format!("failed to create {}", private_dir.display()))?;
    fs::create_dir_all(&extraction_archive_dir)
        .with_context(|| format!("failed to create {}", extraction_archive_dir.display()))?;
    fs::create_dir_all(&source_archive_dir)
        .with_context(|| format!("failed to create {}", source_archive_dir.display()))?;

    archive_verified_extraction(&verified, &options.output)?;

    let mut items = resolved
        .iter()
        .map(|resolved| build_ingestion_item(document, resolved))
        .collect::<Result<Vec<_>>>()?;
    items.sort_by(|left, right| left.source_record_id.cmp(&right.source_record_id));
    let records_sha256 = canonical_mineral_records_hash(&items)?;
    let chunks = items
        .chunks(options.chunk_size)
        .enumerate()
        .map(|(chunk_index, items)| MineralIngestionChunk {
            schema_version: MINERAL_INGESTION_SCHEMA_VERSION,
            chunk_index,
            items: items.to_vec(),
        })
        .collect::<Vec<_>>();

    let configuration = serde_json::json!({
        "chunk_size": options.chunk_size,
        "dataset_key": IMA_DATASET_KEY,
        "extraction_index_sha256": verified.extraction_index_sha256,
        "identity_ledger_sha256": identity_ledger_sha256,
        "ima_number_rule": "ascii_yyyy_nnn_optional_letter_v1",
        "official_fact_mapping": "ima_master_list_context_v1",
        "override_sha256": indexed_hash(&verified.index, SOURCE_OVERRIDES_RELATIVE)?,
        "parser_source_sha256": indexed_hash(&verified.index, PARSER_SOURCE_RELATIVE)?,
        "reconciliation_sha256": reconciliation_sha256,
        "source_key": IMA_SOURCE_KEY,
        "source_transformation_sha256": indexed_hash(&verified.index, TRANSFORMATION_AUDIT_RELATIVE)?,
        "status_mapping": "ima_master_list_status_v1_a_question_to_uncertain",
    });
    let manifest = MineralDatasetManifest {
        schema_version: MINERAL_INGESTION_SCHEMA_VERSION,
        dataset: MineralDatasetDescriptor {
            key: IMA_DATASET_KEY.to_string(),
            title: IMA_DATASET_TITLE.to_string(),
        },
        source: MineralSourceDescriptor {
            key: IMA_SOURCE_KEY.to_string(),
            url: verified.source_metadata.landing_page.clone(),
            license_spdx: document.summary.license_spdx.clone(),
            attribution: Some(MineralSourceAttribution {
                attribution_party: verified.source_metadata.attribution.creator.clone(),
                work_title: verified.source_metadata.attribution.source_title.clone(),
                work_url: verified.source_metadata.artifact.url.clone(),
                license_url: verified.source_metadata.attribution.license_url.clone(),
                changes_notice: verified.source_metadata.attribution.changes_notice.clone(),
                no_endorsement_notice: verified
                    .source_metadata
                    .attribution
                    .no_endorsement_notice
                    .clone(),
                derived_output_license_spdx: verified
                    .source_metadata
                    .attribution
                    .derived_output_license_spdx
                    .clone(),
            }),
        },
        release: MineralReleaseDescriptor {
            version: release_version,
            released_at: options.released_at.clone(),
        },
        retrieval: MineralRetrievalDescriptor {
            retrieved_at: verified.source_metadata.retrieved_at.clone(),
        },
        artifact: MineralArtifactDescriptor {
            url: verified.source_metadata.artifact.url.clone(),
            sha256: document.summary.artifact_sha256.clone(),
        },
        parser: MineralParserDescriptor {
            name: IMA_ADAPTER_NAME.to_string(),
            version: IMA_ADAPTER_VERSION.to_string(),
            code_revision: indexed_hash(&verified.index, PARSER_SOURCE_RELATIVE)?.to_string(),
            configuration_sha256: sha256_bytes(&canonical_json_bytes(&configuration)?),
        },
        policy: MineralIngestionPolicy::ImaIdentityV1,
        expected_record_count: items.len(),
        expected_chunk_count: chunks.len(),
        records_sha256: records_sha256.clone(),
        snapshot_kind: MineralSnapshotKind::Complete,
        base_batch_id: options.base_batch_id.clone(),
    };

    let manifest_path = options.output.join("manifest.json");
    write_pretty_json(&manifest_path, &manifest)?;
    let mut chunk_index = Vec::with_capacity(chunks.len());
    for chunk in &chunks {
        let filename = format!("chunk-{:05}.json", chunk.chunk_index);
        let path = chunks_dir.join(&filename);
        write_pretty_json(&path, chunk)?;
        chunk_index.push(ImaReleaseChunkIndex {
            chunk_index: chunk.chunk_index,
            content_sha256: canonical_mineral_chunk_hash(chunk)?,
            file: format!("chunks/{filename}"),
            file_sha256: sha256_file(&path)?,
            item_count: chunk.items.len(),
        });
    }

    let audit_path = private_dir.join("audit-rows.jsonl");
    let audit_bytes = audit_json_lines(document, &resolved)?;
    fs::write(&audit_path, &audit_bytes)
        .with_context(|| format!("failed to write {}", audit_path.display()))?;
    let reconciliation_archive_path = options.output.join(RECONCILIATION_ARCHIVE_FILE);
    let reconciliation_bytes = fs::read(&reconciliation_archive_path)
        .with_context(|| format!("failed to read {}", reconciliation_archive_path.display()))?;
    let ledger_snapshot_path = options.output.join(IDENTITY_LEDGER_ARCHIVE_FILE);
    write_pretty_json(&ledger_snapshot_path, &ledger)?;

    let index = ImaReleaseBundleIndex {
        format: IMA_RELEASE_BUNDLE_FORMAT.to_string(),
        chunk_size: options.chunk_size,
        artifact_sha256: document.summary.artifact_sha256.clone(),
        artifact_file: ARTIFACT_ARCHIVE_FILE.to_string(),
        artifact_file_sha256: sha256_file(&options.output.join(ARTIFACT_ARCHIVE_FILE))?,
        extraction_index_file: EXTRACTION_INDEX_ARCHIVE_FILE.to_string(),
        extraction_index_file_sha256: verified.extraction_index_sha256,
        source_metadata_file_sha256: indexed_hash(&verified.index, SOURCE_METADATA_RELATIVE)?
            .to_string(),
        source_overrides_file_sha256: indexed_hash(&verified.index, SOURCE_OVERRIDES_RELATIVE)?
            .to_string(),
        source_transformations_file_sha256: indexed_hash(
            &verified.index,
            TRANSFORMATION_AUDIT_RELATIVE,
        )?
        .to_string(),
        parser_source_file_sha256: indexed_hash(&verified.index, PARSER_SOURCE_RELATIVE)?
            .to_string(),
        reconciliation_sha256,
        reconciliation_file: RECONCILIATION_ARCHIVE_FILE.to_string(),
        reconciliation_file_sha256: sha256_bytes(&reconciliation_bytes),
        identity_ledger_sha256,
        identity_ledger_file: IDENTITY_LEDGER_ARCHIVE_FILE.to_string(),
        identity_ledger_file_sha256: sha256_file(&ledger_snapshot_path)?,
        audit_file: AUDIT_ARCHIVE_FILE.to_string(),
        audit_file_sha256: sha256_bytes(&audit_bytes),
        manifest_sha256: canonical_mineral_manifest_hash(&manifest)?,
        manifest_file_sha256: sha256_file(&manifest_path)?,
        records_sha256,
        chunks: chunk_index,
    };
    write_pretty_json(&options.output.join("release-index.json"), &index)?;
    verify_release_bundle(&options.output)?;
    Ok(index)
}

pub fn verify_release_bundle(directory: &Path) -> Result<ImaReleaseBundleIndex> {
    let index_path = directory.join("release-index.json");
    let index: ImaReleaseBundleIndex = read_json(&index_path)?;
    if index.format != IMA_RELEASE_BUNDLE_FORMAT {
        bail!("unsupported IMA release bundle format '{}'", index.format);
    }
    if !(1..=MAX_MINERAL_INGESTION_CHUNK_ITEMS).contains(&index.chunk_size) {
        bail!("release index has an invalid chunk size");
    }
    validate_sha256("index artifact_sha256", &index.artifact_sha256)?;
    validate_sha256("index artifact_file_sha256", &index.artifact_file_sha256)?;
    validate_sha256(
        "index extraction_index_file_sha256",
        &index.extraction_index_file_sha256,
    )?;
    validate_sha256(
        "index source_metadata_file_sha256",
        &index.source_metadata_file_sha256,
    )?;
    validate_sha256(
        "index source_overrides_file_sha256",
        &index.source_overrides_file_sha256,
    )?;
    validate_sha256(
        "index source_transformations_file_sha256",
        &index.source_transformations_file_sha256,
    )?;
    validate_sha256(
        "index parser_source_file_sha256",
        &index.parser_source_file_sha256,
    )?;
    validate_sha256("index reconciliation_sha256", &index.reconciliation_sha256)?;
    validate_sha256(
        "index reconciliation_file_sha256",
        &index.reconciliation_file_sha256,
    )?;
    validate_sha256(
        "index identity_ledger_sha256",
        &index.identity_ledger_sha256,
    )?;
    validate_sha256(
        "index identity_ledger_file_sha256",
        &index.identity_ledger_file_sha256,
    )?;
    validate_sha256("index audit_file_sha256", &index.audit_file_sha256)?;
    validate_sha256("index manifest_sha256", &index.manifest_sha256)?;
    validate_sha256("index manifest_file_sha256", &index.manifest_file_sha256)?;
    validate_sha256("index records_sha256", &index.records_sha256)?;
    for metadata in &index.chunks {
        validate_sha256("chunk content_sha256", &metadata.content_sha256)?;
        validate_sha256("chunk file_sha256", &metadata.file_sha256)?;
    }

    let extraction_index_relative = Path::new(&index.extraction_index_file);
    validate_safe_relative_path(extraction_index_relative, "private")?;
    if index.extraction_index_file != EXTRACTION_INDEX_ARCHIVE_FILE {
        bail!("release index uses an unexpected extraction-index archive path");
    }
    let extraction_index_path = directory.join(extraction_index_relative);
    if sha256_file(&extraction_index_path)? != index.extraction_index_file_sha256 {
        bail!("extraction index archive hash mismatch");
    }
    let artifact_relative = Path::new(&index.artifact_file);
    validate_safe_relative_path(artifact_relative, "private")?;
    if index.artifact_file != ARTIFACT_ARCHIVE_FILE {
        bail!("release index uses an unexpected source-artifact archive path");
    }
    let artifact_path = directory.join(artifact_relative);
    if sha256_file(&artifact_path)? != index.artifact_file_sha256
        || index.artifact_file_sha256 != index.artifact_sha256
    {
        bail!("source-artifact archive hash mismatch");
    }
    let verified = load_verified_extraction(&extraction_index_path, &artifact_path)?;
    if verified.extraction_index_sha256 != index.extraction_index_file_sha256
        || indexed_hash(&verified.index, SOURCE_METADATA_RELATIVE)?
            != index.source_metadata_file_sha256
        || indexed_hash(&verified.index, SOURCE_OVERRIDES_RELATIVE)?
            != index.source_overrides_file_sha256
        || indexed_hash(&verified.index, TRANSFORMATION_AUDIT_RELATIVE)?
            != index.source_transformations_file_sha256
        || indexed_hash(&verified.index, PARSER_SOURCE_RELATIVE)? != index.parser_source_file_sha256
    {
        bail!("release index private extraction hashes do not match the archived index");
    }

    let reconciliation_relative = Path::new(&index.reconciliation_file);
    validate_safe_relative_path(reconciliation_relative, "private")?;
    if index.reconciliation_file != RECONCILIATION_ARCHIVE_FILE {
        bail!("release index uses an unexpected reconciliation archive path");
    }
    let reconciliation_path = directory.join(reconciliation_relative);
    if sha256_file(&reconciliation_path)? != index.reconciliation_file_sha256 {
        bail!("reconciliation archive file hash mismatch");
    }
    let reconciliation = &verified.document;
    if canonical_value_sha256(&reconciliation)? != index.reconciliation_sha256
        || reconciliation.summary.artifact_sha256 != index.artifact_sha256
    {
        bail!("reconciliation archive content mismatch");
    }

    let manifest_path = directory.join("manifest.json");
    let manifest: MineralDatasetManifest = read_json(&manifest_path)?;
    if manifest.schema_version != MINERAL_INGESTION_SCHEMA_VERSION
        || manifest.dataset.key != IMA_DATASET_KEY
        || manifest.dataset.title != IMA_DATASET_TITLE
        || manifest.source.key != IMA_SOURCE_KEY
        || manifest.source.license_spdx != IMA_SOURCE_LICENSE
        || manifest.policy != MineralIngestionPolicy::ImaIdentityV1
        || manifest.snapshot_kind != MineralSnapshotKind::Complete
        || manifest.parser.name != IMA_ADAPTER_NAME
        || manifest.parser.version != IMA_ADAPTER_VERSION
    {
        bail!("manifest is not a strict official IMA complete-snapshot manifest");
    }
    let attribution = manifest
        .source
        .attribution
        .as_ref()
        .context("official IMA manifest is missing source attribution")?;
    let source_metadata = &verified.source_metadata;
    if attribution.attribution_party != source_metadata.attribution.creator
        || attribution.work_title != source_metadata.attribution.source_title
        || attribution.work_url != source_metadata.artifact.url
        || attribution.license_url != source_metadata.attribution.license_url
        || attribution.changes_notice != source_metadata.attribution.changes_notice
        || attribution.no_endorsement_notice != source_metadata.attribution.no_endorsement_notice
        || attribution.derived_output_license_spdx
            != source_metadata.attribution.derived_output_license_spdx
        || manifest.source.url != source_metadata.landing_page
        || manifest.artifact.url != source_metadata.artifact.url
        || manifest.retrieval.retrieved_at != source_metadata.retrieved_at
        || manifest.parser.code_revision != index.parser_source_file_sha256
    {
        bail!("official IMA manifest carries unexpected source attribution");
    }
    validate_http_url("manifest source URL", &manifest.source.url)?;
    validate_http_url("manifest artifact URL", &manifest.artifact.url)?;
    validate_sha256(
        "manifest parser configuration_sha256",
        &manifest.parser.configuration_sha256,
    )?;
    if let Some(base) = manifest.base_batch_id.as_deref() {
        validate_batch_id(base)?;
    }
    let parser_configuration = serde_json::json!({
        "chunk_size": index.chunk_size,
        "dataset_key": IMA_DATASET_KEY,
        "extraction_index_sha256": index.extraction_index_file_sha256,
        "identity_ledger_sha256": index.identity_ledger_sha256,
        "ima_number_rule": "ascii_yyyy_nnn_optional_letter_v1",
        "official_fact_mapping": "ima_master_list_context_v1",
        "override_sha256": index.source_overrides_file_sha256,
        "parser_source_sha256": index.parser_source_file_sha256,
        "reconciliation_sha256": index.reconciliation_sha256,
        "source_key": IMA_SOURCE_KEY,
        "source_transformation_sha256": index.source_transformations_file_sha256,
        "status_mapping": "ima_master_list_status_v1_a_question_to_uncertain",
    });
    if sha256_bytes(&canonical_json_bytes(&parser_configuration)?)
        != manifest.parser.configuration_sha256
    {
        bail!("manifest parser configuration hash cannot be reproduced");
    }
    let expected_release_version = release_version(&reconciliation.summary.release_label)?;
    validate_release_dates(
        &expected_release_version,
        &manifest.release.released_at,
        &manifest.retrieval.retrieved_at,
    )?;
    if manifest.release.version != expected_release_version
        || manifest.expected_record_count != reconciliation.rows.len()
        || manifest.expected_record_count == 0
    {
        bail!("manifest release identity or record count does not match reconciliation");
    }
    if canonical_mineral_manifest_hash(&manifest)? != index.manifest_sha256 {
        bail!("canonical manifest hash mismatch");
    }
    if sha256_file(&manifest_path)? != index.manifest_file_sha256 {
        bail!("manifest file hash mismatch");
    }
    if manifest.artifact.sha256 != index.artifact_sha256
        || manifest.records_sha256 != index.records_sha256
        || manifest.expected_chunk_count != index.chunks.len()
    {
        bail!("manifest and release index are inconsistent");
    }

    let mut all_items = Vec::with_capacity(manifest.expected_record_count);
    for (expected_index, metadata) in index.chunks.iter().enumerate() {
        let relative = Path::new(&metadata.file);
        validate_safe_relative_path(relative, "chunks")?;
        let expected_file = format!("chunks/chunk-{expected_index:05}.json");
        if metadata.file != expected_file {
            bail!("release chunk {expected_index} uses an unexpected archive path");
        }
        if metadata.chunk_index != expected_index {
            bail!("release index contains non-contiguous chunk indexes");
        }
        let path = directory.join(relative);
        let chunk: MineralIngestionChunk = read_json(&path)?;
        if chunk.chunk_index != expected_index
            || chunk.schema_version != MINERAL_INGESTION_SCHEMA_VERSION
            || chunk.items.is_empty()
            || chunk.items.len() > index.chunk_size
            || (expected_index + 1 < index.chunks.len() && chunk.items.len() != index.chunk_size)
            || chunk.items.len() != metadata.item_count
        {
            bail!("invalid release chunk {}", path.display());
        }
        if canonical_mineral_chunk_hash(&chunk)? != metadata.content_sha256
            || sha256_file(&path)? != metadata.file_sha256
        {
            bail!("release chunk hash mismatch at {}", path.display());
        }
        all_items.extend(chunk.items);
    }
    if all_items.len() != manifest.expected_record_count
        || canonical_mineral_records_hash(&all_items)? != index.records_sha256
    {
        bail!("release records do not match the manifest");
    }
    if !all_items
        .windows(2)
        .all(|pair| pair[0].source_record_id < pair[1].source_record_id)
        || all_items
            .iter()
            .any(|item| !is_opaque_source_id(&item.source_record_id))
    {
        bail!("release records are not uniquely sorted opaque source identities");
    }

    let audit_relative = Path::new(&index.audit_file);
    validate_safe_relative_path(audit_relative, "private")?;
    if index.audit_file != AUDIT_ARCHIVE_FILE {
        bail!("release index uses an unexpected audit archive path");
    }
    let audit_bytes = fs::read(directory.join(audit_relative))?;
    if sha256_bytes(&audit_bytes) != index.audit_file_sha256 {
        bail!("private audit file hash mismatch");
    }
    let audit_rows = audit_bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(serde_json::from_slice::<ImaAuditRow>)
        .collect::<serde_json::Result<Vec<_>>>()
        .context("private audit JSONL is invalid")?;
    if audit_rows.len() != all_items.len() {
        bail!("private audit row count does not match release records");
    }
    let audit_ids = audit_rows
        .iter()
        .map(|row| row.source_record_id.as_str())
        .collect::<HashSet<_>>();
    if all_items
        .iter()
        .any(|item| !audit_ids.contains(item.source_record_id.as_str()))
    {
        bail!("a release record has no private audit row");
    }

    let ledger_relative = Path::new(&index.identity_ledger_file);
    validate_safe_relative_path(ledger_relative, "private")?;
    if index.identity_ledger_file != IDENTITY_LEDGER_ARCHIVE_FILE {
        bail!("release index uses an unexpected identity-ledger archive path");
    }
    let ledger_path = directory.join(ledger_relative);
    if sha256_file(&ledger_path)? != index.identity_ledger_file_sha256 {
        bail!("identity ledger archive file hash mismatch");
    }
    let ledger: ImaIdentityLedger = read_json(&ledger_path)?;
    validate_identity_ledger(&ledger)?;
    if canonical_value_sha256(&ledger)? != index.identity_ledger_sha256 {
        bail!("identity ledger canonical hash mismatch");
    }
    let resolved = resolve_release_rows(reconciliation, &ledger)?;
    let mut expected_items = resolved
        .iter()
        .map(|row| build_ingestion_item(reconciliation, row))
        .collect::<Result<Vec<_>>>()?;
    expected_items.sort_by(|left, right| left.source_record_id.cmp(&right.source_record_id));
    if canonical_mineral_records_hash(&expected_items)? != index.records_sha256 {
        bail!("release records cannot be reproduced from reconciliation and ledger");
    }
    if audit_json_lines(reconciliation, &resolved)? != audit_bytes {
        bail!("private audit cannot be reproduced from reconciliation and ledger");
    }
    Ok(index)
}

pub async fn stage_release_bundle(
    directory: &Path,
    server: &str,
    token: &str,
) -> Result<ImaStageOutcome> {
    let index = verify_release_bundle(directory)?;
    if token.chars().count() < 32 || token.trim() != token {
        bail!("staging token must be an unpadded 32-plus-character secret");
    }
    let server = validate_server_url(server)?;
    let mut client_builder = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::none());
    if server.direct_loopback_http {
        client_builder = client_builder.no_proxy();
    }
    let client = client_builder
        .build()
        .context("failed to initialize staging client")?;
    let manifest_bytes = fs::read(directory.join("manifest.json"))?;
    let create_url = format!("{}/admin/ingestion/batches", server.base_url);
    let mut status = send_json(
        client
            .post(&create_url)
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(manifest_bytes),
    )
    .await?;
    validate_mutation_response(&status, &index)?;
    ensure_stage_response_is_successful(&status)?;
    if status.status == "receiving" {
        for metadata in &index.chunks {
            let body = fs::read(directory.join(&metadata.file))?;
            let url = format!(
                "{}/admin/ingestion/batches/{}/chunks/{}",
                server.base_url, status.batch_id, metadata.chunk_index
            );
            let response = client
                .put(url)
                .bearer_auth(token)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .header("X-Content-SHA256", &metadata.content_sha256)
                .body(body)
                .send()
                .await
                .context("failed to stage IMA release chunk")?;
            ensure_success(response, "stage IMA release chunk").await?;
        }
        let finalize_url = format!(
            "{}/admin/ingestion/batches/{}/finalize",
            server.base_url, status.batch_id
        );
        status = send_json(
            client
                .post(finalize_url)
                .bearer_auth(token)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body("{}"),
        )
        .await?;
        validate_mutation_response(&status, &index)?;
        ensure_stage_response_is_successful(&status)?;
        if status.status == "receiving" {
            bail!("server left the complete IMA batch in receiving state after finalization");
        }
    }
    Ok(status.into())
}

fn validate_mutation_response(
    response: &IngestionMutationResponse,
    index: &ImaReleaseBundleIndex,
) -> Result<()> {
    validate_batch_id(&response.batch_id)?;
    validate_sha256("server manifest_hash", &response.manifest_hash)?;
    if let Some(report_hash) = response.report_hash.as_deref() {
        validate_sha256("server report_hash", report_hash)?;
    }
    let expected_batch_id = format!(
        "batch_{}",
        index
            .manifest_sha256
            .strip_prefix("sha256:")
            .context("bundle manifest hash is malformed")?
    );
    let expected_record_count = index
        .chunks
        .iter()
        .map(|chunk| chunk.item_count)
        .sum::<usize>();
    if response.batch_id != expected_batch_id
        || response.manifest_hash != index.manifest_sha256
        || response.expected_chunk_count != index.chunks.len()
        || response.expected_record_count != expected_record_count
        || response.received_chunk_count > response.expected_chunk_count
        || response.received_record_count > response.expected_record_count
        || !is_possible_received_count(
            &index
                .chunks
                .iter()
                .map(|chunk| chunk.item_count)
                .collect::<Vec<_>>(),
            response.received_chunk_count,
            response.received_record_count,
        )
        || !matches!(
            response.status.as_str(),
            "receiving" | "ready" | "needs_attention" | "approved" | "rejected"
        )
    {
        bail!("server returned an inconsistent IMA staging response");
    }
    validate_mutation_status(response)?;
    Ok(())
}

fn is_possible_received_count(
    chunk_item_counts: &[usize],
    received_chunk_count: usize,
    received_record_count: usize,
) -> bool {
    if received_chunk_count > chunk_item_counts.len() {
        return false;
    }
    let mut possible = vec![HashSet::new(); received_chunk_count + 1];
    possible[0].insert(0usize);
    for &item_count in chunk_item_counts {
        for count in (0..received_chunk_count).rev() {
            let additions = possible[count]
                .iter()
                .filter_map(|value| value.checked_add(item_count))
                .collect::<Vec<_>>();
            possible[count + 1].extend(additions);
        }
    }
    possible[received_chunk_count].contains(&received_record_count)
}

fn validate_mutation_status(response: &IngestionMutationResponse) -> Result<()> {
    let counts_complete = response.received_chunk_count == response.expected_chunk_count
        && response.received_record_count == response.expected_record_count;
    match response.status.as_str() {
        "receiving" if response.report_hash.is_some() => {
            bail!("server returned a report hash for a receiving IMA batch")
        }
        "ready" | "needs_attention" | "approved"
            if response.report_hash.is_none() || !counts_complete =>
        {
            bail!("server returned an incomplete finalized IMA batch")
        }
        // An abandoned receiving batch is compacted directly to rejected and has
        // no report. A reviewer-rejected finalized batch has both a report and
        // complete counts.
        "rejected" if response.report_hash.is_some() && !counts_complete => {
            bail!("server returned an incomplete reviewed IMA batch")
        }
        _ => {}
    }
    Ok(())
}

fn ensure_stage_response_is_successful(response: &IngestionMutationResponse) -> Result<()> {
    if response.status == "rejected" && response.report_hash.is_none() {
        bail!(
            "server reports that the IMA staging batch was rejected before finalization (it may have expired while abandoned)"
        );
    }
    Ok(())
}

pub fn staging_endpoint_suffixes(batch_id: &str, chunk_count: usize) -> Vec<String> {
    let mut endpoints = vec!["/admin/ingestion/batches".to_string()];
    endpoints.extend(
        (0..chunk_count).map(|index| format!("/admin/ingestion/batches/{batch_id}/chunks/{index}")),
    );
    endpoints.push(format!("/admin/ingestion/batches/{batch_id}/finalize"));
    endpoints
}

async fn send_json(builder: reqwest::RequestBuilder) -> Result<IngestionMutationResponse> {
    let response = builder
        .send()
        .await
        .context("failed to send IMA ingestion request")?;
    let response = ensure_success(response, "IMA ingestion request").await?;
    response
        .json::<IngestionMutationResponse>()
        .await
        .context("IMA ingestion server returned invalid JSON")
}

async fn ensure_success(response: reqwest::Response, operation: &str) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let bounded = body.chars().take(500).collect::<String>();
    bail!("{operation} failed with HTTP {status}: {bounded}")
}

struct ResolvedReleaseRow<'a> {
    row: &'a ImaExtractedRow,
    identity: &'a ImaIdentityEntry,
}

fn resolve_release_rows<'a>(
    document: &'a ImaReconciledDocument,
    ledger: &'a ImaIdentityLedger,
) -> Result<Vec<ResolvedReleaseRow<'a>>> {
    let mut by_number = HashMap::new();
    let mut by_name = HashMap::new();
    for entry in &ledger.entries {
        for number in &entry.ima_numbers {
            if by_number
                .insert(number.to_ascii_lowercase(), entry)
                .is_some()
            {
                bail!("ledger contains duplicate IMA number '{number}'");
            }
        }
        for name in &entry.canonical_names {
            if by_name
                .insert(normalize_identity_name(name), entry)
                .is_some()
            {
                bail!("ledger contains duplicate canonical name '{name}'");
            }
        }
    }

    let mut identities = HashSet::new();
    let mut resolved = Vec::with_capacity(document.rows.len());
    for row in &document.rows {
        let number_entry = official_ima_number(&row.ima_number_year)
            .and_then(|number| by_number.get(&number).copied());
        let name_entry = by_name
            .get(&normalize_identity_name(&row.canonical_name))
            .copied();
        let identity = match (number_entry, name_entry) {
            (Some(left), Some(right)) if left.source_record_id != right.source_record_id => bail!(
                "row '{}' resolves to different identities by name and IMA number",
                row.canonical_name
            ),
            (Some(entry), _) | (_, Some(entry)) => entry,
            (None, None) => bail!(
                "row '{}' has no stable identity in the fixed ledger",
                row.canonical_name
            ),
        };
        if !identities.insert(identity.source_record_id.as_str()) {
            bail!(
                "more than one release row resolves to '{}'",
                identity.source_record_id
            );
        }
        resolved.push(ResolvedReleaseRow { row, identity });
    }
    Ok(resolved)
}

fn build_ingestion_item(
    document: &ImaReconciledDocument,
    resolved: &ResolvedReleaseRow<'_>,
) -> Result<MineralIngestionItem> {
    let mapping = map_ima_status(&resolved.row.raw_status)?;
    let mut official_identifiers = BTreeMap::new();
    if let Some(number) = official_ima_number(&resolved.row.ima_number_year) {
        official_identifiers.insert("ima_number".to_string(), number);
    }
    let mut synonyms = resolved
        .identity
        .canonical_names
        .iter()
        .filter(|name| {
            normalize_identity_name(name) != normalize_identity_name(&resolved.row.canonical_name)
        })
        .cloned()
        .collect::<Vec<_>>();
    synonyms.sort_by_key(|name| normalize_identity_name(name));
    Ok(MineralIngestionItem {
        source_record_id: resolved.identity.source_record_id.clone(),
        source_locator: Some(source_locator(
            &document.summary.artifact_sha256,
            resolved.row,
        )),
        slug: resolved.identity.slug.clone(),
        canonical_name: resolved.row.canonical_name.clone(),
        formula: resolved.row.formula.clone(),
        nomenclature_status: mapping.nomenclature_status.to_string(),
        is_valid_species: mapping.is_valid_species,
        official_identifiers,
        synonyms,
        official_facts: MineralOfficialFacts {
            discovery_country: resolved.row.country.clone(),
            first_reference: resolved.row.first_reference.clone(),
            second_reference: resolved.row.second_reference.clone(),
            source_status: resolved.row.raw_status.clone(),
        },
    })
}

fn audit_json_lines(
    document: &ImaReconciledDocument,
    resolved: &[ResolvedReleaseRow<'_>],
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for value in resolved {
        let raw_kind = classify_ima_number_year(&value.row.ima_number_year)?;
        let audit = ImaAuditRow {
            ordinal: value.row.ordinal,
            physical_pdf_page: value.row.page,
            page_row: value.row.page_row,
            bbox: value.row.bbox.clone(),
            source_locator: source_locator(&document.summary.artifact_sha256, value.row),
            source_record_id: value.identity.source_record_id.clone(),
            slug: value.identity.slug.clone(),
            bootstrap_adoption: value.identity.bootstrap_adoption,
            canonical_name: value.row.canonical_name.clone(),
            formula: value.row.formula.clone(),
            raw_status: value.row.raw_status.clone(),
            raw_ima_number_year: value.row.ima_number_year.clone(),
            ima_number_year_kind: raw_kind.to_string(),
            official_ima_number: official_ima_number(&value.row.ima_number_year),
            country: value.row.country.clone(),
            first_reference: value.row.first_reference.clone(),
            second_reference: value.row.second_reference.clone(),
        };
        bytes.extend_from_slice(&serde_json::to_vec(&audit)?);
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn validate_extraction_index_contract(index: &ImaExtractionIndex) -> Result<()> {
    if index.format != IMA_EXTRACTION_INDEX_FORMAT
        || index.reconciliation_format != IMA_RECONCILIATION_FORMAT
    {
        bail!("unsupported extraction-index or reconciliation format");
    }
    if index.artifact.sha256 != IMA_EXPECTED_ARTIFACT_SHA256
        || index.artifact.bytes != IMA_EXPECTED_ARTIFACT_BYTES
        || index.artifact.page_count != 243
    {
        bail!("extraction index is not bound to the reviewed July 2026 IMA PDF");
    }
    if index.policies.normalization != IMA_NORMALIZATION_POLICY
        || index.policies.override_review != IMA_OVERRIDE_REVIEW_POLICY
        || index.runtime.python != "3.12.13"
        || index.runtime.pdfplumber != "0.11.9"
        || index.runtime.pymupdf != "1.28.2"
    {
        bail!("extraction index runtime or review policy changed");
    }
    let expected_packages = BTreeMap::from([
        ("Pillow".to_string(), "12.3.0".to_string()),
        ("PyMuPDF".to_string(), "1.28.2".to_string()),
        ("cffi".to_string(), "2.1.1".to_string()),
        ("charset-normalizer".to_string(), "3.4.9".to_string()),
        ("cryptography".to_string(), "50.0.0".to_string()),
        ("pdfminer.six".to_string(), "20251230".to_string()),
        ("pdfplumber".to_string(), "0.11.9".to_string()),
        ("pycparser".to_string(), "3.0".to_string()),
        ("pypdfium2".to_string(), "5.12.1".to_string()),
    ]);
    if index.runtime.pinned_packages != expected_packages
        || index.counts.extractor_versions.python != index.runtime.python
        || index.counts.extractor_versions.pdfplumber != index.runtime.pdfplumber
        || index.counts.extractor_versions.pymupdf != index.runtime.pymupdf
        || index.counts.normalization_policy != index.policies.normalization
        || index.counts.override_review_policy != index.policies.override_review
    {
        bail!("extraction index runtime does not match its reconciliation counts");
    }
    if index.files.len() != INDEXED_EXTRACTION_FILES.len() {
        bail!("extraction index must list the exact reviewed 12-file archive");
    }
    for (relative, role) in INDEXED_EXTRACTION_FILES {
        let metadata = index
            .files
            .get(relative)
            .with_context(|| format!("extraction index is missing {relative}"))?;
        if metadata.role != role || metadata.bytes == 0 {
            bail!("indexed file {relative} has an unexpected role or empty content");
        }
        validate_sha256("indexed file sha256", &metadata.sha256)?;
    }
    if indexed_hash(index, SOURCE_OVERRIDES_RELATIVE)? != IMA_EXPECTED_OVERRIDE_SHA256 {
        bail!("reviewed extraction override file hash changed");
    }
    if indexed_hash(index, SOURCE_METADATA_RELATIVE)? != IMA_EXPECTED_SOURCE_METADATA_SHA256
        || indexed_hash(index, PARSER_SOURCE_RELATIVE)? != IMA_EXPECTED_PARSER_SOURCE_SHA256
        || indexed_hash(index, RECONCILED_RELATIVE)? != IMA_EXPECTED_RECONCILED_FILE_SHA256
        || indexed_hash(index, RECONCILIATION_REPORT_RELATIVE)?
            != IMA_EXPECTED_RECONCILIATION_REPORT_SHA256
    {
        bail!("an audited official extraction snapshot hash changed");
    }
    Ok(())
}

fn validate_indexed_extraction_files(
    root: &Path,
    index_path: &Path,
    index: &ImaExtractionIndex,
) -> Result<()> {
    let mut actual_files = BTreeMap::<String, PathBuf>::new();
    collect_regular_files(root, root, &mut actual_files)?;
    actual_files.remove(EXTRACTION_INDEX_NAME);
    let expected = index.files.keys().cloned().collect::<HashSet<_>>();
    let actual = actual_files.keys().cloned().collect::<HashSet<_>>();
    if actual != expected {
        let missing = expected.difference(&actual).cloned().collect::<Vec<_>>();
        let unknown = actual.difference(&expected).cloned().collect::<Vec<_>>();
        bail!("extraction archive file set differs: missing={missing:?}, unknown={unknown:?}");
    }
    if !index_path.starts_with(root) {
        bail!("extraction index is outside its archive root");
    }
    for (relative, metadata) in &index.files {
        validate_index_relative_path(relative)?;
        let path = actual_files
            .get(relative)
            .with_context(|| format!("indexed path {relative} is absent"))?;
        let file_metadata = fs::metadata(path)
            .with_context(|| format!("failed to inspect indexed file {}", path.display()))?;
        if file_metadata.len() != metadata.bytes || sha256_file(path)? != metadata.sha256 {
            bail!("indexed file changed: {relative}");
        }
    }
    Ok(())
}

fn collect_regular_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, PathBuf>,
) -> Result<()> {
    let directory_metadata = fs::symlink_metadata(directory)
        .with_context(|| format!("failed to inspect {}", directory.display()))?;
    if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
        bail!("extraction archive contains a symlink or non-directory");
    }
    for entry in fs::read_dir(directory)
        .with_context(|| format!("failed to enumerate {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            bail!("extraction archive contains symlink {}", path.display());
        }
        if metadata.is_dir() {
            collect_regular_files(root, &path, files)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .context("archive traversal escaped its root")?;
            let relative = relative_path_to_posix(relative)?;
            if files.insert(relative.clone(), path).is_some() {
                bail!("duplicate extraction archive path {relative}");
            }
        } else {
            bail!(
                "extraction archive contains non-regular entry {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn relative_path_to_posix(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(
                value
                    .to_str()
                    .context("extraction archive path is not UTF-8")?,
            ),
            _ => bail!("extraction archive contains a non-normal relative path"),
        }
    }
    Ok(parts.join("/"))
}

fn validate_index_relative_path(value: &str) -> Result<()> {
    if value.is_empty() || value.contains('\\') || value.starts_with('/') {
        bail!("invalid indexed relative path '{value}'");
    }
    let path = Path::new(value);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("invalid indexed relative path '{value}'");
    }
    Ok(())
}

fn reject_symlink_or_non_file(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {label} {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("{label} must be a regular, non-symlink file");
    }
    Ok(())
}

fn indexed_hash<'a>(index: &'a ImaExtractionIndex, relative: &str) -> Result<&'a str> {
    Ok(index
        .files
        .get(relative)
        .with_context(|| format!("indexed file {relative} is missing"))?
        .sha256
        .as_str())
}

fn load_source_metadata(path: &Path) -> Result<ImaSourceMetadata> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read source metadata {}", path.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid source metadata JSON in {}", path.display()))?;
    require_exact_json_keys(
        &value,
        &[
            "artifact",
            "attribution",
            "dataset_key",
            "format",
            "landing_page",
            "retrieved_at",
            "source_key",
        ],
        "source metadata",
    )?;
    require_exact_json_keys(
        value
            .get("artifact")
            .context("source metadata artifact is missing")?,
        &[
            "bytes",
            "content_type",
            "etag",
            "last_modified",
            "sha256",
            "url",
        ],
        "source metadata artifact",
    )?;
    require_exact_json_keys(
        value
            .get("attribution")
            .context("source metadata attribution is missing")?,
        &[
            "changes_notice",
            "creator",
            "derived_output_license_spdx",
            "license_spdx",
            "license_url",
            "no_endorsement_notice",
            "source_title",
        ],
        "source metadata attribution",
    )?;
    serde_json::from_value(value).context("source metadata does not match its strict schema")
}

fn require_exact_json_keys(value: &Value, expected: &[&str], label: &str) -> Result<()> {
    let object = value
        .as_object()
        .with_context(|| format!("{label} must be an object"))?;
    let expected = expected.iter().copied().collect::<HashSet<_>>();
    let actual = object.keys().map(String::as_str).collect::<HashSet<_>>();
    if actual != expected {
        bail!("{label} keys do not match the strict extraction contract");
    }
    Ok(())
}

fn validate_source_metadata(
    metadata: &ImaSourceMetadata,
    index: &ImaExtractionIndex,
) -> Result<()> {
    if metadata.format != "waajacu-source-artifact-v1"
        || metadata.dataset_key != IMA_DATASET_KEY
        || metadata.source_key != IMA_SOURCE_KEY
        || metadata.artifact.sha256 != index.artifact.sha256
        || metadata.artifact.bytes != index.artifact.bytes
        || metadata.artifact.content_type != "application/pdf"
        || metadata.attribution.license_spdx != IMA_SOURCE_LICENSE
        || metadata.attribution.derived_output_license_spdx != IMA_SOURCE_LICENSE
        || metadata.attribution.license_url != IMA_LICENSE_URL
        || metadata.attribution.creator != IMA_ATTRIBUTION_PARTY
    {
        bail!("source metadata does not describe the indexed official IMA artifact");
    }
    validate_http_url("source landing page", &metadata.landing_page)?;
    validate_http_url("source artifact URL", &metadata.artifact.url)?;
    DateTime::parse_from_rfc3339(&metadata.retrieved_at)
        .context("source metadata retrieved_at must be RFC 3339")?;
    for (label, value) in [
        ("attribution creator", metadata.attribution.creator.as_str()),
        ("source title", metadata.attribution.source_title.as_str()),
        (
            "changes notice",
            metadata.attribution.changes_notice.as_str(),
        ),
        (
            "no-endorsement notice",
            metadata.attribution.no_endorsement_notice.as_str(),
        ),
    ] {
        validate_source_text(label, value, false)?;
    }
    for (label, value) in [
        ("ETag", metadata.artifact.etag.as_deref()),
        ("Last-Modified", metadata.artifact.last_modified.as_deref()),
    ] {
        if let Some(value) = value {
            validate_source_text(label, value, false)?;
        }
    }
    Ok(())
}

fn validate_extraction_overrides(
    overrides: &ImaExtractionOverrides,
    index: &ImaExtractionIndex,
) -> Result<()> {
    if overrides.format != "waajacu-ima-extraction-overrides-v1"
        || overrides.artifact_sha256 != index.artifact.sha256
        || overrides.review_policy != IMA_OVERRIDE_REVIEW_POLICY
        || overrides.whitespace_resolutions.len() != 90
        || overrides.formula_transformations.len() != 6
    {
        bail!("reviewed extraction overrides do not match the official contract");
    }
    let mut whitespace_keys = HashSet::new();
    for event in &overrides.whitespace_resolutions {
        if !SOURCE_COLUMNS.contains(&event.field.as_str())
            || event.ordinal == 0
            || event.page < 3
            || event.page_row == 0
            || !whitespace_keys.insert((event.ordinal, event.field.as_str()))
        {
            bail!("invalid or duplicate reviewed whitespace resolution");
        }
        for value in [&event.pdfplumber, &event.pymupdf, &event.resolved] {
            if normalize_extractor_text(value)? != *value {
                bail!("reviewed whitespace resolution is not normalized");
            }
        }
        if event.pdfplumber == event.pymupdf
            || whitespace_signature(&event.pdfplumber) != whitespace_signature(&event.pymupdf)
            || whitespace_signature(&event.pdfplumber) != whitespace_signature(&event.resolved)
        {
            bail!("reviewed whitespace resolution changes non-whitespace content");
        }
    }
    let mut transformation_ordinals = HashSet::new();
    for event in &overrides.formula_transformations {
        if event.ordinal == 0
            || event.page < 3
            || event.page_row == 0
            || !transformation_ordinals.insert(event.ordinal)
            || event.replacements.is_empty()
            || normalize_extractor_text(&event.raw_formula)? != event.raw_formula
            || normalize_extractor_text(&event.resolved_formula)? != event.resolved_formula
        {
            bail!("invalid or duplicate reviewed formula transformation");
        }
        let mut formula = event.raw_formula.clone();
        for replacement in &event.replacements {
            if replacement.count == 0
                || replacement.from.chars().count() != 1
                || replacement.to.chars().count() != 1
                || !replacement.from.chars().all(is_forbidden_formula_codepoint)
                || replacement.to.chars().any(is_forbidden_formula_codepoint)
                || formula.matches(&replacement.from).count() != replacement.count
            {
                bail!("reviewed formula transformation has an unsafe operation");
            }
            formula = formula.replace(&replacement.from, &replacement.to);
        }
        if normalize_extractor_text(&formula)? != event.resolved_formula
            || event
                .resolved_formula
                .chars()
                .any(is_forbidden_formula_codepoint)
        {
            bail!("reviewed formula transformation does not produce its resolution");
        }
    }
    Ok(())
}

fn whitespace_signature(value: &str) -> String {
    value.split_whitespace().collect()
}

fn verify_reconciliation_report(
    root: &Path,
    document: &ImaReconciledDocument,
    index: &ImaExtractionIndex,
) -> Result<()> {
    let path = root.join(RECONCILIATION_REPORT_RELATIVE);
    let mut value: Value = read_json(&path)?;
    let object = value
        .as_object_mut()
        .context("reconciliation report must be an object")?;
    let normalized_hashes: BTreeMap<String, String> = serde_json::from_value(
        object
            .remove("engine_normalized_stream_sha256")
            .context("reconciliation report lacks normalized stream hashes")?,
    )?;
    let raw_hashes: BTreeMap<String, String> = serde_json::from_value(
        object
            .remove("engine_raw_stream_sha256")
            .context("reconciliation report lacks raw stream hashes")?,
    )?;
    let overrides_sha256: String = serde_json::from_value(
        object
            .remove("overrides_sha256")
            .context("reconciliation report lacks overrides_sha256")?,
    )?;
    let source_metadata_sha256: String = serde_json::from_value(
        object
            .remove("source_metadata_sha256")
            .context("reconciliation report lacks source_metadata_sha256")?,
    )?;
    let summary: ImaReconciliationSummary = serde_json::from_value(value)?;
    if summary != document.summary
        || normalized_hashes
            != BTreeMap::from([
                (
                    "pdfplumber".to_string(),
                    indexed_hash(index, PDFPLUMBER_NORMALIZED_RELATIVE)?.to_string(),
                ),
                (
                    "pymupdf".to_string(),
                    indexed_hash(index, PYMUPDF_NORMALIZED_RELATIVE)?.to_string(),
                ),
            ])
        || raw_hashes
            != BTreeMap::from([
                (
                    "pdfplumber".to_string(),
                    indexed_hash(index, PDFPLUMBER_RAW_RELATIVE)?.to_string(),
                ),
                (
                    "pymupdf".to_string(),
                    indexed_hash(index, PYMUPDF_RAW_RELATIVE)?.to_string(),
                ),
            ])
        || overrides_sha256 != indexed_hash(index, SOURCE_OVERRIDES_RELATIVE)?
        || source_metadata_sha256 != indexed_hash(index, SOURCE_METADATA_RELATIVE)?
    {
        bail!("reconciliation report cannot be reproduced from indexed inputs");
    }
    Ok(())
}

fn recompute_reconciled_rows(
    root: &Path,
    document: &ImaReconciledDocument,
    overrides: &ImaExtractionOverrides,
) -> Result<()> {
    let pdfplumber_raw: Vec<ImaRawExtractedRow> =
        read_json_lines(&root.join(PDFPLUMBER_RAW_RELATIVE))?;
    let pdfplumber_normalized: Vec<ImaExtractedRow> =
        read_json_lines(&root.join(PDFPLUMBER_NORMALIZED_RELATIVE))?;
    let pymupdf_raw: Vec<ImaRawExtractedRow> = read_json_lines(&root.join(PYMUPDF_RAW_RELATIVE))?;
    let pymupdf_normalized: Vec<ImaExtractedRow> =
        read_json_lines(&root.join(PYMUPDF_NORMALIZED_RELATIVE))?;
    verify_engine_normalization("pdfplumber", &pdfplumber_raw, &pdfplumber_normalized)?;
    verify_engine_normalization("pymupdf", &pymupdf_raw, &pymupdf_normalized)?;
    if pdfplumber_normalized.len() != document.summary.total_rows
        || pymupdf_normalized.len() != document.summary.total_rows
    {
        bail!("indexed engine streams do not have the reviewed row count");
    }

    let whitespace_by_key = overrides
        .whitespace_resolutions
        .iter()
        .map(|event| ((event.ordinal, event.field.as_str()), event))
        .collect::<HashMap<_, _>>();
    let transformations_by_ordinal = overrides
        .formula_transformations
        .iter()
        .map(|event| (event.ordinal, event))
        .collect::<HashMap<_, _>>();
    let mut used_whitespace = HashSet::new();
    let mut used_transformations = HashSet::new();
    let mut reviewed_whitespace = Vec::new();
    let mut reviewed_transformations = Vec::new();
    let mut rows = Vec::with_capacity(pdfplumber_normalized.len());
    for (left, right) in pdfplumber_normalized.iter().zip(&pymupdf_normalized) {
        if left.ordinal != right.ordinal
            || left.page != right.page
            || left.page_row != right.page_row
        {
            bail!(
                "dual extractor row locators disagree at ordinal {}",
                left.ordinal
            );
        }
        let mut row = left.clone();
        for field in SOURCE_COLUMNS {
            let left_value = extracted_field(left, field);
            let right_value = extracted_field(right, field);
            if left_value == right_value {
                continue;
            }
            let key = (left.ordinal, field);
            let resolution = whitespace_by_key.get(&key).with_context(|| {
                format!(
                    "unreviewed extractor disagreement at ordinal {} {field}",
                    left.ordinal
                )
            })?;
            if resolution.page != left.page
                || resolution.page_row != left.page_row
                || resolution.pdfplumber != left_value
                || resolution.pymupdf != right_value
            {
                bail!(
                    "reviewed whitespace locator/value changed at ordinal {}",
                    left.ordinal
                );
            }
            set_extracted_field(&mut row, field, resolution.resolved.clone())?;
            used_whitespace.insert(key);
            reviewed_whitespace.push((*resolution).clone());
        }
        if let Some(transformation) = transformations_by_ordinal.get(&row.ordinal) {
            if transformation.page != row.page
                || transformation.page_row != row.page_row
                || transformation.canonical_name != row.canonical_name
                || transformation.raw_formula != row.formula
            {
                bail!(
                    "reviewed formula transformation changed at ordinal {}",
                    row.ordinal
                );
            }
            let mut formula = row.formula.clone();
            for replacement in &transformation.replacements {
                if formula.matches(&replacement.from).count() != replacement.count {
                    bail!(
                        "formula replacement count changed at ordinal {}",
                        row.ordinal
                    );
                }
                formula = formula.replace(&replacement.from, &replacement.to);
            }
            row.formula = normalize_extractor_text(&formula)?;
            if row.formula != transformation.resolved_formula {
                bail!(
                    "formula transformation result changed at ordinal {}",
                    row.ordinal
                );
            }
            used_transformations.insert(row.ordinal);
            reviewed_transformations.push((*transformation).clone());
        }
        rows.push(row);
    }
    if used_whitespace.len() != overrides.whitespace_resolutions.len()
        || used_transformations.len() != overrides.formula_transformations.len()
        || rows != document.rows
    {
        bail!("raw engine streams and reviewed overrides do not reproduce reconciled rows");
    }
    let whitespace_audit: ImaReviewedEvents<ImaWhitespaceResolution> =
        read_json(&root.join(WHITESPACE_AUDIT_RELATIVE))?;
    let transformation_audit: ImaReviewedEvents<ImaFormulaTransformation> =
        read_json(&root.join(TRANSFORMATION_AUDIT_RELATIVE))?;
    if whitespace_audit.artifact_sha256 != document.summary.artifact_sha256
        || whitespace_audit.format != IMA_RECONCILIATION_FORMAT
        || whitespace_audit.review_policy != IMA_OVERRIDE_REVIEW_POLICY
        || whitespace_audit.events != reviewed_whitespace
        || transformation_audit.artifact_sha256 != document.summary.artifact_sha256
        || transformation_audit.format != IMA_RECONCILIATION_FORMAT
        || transformation_audit.review_policy != IMA_OVERRIDE_REVIEW_POLICY
        || transformation_audit.events != reviewed_transformations
    {
        bail!("reviewed audit event archives cannot be reproduced");
    }
    Ok(())
}

fn verify_engine_normalization(
    engine: &str,
    raw_rows: &[ImaRawExtractedRow],
    normalized_rows: &[ImaExtractedRow],
) -> Result<()> {
    if raw_rows.len() != normalized_rows.len() {
        bail!("{engine} raw and normalized streams have different lengths");
    }
    for (index, (raw, expected)) in raw_rows.iter().zip(normalized_rows).enumerate() {
        if raw.ordinal != index + 1
            || raw.ordinal != expected.ordinal
            || raw.page != expected.page
            || raw.page_row != expected.page_row
            || raw.bbox != expected.bbox
            || raw
                .values
                .keys()
                .map(String::as_str)
                .collect::<HashSet<_>>()
                != SOURCE_COLUMNS.into_iter().collect::<HashSet<_>>()
            || raw
                .cell_bboxes
                .keys()
                .map(String::as_str)
                .collect::<HashSet<_>>()
                != SOURCE_COLUMNS.into_iter().collect::<HashSet<_>>()
        {
            bail!(
                "{engine} raw row geometry/schema changed at ordinal {}",
                raw.ordinal
            );
        }
        validate_bbox(&raw.bbox, "raw row")?;
        for bbox in raw.cell_bboxes.values() {
            validate_bbox(bbox, "raw cell")?;
        }
        let rebuilt = ImaExtractedRow {
            ordinal: raw.ordinal,
            page: raw.page,
            page_row: raw.page_row,
            bbox: raw.bbox.clone(),
            canonical_name: normalize_extractor_text(raw_value(raw, "canonical_name")?)?,
            formula: normalize_extractor_text(raw_value(raw, "formula")?)?,
            raw_status: normalize_extractor_text(raw_value(raw, "raw_status")?)?,
            ima_number_year: normalize_extractor_text(raw_value(raw, "ima_number_year")?)?,
            country: normalize_extractor_text(raw_value(raw, "country")?)?,
            first_reference: normalize_extractor_text(raw_value(raw, "first_reference")?)?,
            second_reference: normalize_extractor_text(raw_value(raw, "second_reference")?)?,
        };
        if &rebuilt != expected {
            bail!(
                "{engine} normalized stream cannot be reproduced at ordinal {}",
                raw.ordinal
            );
        }
    }
    Ok(())
}

fn raw_value<'a>(row: &'a ImaRawExtractedRow, field: &str) -> Result<&'a str> {
    Ok(row
        .values
        .get(field)
        .with_context(|| format!("raw row is missing field {field}"))?)
}

fn extracted_field<'a>(row: &'a ImaExtractedRow, field: &str) -> &'a str {
    match field {
        "canonical_name" => &row.canonical_name,
        "formula" => &row.formula,
        "raw_status" => &row.raw_status,
        "ima_number_year" => &row.ima_number_year,
        "country" => &row.country,
        "first_reference" => &row.first_reference,
        "second_reference" => &row.second_reference,
        _ => unreachable!("SOURCE_COLUMNS contains only known fields"),
    }
}

fn set_extracted_field(row: &mut ImaExtractedRow, field: &str, value: String) -> Result<()> {
    match field {
        "canonical_name" => row.canonical_name = value,
        "formula" => row.formula = value,
        "raw_status" => row.raw_status = value,
        "ima_number_year" => row.ima_number_year = value,
        "country" => row.country = value,
        "first_reference" => row.first_reference = value,
        "second_reference" => row.second_reference = value,
        _ => bail!("unsupported source field {field}"),
    }
    Ok(())
}

fn normalize_extractor_text(value: &str) -> Result<String> {
    let normalized = value.nfc().collect::<String>();
    if normalized.contains('\u{fffd}') {
        bail!("source text contains an unresolved replacement glyph");
    }
    let collapsed = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().any(is_control_format_or_surrogate) {
        bail!("source text contains an unresolved control/format glyph");
    }
    Ok(collapsed)
}

fn is_control_format_or_surrogate(character: char) -> bool {
    let value = character as u32;
    character.is_control()
        || matches!(
            value,
            0x00AD
                | 0x0600..=0x0605
                | 0x061C
                | 0x06DD
                | 0x070F
                | 0x0890..=0x0891
                | 0x08E2
                | 0x180E
                | 0x200B..=0x200F
                | 0x202A..=0x202E
                | 0x2060..=0x2064
                | 0x2066..=0x206F
                | 0xFEFF
                | 0xFFF9..=0xFFFB
                | 0x110BD
                | 0x110CD
                | 0x13430..=0x1343F
                | 0x1BCA0..=0x1BCA3
                | 0x1D173..=0x1D17A
                | 0xE0001
                | 0xE0020..=0xE007F
        )
}

fn validate_bbox(bbox: &[f64], label: &str) -> Result<()> {
    if bbox.len() != 4
        || bbox.iter().any(|value| !value.is_finite())
        || bbox[0] >= bbox[2]
        || bbox[1] >= bbox[3]
    {
        bail!("{label} has invalid geometry");
    }
    Ok(())
}

fn read_json_lines<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    if bytes.len() < 2 || !bytes.ends_with(b"\n") {
        bail!("JSONL file {} is not newline terminated", path.display());
    }
    let body = &bytes[..bytes.len() - 1];
    if body
        .split(|byte| *byte == b'\n')
        .any(|line| line.is_empty())
    {
        bail!("JSONL file {} contains an empty row", path.display());
    }
    body.split(|byte| *byte == b'\n')
        .map(|line| {
            serde_json::from_slice(line)
                .with_context(|| format!("invalid JSONL row in {}", path.display()))
        })
        .collect()
}

fn archive_verified_extraction(verified: &ImaVerifiedExtraction, output: &Path) -> Result<()> {
    let extraction_destination = output.join(EXTRACTION_ARCHIVE_DIR);
    for (relative, _) in INDEXED_EXTRACTION_FILES {
        let source = verified.extraction_root.join(relative);
        let destination = extraction_destination.join(Path::new(relative));
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&source, &destination).with_context(|| {
            format!(
                "failed to archive extraction file {} to {}",
                source.display(),
                destination.display()
            )
        })?;
    }
    fs::copy(
        verified.extraction_root.join(EXTRACTION_INDEX_NAME),
        output.join(EXTRACTION_INDEX_ARCHIVE_FILE),
    )?;
    fs::copy(&verified.artifact_path, output.join(ARTIFACT_ARCHIVE_FILE))?;
    Ok(())
}

fn validate_reconciled_document(document: &ImaReconciledDocument) -> Result<()> {
    if document.format != IMA_RECONCILIATION_FORMAT
        || document.summary.format != IMA_RECONCILIATION_FORMAT
    {
        bail!("unsupported IMA reconciliation format");
    }
    validate_sha256("artifact_sha256", &document.summary.artifact_sha256)?;
    if document.summary.license_spdx != IMA_SOURCE_LICENSE {
        bail!(
            "IMA source license must be {IMA_SOURCE_LICENSE}, got '{}'",
            document.summary.license_spdx
        );
    }
    if document.summary.extractor_disagreement_count != 0 {
        bail!("reconciliation contains extractor disagreements");
    }
    if document.summary.table_page_count > document.summary.page_count
        || document
            .summary
            .reviewed_whitespace_resolution_fields
            .values()
            .sum::<usize>()
            != document.summary.reviewed_whitespace_resolution_count
        || document
            .summary
            .source_transformation_fields
            .values()
            .sum::<usize>()
            != document.summary.source_transformation_count
    {
        bail!("reconciliation audit counts are internally inconsistent");
    }
    if document.summary.formula_replacement_glyph_count != 0
        || document.summary.formula_private_use_count != 0
        || document.summary.formula_cyrillic_count != 0
    {
        bail!("reconciliation retains unsafe formula glyphs");
    }
    if document.summary.page_count < 3 {
        bail!("reconciliation has an invalid PDF page count");
    }
    if document.rows.len() != document.summary.total_rows {
        bail!("reconciliation summary row count does not match rows");
    }
    if document.summary.valid_species != document.summary.declared_valid_species {
        bail!("source-declared and extracted valid-species counts differ");
    }
    let mut names = HashSet::new();
    let mut numbers = HashSet::new();
    let mut status_counts = BTreeMap::<String, usize>::new();
    let mut valid_count = 0usize;
    let mut hidden_count = 0usize;
    let mut official_number_count = 0usize;
    let mut source_coordinates = HashSet::new();
    for (index, row) in document.rows.iter().enumerate() {
        if row.ordinal != index + 1
            || row.page < 3
            || row.page > document.summary.page_count
            || row.page_row == 0
        {
            bail!("invalid row coordinates for '{}'", row.canonical_name);
        }
        if !source_coordinates.insert((row.page, row.page_row)) {
            bail!(
                "duplicate source coordinates at PDF page {}, row {}",
                row.page,
                row.page_row
            );
        }
        if row.bbox.len() != 4
            || row.bbox.iter().any(|value| !value.is_finite())
            || row.bbox[0] >= row.bbox[2]
            || row.bbox[1] >= row.bbox[3]
        {
            bail!("invalid row bounding box for '{}'", row.canonical_name);
        }
        validate_source_text("canonical_name", &row.canonical_name, false)?;
        validate_source_text("formula", &row.formula, true)?;
        validate_source_text("raw_status", &row.raw_status, false)?;
        validate_source_text("ima_number_year", &row.ima_number_year, false)?;
        validate_source_text("country", &row.country, true)?;
        validate_source_text("first_reference", &row.first_reference, true)?;
        validate_source_text("second_reference", &row.second_reference, true)?;
        if row.canonical_name.chars().count() > 240 {
            bail!("canonical name exceeds the server's 240-character limit");
        }
        if row.formula.chars().count() > 500 {
            bail!("formula exceeds the server's 500-character limit");
        }
        if let Some(character) = row
            .formula
            .chars()
            .find(|character| is_forbidden_formula_codepoint(*character))
        {
            bail!(
                "formula contains forbidden unresolved codepoint U+{:04X}",
                character as u32
            );
        }
        let mapping = map_ima_status(&row.raw_status)?;
        *status_counts.entry(row.raw_status.clone()).or_default() += 1;
        if mapping.is_valid_species {
            valid_count += 1;
        } else {
            hidden_count += 1;
        }
        classify_ima_number_year(&row.ima_number_year)?;
        if let Some(number) = official_ima_number(&row.ima_number_year) {
            official_number_count += 1;
            if !numbers.insert(number.clone()) {
                bail!("duplicate official IMA number '{number}'");
            }
        }
        if !names.insert(normalize_identity_name(&row.canonical_name)) {
            bail!("duplicate canonical name '{}'", row.canonical_name);
        }
    }
    let missing_formula_count = document
        .rows
        .iter()
        .filter(|row| row.formula.is_empty())
        .count();
    if valid_count != document.summary.valid_species
        || hidden_count != document.summary.hidden_historical_rows
        || official_number_count != document.summary.official_ima_number_count
        || missing_formula_count != document.summary.missing_formula_count
        || status_counts != document.summary.status_counts
    {
        bail!("reconciliation summary counts do not match its rows");
    }
    Ok(())
}

fn validate_official_release_contract(document: &ImaReconciledDocument) -> Result<()> {
    validate_reconciled_document(document)?;
    let expected_status_counts = BTreeMap::from([
        ("A".to_string(), 4_293),
        ("A ?".to_string(), 6),
        ("D".to_string(), 1),
        ("G".to_string(), 1_129),
        ("Q".to_string(), 96),
        ("Rd".to_string(), 413),
        ("Rn".to_string(), 289),
    ]);
    let expected_whitespace_fields = BTreeMap::from([
        ("canonical_name".to_string(), 20),
        ("first_reference".to_string(), 6),
        ("formula".to_string(), 62),
        ("second_reference".to_string(), 2),
    ]);
    let expected_transformation_fields = BTreeMap::from([("formula".to_string(), 6)]);
    if document.summary.artifact_sha256 != IMA_EXPECTED_ARTIFACT_SHA256
        || document.summary.page_count != 243
        || document.summary.table_page_count != 241
        || document.summary.release_label != "July 2026"
        || document.summary.declared_valid_species != 6_226
        || document.summary.total_rows != 6_227
        || document.summary.valid_species != 6_226
        || document.summary.hidden_historical_rows != 1
        || document.summary.status_counts != expected_status_counts
        || document.summary.official_ima_number_count != 4_127
        || document.summary.missing_formula_count != 0
        || document.summary.reviewed_whitespace_resolution_count != 90
        || document.summary.reviewed_whitespace_resolution_fields != expected_whitespace_fields
        || document.summary.extractor_versions.python != "3.12.13"
        || document.summary.extractor_versions.pdfplumber != "0.11.9"
        || document.summary.extractor_versions.pymupdf != "1.28.2"
        || document.summary.normalization_policy != IMA_NORMALIZATION_POLICY
        || document.summary.override_review_policy != IMA_OVERRIDE_REVIEW_POLICY
        || document.summary.source_transformation_count != 6
        || document.summary.source_transformation_fields != expected_transformation_fields
    {
        bail!("reconciliation does not match the reviewed July 2026 release contract");
    }
    let actual_composite_status_rows = document
        .rows
        .iter()
        .filter(|row| row.raw_status == "A ?")
        .map(|row| {
            (
                row.canonical_name.as_str(),
                row.page,
                row.page_row,
                row.ima_number_year.as_str(),
            )
        })
        .collect::<Vec<_>>();
    let expected_composite_status_rows = vec![
        ("Balipholite", 19, 18, "?"),
        ("Calcjarlite", 35, 23, "1973"),
        ("Changbaiite", 41, 9, "?"),
        ("Chelkarite", 42, 1, "1968"),
        ("Cuprostibite", 53, 4, "1969"),
        ("Daomanite", 54, 19, "?"),
    ];
    if actual_composite_status_rows != expected_composite_status_rows {
        bail!("the six reviewed A ? source rows changed coordinates or values");
    }
    let historical_rows = document
        .rows
        .iter()
        .filter(|row| row.raw_status == "D")
        .map(|row| {
            (
                row.canonical_name.as_str(),
                row.page,
                row.page_row,
                row.ima_number_year.as_str(),
            )
        })
        .collect::<Vec<_>>();
    if historical_rows != vec![("Franklinphilite", 77, 24, "1990-050")] {
        bail!("the reviewed discredited historical row changed coordinates or values");
    }
    Ok(())
}

fn validate_identity_ledger(ledger: &ImaIdentityLedger) -> Result<()> {
    if ledger.format != IMA_IDENTITY_LEDGER_FORMAT || ledger.dataset_key != IMA_DATASET_KEY {
        bail!("unsupported IMA identity ledger");
    }
    if let Some(parent) = ledger.parent_sha256.as_deref() {
        validate_sha256("ledger parent_sha256", parent)?;
    }
    match (
        ledger.revision,
        ledger.parent_sha256.is_some(),
        ledger.entries.is_empty(),
    ) {
        (0, false, true) => {}
        (0, _, _) => bail!("identity ledger revision zero must be the empty root ledger"),
        (_, false, _) => bail!("a versioned identity ledger requires parent_sha256"),
        (_, true, false) => {}
        (_, true, true) => bail!("a versioned identity ledger cannot be empty"),
    }
    let mut source_ids = HashSet::new();
    let mut slugs = HashSet::new();
    let mut names = HashSet::new();
    let mut numbers = HashSet::new();
    let mut previous_source_id: Option<&str> = None;
    for entry in &ledger.entries {
        if !is_opaque_source_id(&entry.source_record_id) {
            bail!(
                "invalid opaque source_record_id '{}'",
                entry.source_record_id
            );
        }
        if !source_ids.insert(entry.source_record_id.clone()) {
            bail!("duplicate source_record_id '{}'", entry.source_record_id);
        }
        if previous_source_id.is_some_and(|previous| previous >= entry.source_record_id.as_str()) {
            bail!("identity ledger entries must be strictly sorted by source_record_id");
        }
        previous_source_id = Some(&entry.source_record_id);
        if !is_valid_mineral_slug(&entry.slug) || !slugs.insert(entry.slug.clone()) {
            bail!("invalid or duplicate stable slug '{}'", entry.slug);
        }
        if entry.canonical_names.is_empty() {
            bail!(
                "ledger identity '{}' has no known name",
                entry.source_record_id
            );
        }
        if !entry
            .canonical_names
            .windows(2)
            .all(|pair| normalize_identity_name(&pair[0]) < normalize_identity_name(&pair[1]))
        {
            bail!("ledger canonical names must be uniquely sorted");
        }
        for name in &entry.canonical_names {
            validate_source_text("ledger canonical name", name, false)?;
            if !names.insert(normalize_identity_name(name)) {
                bail!(
                    "ledger canonical name '{}' belongs to multiple identities",
                    name
                );
            }
        }
        for number in &entry.ima_numbers {
            if !is_official_ima_number(number)
                || number != &number.to_ascii_lowercase()
                || !numbers.insert(number.to_ascii_lowercase())
            {
                bail!("invalid or duplicate ledger IMA number '{number}'");
            }
        }
        if !entry.ima_numbers.windows(2).all(|pair| pair[0] < pair[1]) {
            bail!("ledger IMA numbers must be uniquely sorted");
        }
        let release = &entry.first_seen_release;
        if release.len() != 7
            || release.as_bytes()[4] != b'-'
            || NaiveDate::parse_from_str(&format!("{release}-01"), "%Y-%m-%d").is_err()
        {
            bail!("ledger first_seen_release must be YYYY-MM");
        }
        validate_sha256(
            "ledger first_seen_artifact_sha256",
            &entry.first_seen_artifact_sha256,
        )?;
        validate_source_text(
            "ledger first_seen_source_locator",
            &entry.first_seen_source_locator,
            false,
        )?;
        if entry
            .canonical_names
            .iter()
            .any(|name| normalize_identity_name(name) == "phenakite")
            && (!entry.bootstrap_adoption || entry.slug != LEGACY_PHENAKITE_SLUG)
        {
            bail!(
                "Phenakite must explicitly adopt the existing route '{}'",
                LEGACY_PHENAKITE_SLUG
            );
        }
    }
    Ok(())
}

fn validate_identity_overrides(overrides: &ImaIdentityOverrides) -> Result<()> {
    if overrides.format != IMA_IDENTITY_OVERRIDES_FORMAT || overrides.dataset_key != IMA_DATASET_KEY
    {
        bail!("unsupported IMA identity overrides file");
    }
    if !overrides.entries.is_empty()
        && (overrides.entries.len() != 1
            || normalize_identity_name(&overrides.entries[0].canonical_name) != "phenakite")
    {
        bail!("the reviewed bootstrap crosswalk may adopt only the existing Phenakite route");
    }
    let mut names = HashSet::new();
    let mut slugs = HashSet::new();
    let mut source_ids = HashSet::new();
    for entry in &overrides.entries {
        validate_source_text(
            "identity override canonical_name",
            &entry.canonical_name,
            false,
        )?;
        if !names.insert(normalize_identity_name(&entry.canonical_name)) {
            bail!(
                "duplicate identity override name '{}'",
                entry.canonical_name
            );
        }
        if !is_valid_mineral_slug(&entry.slug) || !slugs.insert(entry.slug.clone()) {
            bail!(
                "invalid or duplicate identity override slug '{}'",
                entry.slug
            );
        }
        if !entry.adopt_existing_route {
            bail!("identity overrides are only permitted for explicit route adoption");
        }
        if let Some(source_id) = entry.source_record_id.as_deref() {
            if !is_opaque_source_id(source_id) || !source_ids.insert(source_id.to_string()) {
                bail!("invalid or duplicate override source_record_id '{source_id}'");
            }
        }
        if normalize_identity_name(&entry.canonical_name) == "phenakite"
            && entry.slug != LEGACY_PHENAKITE_SLUG
        {
            bail!(
                "Phenakite override must use existing route '{}'",
                LEGACY_PHENAKITE_SLUG
            );
        }
    }
    Ok(())
}

fn validate_build_options(options: &ImaBundleBuildOptions) -> Result<()> {
    if !(1..=MAX_MINERAL_INGESTION_CHUNK_ITEMS).contains(&options.chunk_size) {
        bail!("chunk size must be between 1 and {MAX_MINERAL_INGESTION_CHUNK_ITEMS}");
    }
    if NaiveDate::parse_from_str(&options.released_at, "%Y-%m-%d").is_err()
        && DateTime::parse_from_rfc3339(&options.released_at).is_err()
    {
        bail!("released-at must be YYYY-MM-DD or RFC 3339");
    }
    if let Some(base) = options.base_batch_id.as_deref() {
        validate_batch_id(base)?;
    }
    Ok(())
}

fn validate_release_dates(version: &str, released_at: &str, retrieved_at: &str) -> Result<()> {
    let release_date = NaiveDate::parse_from_str(released_at, "%Y-%m-%d")
        .or_else(|_| DateTime::parse_from_rfc3339(released_at).map(|value| value.date_naive()))
        .context("released-at must be YYYY-MM-DD or RFC 3339")?;
    let retrieval_date = DateTime::parse_from_rfc3339(retrieved_at)
        .context("retrieved-at must be RFC 3339")?
        .date_naive();
    if release_date.format("%Y-%m").to_string() != version {
        bail!("released-at month does not match the source release label");
    }
    if retrieval_date < release_date {
        bail!("retrieved-at cannot precede released-at");
    }
    Ok(())
}

fn validate_source_text(label: &str, value: &str, allow_empty: bool) -> Result<()> {
    if (!allow_empty && value.trim().is_empty()) || value.trim() != value {
        bail!("{label} is empty or padded");
    }
    if value.chars().any(char::is_control) {
        bail!("{label} contains a control character");
    }
    Ok(())
}

fn is_forbidden_formula_codepoint(character: char) -> bool {
    let value = character as u32;
    character == '\u{fffd}'
        || matches!(
            value,
            0xE000..=0xF8FF
                | 0xF0000..=0xFFFFD
                | 0x100000..=0x10FFFD
                | 0x0400..=0x052F
                | 0x1C80..=0x1C8F
                | 0x2DE0..=0x2DFF
                | 0xA640..=0xA69F
                | 0xFE2E..=0xFE2F
                | 0x1E030..=0x1E08F
        )
}

fn validate_http_url(label: &str, value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).with_context(|| format!("invalid {label}"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        bail!("{label} must be an HTTP(S) URL without credentials or a fragment");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StagingServerUrl {
    base_url: String,
    direct_loopback_http: bool,
}

fn validate_server_url(value: &str) -> Result<StagingServerUrl> {
    validate_http_url("server URL", value)?;
    let url = reqwest::Url::parse(value)?;
    if url.query().is_some() {
        bail!("server URL cannot contain a query");
    }
    let direct_loopback_http = url.scheme() == "http"
        && url
            .host_str()
            .and_then(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().ok())
            .is_some_and(|address| address.is_loopback());
    if url.scheme() != "https" && !direct_loopback_http {
        bail!("server URL must use HTTPS except for a literal loopback IP over HTTP");
    }
    Ok(StagingServerUrl {
        base_url: url.as_str().trim_end_matches('/').to_string(),
        direct_loopback_http,
    })
}

fn release_version(label: &str) -> Result<String> {
    let mut parts = label.split_whitespace();
    let month = parts.next().context("release label has no month")?;
    let year = parts.next().context("release label has no year")?;
    if parts.next().is_some() || year.len() != 4 || !year.bytes().all(|byte| byte.is_ascii_digit())
    {
        bail!("unsupported release label '{label}'");
    }
    let month = match month.to_ascii_lowercase().as_str() {
        "january" => "01",
        "february" => "02",
        "march" => "03",
        "april" => "04",
        "may" => "05",
        "june" => "06",
        "july" => "07",
        "august" => "08",
        "september" => "09",
        "october" => "10",
        "november" => "11",
        "december" => "12",
        _ => bail!("unsupported release month '{month}'"),
    };
    Ok(format!("{year}-{month}"))
}

fn source_locator(artifact_sha256: &str, row: &ImaExtractedRow) -> String {
    format!(
        "ima-master-list:{};pdf-page:{};table-row:{};ordinal:{}",
        artifact_sha256, row.page, row.page_row, row.ordinal
    )
}

fn normalize_identity_name(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn unique_opaque_source_id(entries: &[ImaIdentityEntry]) -> Result<String> {
    let existing = entries
        .iter()
        .map(|entry| entry.source_record_id.as_str())
        .collect::<HashSet<_>>();
    for _ in 0..16 {
        let mut bytes = [0u8; 16];
        getrandom::getrandom(&mut bytes)
            .map_err(|error| anyhow::anyhow!("failed to create opaque source identity: {error}"))?;
        let candidate = format!("ima_species_{}", hex_lower(&bytes));
        if !existing.contains(candidate.as_str()) {
            return Ok(candidate);
        }
    }
    bail!("could not create a unique opaque source identity")
}

fn is_opaque_source_id(value: &str) -> bool {
    value.strip_prefix("ima_species_").is_some_and(|suffix| {
        suffix.len() == 32
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn stable_slug(name: &str, source_record_id: &str) -> String {
    let mut base = String::new();
    let mut separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !base.is_empty() {
                base.push('-');
            }
            separator = false;
            base.push(character.to_ascii_lowercase());
        } else {
            separator = true;
        }
        if base.len() >= 150 {
            break;
        }
    }
    while base.ends_with('-') {
        base.pop();
    }
    if base.is_empty() {
        base.push_str("species");
    }
    let suffix = source_record_id
        .strip_prefix("ima_species_")
        .unwrap_or(source_record_id)
        .chars()
        .take(12)
        .collect::<String>();
    format!("mineral.{base}-{suffix}")
}

fn is_valid_mineral_slug(value: &str) -> bool {
    value.starts_with("mineral.")
        && value.len() <= 200
        && value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '-' | '_')
        })
}

fn validate_sha256(label: &str, value: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        bail!("{label} must start with sha256:");
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{label} must contain 64 lowercase hexadecimal characters");
    }
    Ok(())
}

fn validate_batch_id(value: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("batch_") else {
        bail!("base batch id must start with batch_");
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("base batch id must contain 64 lowercase hexadecimal characters");
    }
    Ok(())
}

fn canonical_value_sha256(value: &impl Serialize) -> Result<String> {
    Ok(sha256_bytes(&canonical_json_bytes(value)?))
}

fn canonical_json_bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value)?;
    fn sorted(value: Value) -> Value {
        match value {
            Value::Object(values) => Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, sorted(value)))
                    .collect::<BTreeMap<_, _>>()
                    .into_iter()
                    .collect(),
            ),
            Value::Array(values) => Value::Array(values.into_iter().map(sorted).collect()),
            value => value,
        }
    }
    serde_json::to_vec(&sorted(value)).context("failed to serialize canonical JSON")
}

fn sha256_file(path: &Path) -> Result<String> {
    Ok(sha256_bytes(&fs::read(path).with_context(|| {
        format!("failed to read {}", path.display())
    })?))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex_lower(digest(&SHA256, bytes).as_ref()))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn validate_safe_relative_path(path: &Path, required_prefix: &str) -> Result<()> {
    if path.is_absolute()
        || !path.starts_with(required_prefix)
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("unsafe release bundle path {}", path.display());
    }
    Ok(())
}

fn write_pretty_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("invalid JSON in {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_row(name: &str, formula: &str, status: &str, number_year: &str) -> ImaExtractedRow {
        ImaExtractedRow {
            ordinal: 1,
            page: 3,
            page_row: 2,
            bbox: vec![1.0, 2.0, 3.0, 4.0],
            canonical_name: name.to_string(),
            formula: formula.to_string(),
            raw_status: status.to_string(),
            ima_number_year: number_year.to_string(),
            country: "Testland".to_string(),
            first_reference: "Test reference".to_string(),
            second_reference: String::new(),
        }
    }

    fn test_document(rows: Vec<ImaExtractedRow>) -> ImaReconciledDocument {
        let mut rows = rows;
        for (index, row) in rows.iter_mut().enumerate() {
            row.ordinal = index + 1;
            row.page_row = index + 2;
        }
        let mut status_counts = BTreeMap::new();
        let mut valid = 0;
        let mut hidden = 0;
        let mut official = 0;
        for row in &rows {
            *status_counts.entry(row.raw_status.clone()).or_default() += 1;
            if map_ima_status(&row.raw_status).unwrap().is_valid_species {
                valid += 1;
            } else {
                hidden += 1;
            }
            official += usize::from(official_ima_number(&row.ima_number_year).is_some());
        }
        ImaReconciledDocument {
            format: IMA_RECONCILIATION_FORMAT.to_string(),
            summary: ImaReconciliationSummary {
                format: IMA_RECONCILIATION_FORMAT.to_string(),
                artifact_sha256: format!("sha256:{}", "a".repeat(64)),
                page_count: 3,
                table_page_count: 1,
                release_label: "July 2026".to_string(),
                license_spdx: IMA_SOURCE_LICENSE.to_string(),
                declared_valid_species: valid,
                total_rows: rows.len(),
                valid_species: valid,
                hidden_historical_rows: hidden,
                status_counts,
                official_ima_number_count: official,
                missing_formula_count: rows.iter().filter(|row| row.formula.is_empty()).count(),
                extractor_disagreement_count: 0,
                reviewed_whitespace_resolution_count: 90,
                reviewed_whitespace_resolution_fields: BTreeMap::from([
                    ("canonical_name".to_string(), 20),
                    ("first_reference".to_string(), 6),
                    ("formula".to_string(), 62),
                    ("second_reference".to_string(), 2),
                ]),
                extractor_versions: ImaExtractorVersions {
                    pdfplumber: "0.11.9".to_string(),
                    pymupdf: "1.28.2".to_string(),
                    python: "3.12.13".to_string(),
                },
                formula_replacement_glyph_count: 0,
                formula_private_use_count: 0,
                formula_cyrillic_count: 0,
                normalization_policy: IMA_NORMALIZATION_POLICY.to_string(),
                override_review_policy: IMA_OVERRIDE_REVIEW_POLICY.to_string(),
                source_transformation_count: 6,
                source_transformation_fields: BTreeMap::from([("formula".to_string(), 6)]),
            },
            rows,
        }
    }

    fn fixed_ledger(document: &ImaReconciledDocument) -> ImaIdentityLedger {
        ImaIdentityLedger {
            format: IMA_IDENTITY_LEDGER_FORMAT.to_string(),
            dataset_key: IMA_DATASET_KEY.to_string(),
            revision: 1,
            parent_sha256: Some(format!("sha256:{}", "b".repeat(64))),
            entries: document
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let id = format!("ima_species_{index:032x}");
                    ImaIdentityEntry {
                        source_record_id: id.clone(),
                        slug: stable_slug(&row.canonical_name, &id),
                        canonical_names: vec![row.canonical_name.clone()],
                        ima_numbers: official_ima_number(&row.ima_number_year)
                            .into_iter()
                            .collect(),
                        first_seen_release: "2026-07".to_string(),
                        first_seen_artifact_sha256: document.summary.artifact_sha256.clone(),
                        first_seen_source_locator: source_locator(
                            &document.summary.artifact_sha256,
                            row,
                        ),
                        bootstrap_adoption: false,
                    }
                })
                .collect(),
        }
    }

    #[test]
    fn identity_ledger_writer_never_overwrites_a_preexisting_file() {
        let temp = tempfile::tempdir().expect("temporary ledger directory");
        let path = temp.path().join("identity-ledger.json");
        let sentinel = b"operator-owned existing ledger\n";
        fs::write(&path, sentinel).expect("write existing ledger sentinel");
        let document = test_document(vec![test_row("Testite", "SiO2", "A", "2026-001")]);
        let ledger = fixed_ledger(&document);

        let error = write_identity_ledger(&path, &ledger)
            .expect_err("exclusive ledger creation must reject an existing path");

        assert!(error.to_string().contains("refusing to overwrite"));
        assert_eq!(fs::read(&path).expect("read existing ledger"), sentinel);
    }

    #[test]
    fn concurrent_identity_ledger_writers_have_exactly_one_winner() {
        const WRITERS: usize = 8;

        let temp = tempfile::tempdir().expect("temporary ledger directory");
        let path = std::sync::Arc::new(temp.path().join("identity-ledger.json"));
        let document = test_document(vec![test_row("Testite", "SiO2", "A", "2026-001")]);
        let ledger = std::sync::Arc::new(fixed_ledger(&document));
        let expected_hash = canonical_value_sha256(ledger.as_ref()).expect("canonical ledger hash");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(WRITERS));
        let writers = (0..WRITERS)
            .map(|_| {
                let path = std::sync::Arc::clone(&path);
                let ledger = std::sync::Arc::clone(&ledger);
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    write_identity_ledger(path.as_ref(), ledger.as_ref())
                })
            })
            .collect::<Vec<_>>();

        let results = writers
            .into_iter()
            .map(|writer| writer.join().expect("ledger writer did not panic"))
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results.iter().filter(|result| result.is_err()).count(),
            WRITERS - 1
        );
        let stored = load_identity_ledger(path.as_ref()).expect("load winning ledger");
        assert_eq!(
            canonical_value_sha256(&stored).expect("stored ledger hash"),
            expected_hash
        );
    }

    #[test]
    fn status_mapping_is_explicit_and_a_question_is_uncertain_but_valid() {
        assert_eq!(
            map_ima_status("A?").unwrap(),
            ImaStatusMapping {
                nomenclature_status: "uncertain",
                is_valid_species: true
            }
        );
        assert_eq!(
            map_ima_status("A ?").unwrap(),
            map_ima_status("A?").unwrap()
        );
        assert!(map_ima_status("A  ?").is_err());
        assert!(map_ima_status(" A?").is_err());
        assert_eq!(
            map_ima_status("D").unwrap(),
            ImaStatusMapping {
                nomenclature_status: "discredited",
                is_valid_species: false
            }
        );
        assert!(map_ima_status("N").is_err());
    }

    #[test]
    fn opaque_source_id_contract_is_exact() {
        assert!(is_opaque_source_id(
            "ima_species_0123456789abcdef0123456789abcdef"
        ));
        assert!(!is_opaque_source_id(
            "ima_species_0123456789ABCDEF0123456789ABCDEF"
        ));
        assert!(!is_opaque_source_id("ima_species_phenakite"));
        assert!(!is_opaque_source_id(
            "ima_species_0123456789abcdef0123456789abcdef00"
        ));
    }

    #[test]
    fn official_ima_number_excludes_year_special_procedure_and_unknown() {
        assert!(is_official_ima_number("2025-031a"));
        assert!(is_official_ima_number("2014-111"));
        assert!(!is_official_ima_number("1973"));
        assert!(!is_official_ima_number("1988 s.p."));
        assert!(!is_official_ima_number("?"));
        assert_eq!(
            official_ima_number("2025-031A").as_deref(),
            Some("2025-031a")
        );
        for (raw, kind) in [
            ("1968 ?", "uncertain_year"),
            ("1982 s.p.?", "uncertain_special_procedure_year"),
            ("1982 s.p. ?", "uncertain_special_procedure_year"),
            ("1971-xxx", "placeholder_ima_number"),
            ("1988-xxx ?", "uncertain_placeholder_ima_number"),
        ] {
            assert_eq!(classify_ima_number_year(raw).unwrap(), kind);
            assert_eq!(official_ima_number(raw), None);
        }
    }

    #[test]
    fn reconciled_formulas_reject_unresolved_glyphs_without_transliteration() {
        for formula in [
            "Ca\u{fffd}CO3",
            "Ca\u{e000}CO3",
            "Ca\u{f0000}CO3",
            "\u{0410}l2O3",
            "C\u{1e030}O2",
        ] {
            let document = test_document(vec![test_row("Unsafe", formula, "A", "2026-001")]);
            assert!(validate_reconciled_document(&document).is_err());
        }
        let valid = test_document(vec![test_row(
            "Hydrate",
            "CaSO4\u{00b7}2H2O",
            "A",
            "2026-002",
        )]);
        validate_reconciled_document(&valid).unwrap();
    }

    #[test]
    fn extractor_normalization_reconstructs_whitespace_and_preserves_middle_dots() {
        assert_eq!(
            normalize_extractor_text("CaSO4·\n2H2O").unwrap(),
            "CaSO4· 2H2O"
        );
        assert_eq!(normalize_extractor_text("CaCO3∙H2O").unwrap(), "CaCO3∙H2O");
        assert_eq!(normalize_extractor_text("Cafe\u{301}").unwrap(), "Café");
        assert!(normalize_extractor_text("Ca\u{fffd}CO3").is_err());
    }

    #[test]
    fn renamed_row_with_same_official_number_keeps_identity_and_slug() {
        let first = test_document(vec![test_row("Old name", "SiO2", "A", "2025-031a")]);
        let ledger = fixed_ledger(&first);
        let source_id = ledger.entries[0].source_record_id.clone();
        let slug = ledger.entries[0].slug.clone();
        let renamed = test_document(vec![test_row(
            "New accepted name",
            "SiO2",
            "Rn",
            "2025-031a",
        )]);
        let next = evolve_identity_ledger(&renamed, &ledger, false).unwrap();
        assert_eq!(next.entries[0].source_record_id, source_id);
        assert_eq!(next.entries[0].slug, slug);
        assert!(next.entries[0]
            .canonical_names
            .contains(&"New accepted name".to_string()));
        let resolved = resolve_release_rows(&renamed, &next).unwrap();
        let item = build_ingestion_item(&renamed, &resolved[0]).unwrap();
        assert_eq!(item.synonyms, vec!["Old name"]);
        assert_eq!(item.official_facts.discovery_country, "Testland");
        assert_eq!(item.official_facts.first_reference, "Test reference");
        assert!(item.official_facts.second_reference.is_empty());
        assert_eq!(item.official_facts.source_status, "Rn");
    }

    #[test]
    fn phenakite_bootstrap_requires_explicit_existing_route_adoption() {
        let document = test_document(vec![test_row("Phenakite", "Be2SiO4", "G", "1833")]);
        let error = initialize_identity_ledger(&document).unwrap_err();
        assert!(error.to_string().contains("must explicitly adopt"));

        let overrides = ImaIdentityOverrides {
            format: IMA_IDENTITY_OVERRIDES_FORMAT.to_string(),
            dataset_key: IMA_DATASET_KEY.to_string(),
            entries: vec![ImaIdentityOverride {
                canonical_name: "Phenakite".to_string(),
                slug: LEGACY_PHENAKITE_SLUG.to_string(),
                source_record_id: Some("ima_species_0123456789abcdef0123456789abcdef".to_string()),
                adopt_existing_route: true,
            }],
        };
        let ledger = initialize_identity_ledger_with_overrides(&document, &overrides).unwrap();
        assert_eq!(ledger.entries[0].slug, LEGACY_PHENAKITE_SLUG);
        assert!(ledger.entries[0].bootstrap_adoption);
        assert!(is_opaque_source_id(&ledger.entries[0].source_record_id));

        let mut wrong_route = overrides;
        wrong_route.entries[0].slug = "mineral.phenakite-new".to_string();
        assert!(initialize_identity_ledger_with_overrides(&document, &wrong_route).is_err());
    }

    #[test]
    fn deterministic_release_material_uses_fixed_ledger_and_preserves_raw_audit() {
        let document = test_document(vec![
            test_row("Balipholite", "Li2CO3·H2O", "A ?", "?"),
            test_row("Franklinphilite", "K4Mn48Si72O216", "D", "1990-050"),
        ]);
        let ledger = fixed_ledger(&document);
        let material = || {
            let resolved = resolve_release_rows(&document, &ledger).unwrap();
            let mut items = resolved
                .iter()
                .map(|row| build_ingestion_item(&document, row).unwrap())
                .collect::<Vec<_>>();
            items.sort_by(|left, right| left.source_record_id.cmp(&right.source_record_id));
            let chunks = items
                .iter()
                .enumerate()
                .map(|(chunk_index, item)| MineralIngestionChunk {
                    schema_version: MINERAL_INGESTION_SCHEMA_VERSION,
                    chunk_index,
                    items: vec![item.clone()],
                })
                .collect::<Vec<_>>();
            (
                canonical_mineral_records_hash(&items).unwrap(),
                chunks
                    .iter()
                    .map(|chunk| canonical_mineral_chunk_hash(chunk).unwrap())
                    .collect::<Vec<_>>(),
                audit_json_lines(&document, &resolved).unwrap(),
                chunks,
            )
        };
        let first = material();
        let second = material();
        assert_eq!(first.0, second.0);
        assert_eq!(first.1, second.1);
        assert_eq!(first.2, second.2);
        let audit = String::from_utf8(first.2).unwrap();
        assert!(audit.contains("\"raw_status\":\"A ?\""));
        assert!(audit.contains("\"raw_ima_number_year\":\"?\""));
        assert!(audit.contains("Li2CO3·H2O"));
        let first_chunk = &first.3[0];
        assert_eq!(first_chunk.items[0].nomenclature_status, "uncertain");
        assert_eq!(first_chunk.items[0].formula, "Li2CO3·H2O");
        assert!(first_chunk.items[0].official_identifiers.is_empty());
        assert_eq!(
            first_chunk.items[0].official_facts.discovery_country,
            "Testland"
        );
        assert_eq!(first_chunk.items[0].official_facts.source_status, "A ?");
        let chunk_json = serde_json::to_value(first_chunk).unwrap();
        assert!(!chunk_json.to_string().contains("image"));
        let historical_chunk = &first.3[1];
        assert_eq!(historical_chunk.items[0].nomenclature_status, "discredited");
        assert!(!historical_chunk.items[0].is_valid_species);
    }

    #[test]
    fn staging_plan_contains_no_approval_or_decision_endpoint() {
        let endpoints = staging_endpoint_suffixes("batch_abc", 3);
        assert!(endpoints.iter().all(|path| !path.contains("decision")));
        assert!(endpoints.iter().all(|path| !path.contains("approve")));
        assert_eq!(
            endpoints.last().unwrap(),
            "/admin/ingestion/batches/batch_abc/finalize"
        );
    }

    fn staging_response(
        status: &str,
        report_hash: Option<String>,
        received_chunk_count: usize,
        received_record_count: usize,
    ) -> IngestionMutationResponse {
        IngestionMutationResponse {
            batch_id: format!("batch_{}", "a".repeat(64)),
            status: status.to_string(),
            manifest_hash: format!("sha256:{}", "a".repeat(64)),
            report_hash,
            received_chunk_count,
            expected_chunk_count: 2,
            received_record_count,
            expected_record_count: 3,
        }
    }

    #[test]
    fn staging_response_status_requires_matching_report_and_complete_counts() {
        let report_hash = Some(format!("sha256:{}", "b".repeat(64)));
        assert!(validate_mutation_status(&staging_response("receiving", None, 1, 2)).is_ok());
        assert!(validate_mutation_status(&staging_response(
            "receiving",
            report_hash.clone(),
            1,
            2
        ))
        .is_err());

        for status in ["ready", "needs_attention", "approved"] {
            assert!(
                validate_mutation_status(&staging_response(status, report_hash.clone(), 2, 3))
                    .is_ok()
            );
            assert!(validate_mutation_status(&staging_response(status, None, 2, 3)).is_err());
            assert!(
                validate_mutation_status(&staging_response(status, report_hash.clone(), 1, 2))
                    .is_err()
            );
        }

        assert!(validate_mutation_status(&staging_response("rejected", None, 1, 2)).is_ok());
        assert!(
            ensure_stage_response_is_successful(&staging_response("rejected", None, 1, 2)).is_err()
        );
        assert!(
            validate_mutation_status(&staging_response("rejected", report_hash.clone(), 1, 2))
                .is_err()
        );
        let reviewed_rejection = staging_response("rejected", report_hash, 2, 3);
        assert!(validate_mutation_status(&reviewed_rejection).is_ok());
        assert!(ensure_stage_response_is_successful(&reviewed_rejection).is_ok());
    }

    #[test]
    fn staging_response_record_count_must_match_a_real_chunk_subset() {
        let item_counts = [2, 2, 1];
        assert!(is_possible_received_count(&item_counts, 0, 0));
        assert!(is_possible_received_count(&item_counts, 1, 1));
        assert!(is_possible_received_count(&item_counts, 1, 2));
        assert!(is_possible_received_count(&item_counts, 2, 3));
        assert!(is_possible_received_count(&item_counts, 2, 4));
        assert!(is_possible_received_count(&item_counts, 3, 5));
        assert!(!is_possible_received_count(&item_counts, 0, 1));
        assert!(!is_possible_received_count(&item_counts, 2, 2));
        assert!(!is_possible_received_count(&item_counts, 3, 4));
    }

    #[test]
    fn staging_url_requires_https_except_for_literal_loopback_http() {
        let https = validate_server_url("https://registry.example.test/base/").unwrap();
        assert_eq!(https.base_url, "https://registry.example.test/base");
        assert!(!https.direct_loopback_http);

        for value in [
            "http://127.0.0.1:7979",
            "http://127.42.0.9:7979/",
            "http://[::1]:7979",
        ] {
            let server = validate_server_url(value).unwrap();
            assert!(server.direct_loopback_http, "{value}");
        }

        for value in [
            "http://localhost:7979",
            "http://registry.example.test",
            "http://192.168.1.10",
            "https://user:secret@registry.example.test",
            "https://registry.example.test?token=secret",
            "https://registry.example.test/#fragment",
        ] {
            assert!(validate_server_url(value).is_err(), "{value}");
        }
    }
}
