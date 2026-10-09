//! Offline candidate discovery over the complete, immutable COD metadata snapshot.
//! All source rows remain addressable; matching never accepts a crosswalk or
//! normalizes a crystallographic measurement.

use std::io::BufWriter;

use serde_json::json;
use unicode_normalization::char::is_combining_mark;

use super::*;

const MATCHING_ROOT: &str = "candidate-matching-v1";
const MANIFEST_FILE: &str = "matching-manifest.json";
const INVENTORY_FILE: &str = "source-inventory.jsonl";
const QUEUE_FILE: &str = "review-queue.jsonl";
const COVERAGE_FILE: &str = "mineral-coverage.jsonl";
const REPORT_FILE: &str = "report.md";
const ARTIFACT_FILES: [&str; 4] = [INVENTORY_FILE, QUEUE_FILE, COVERAGE_FILE, REPORT_FILE];
const RAW_FIELDS: &[&str] = &[
    "mineral",
    "commonname",
    "chemname",
    "formula",
    "compoundsource",
    "duplicateof",
    "optimal",
    "status",
    "flags",
    "onhold",
    "doi",
    "title",
    "authors",
    "journal",
    "year",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchingManifest {
    format: String,
    schema_version: u32,
    pilot_id: String,
    status: String,
    output_directory: String,
    inputs: Value,
    policy: Value,
    counts: MatchingCounts,
    artifacts: Vec<ArtifactBinding>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MatchingCounts {
    source_rows: u64,
    unique_cod_revision_pairs: u64,
    repeated_metadata_rows: u64,
    rows_with_mineral_name: u64,
    unique_exact_candidate_rows: u64,
    ambiguous_exact_candidate_rows: u64,
    exploratory_only_rows: u64,
    unlinked_rows: u64,
    review_queue_rows: u64,
    minerals_total: u64,
    minerals_with_exact_candidates: u64,
    minerals_with_ambiguous_exact_candidates: u64,
    minerals_with_exploratory_leads: u64,
    minerals_without_exact_candidates: u64,
    minerals_without_any_links: u64,
    review_flags: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Target {
    material_public_id: String,
    canonical_name_snapshot: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct ResearchLead {
    material_public_id: String,
    canonical_name_snapshot: String,
    source_field: String,
    source_value: String,
    signal: String,
}

#[derive(Debug, Serialize)]
struct SourceRecord {
    cod_id: String,
    svn_revision: u64,
    metadata_locator: MetadataLocator,
    raw_fields: BTreeMap<String, Value>,
    disposition: String,
    exact_candidates: Vec<Target>,
    exploratory_leads: Vec<ResearchLead>,
    review_flags: Vec<String>,
    repeated_metadata_row: bool,
    review_status: String,
}

#[derive(Debug, Serialize)]
struct MetadataLocator {
    raw_body: RawBodyBinding,
    json_pointer: String,
    request_ordinal: u32,
    retrieved_at: String,
}

#[derive(Debug, Default, Serialize)]
struct MineralCoverage {
    material_public_id: String,
    canonical_name_snapshot: String,
    exact_candidate_records: u64,
    ambiguous_exact_candidate_records: u64,
    exploratory_lead_records: u64,
    research_state: String,
}

struct NameIndex {
    targets: Vec<Target>,
    exact: BTreeMap<String, Vec<usize>>,
    folded: BTreeMap<String, Vec<usize>>,
    deletion_signatures: BTreeMap<String, BTreeSet<String>>,
}

impl NameIndex {
    fn new(entries: &[PopulationEntry]) -> Self {
        let mut index = Self {
            targets: Vec::new(),
            exact: BTreeMap::new(),
            folded: BTreeMap::new(),
            deletion_signatures: BTreeMap::new(),
        };
        for entry in entries {
            let position = index.targets.len();
            index.targets.push(Target {
                material_public_id: entry.material_public_id.clone(),
                canonical_name_snapshot: entry.canonical_name_snapshot.clone(),
            });
            index
                .exact
                .entry(entry.canonical_name_snapshot.clone())
                .or_default()
                .push(position);
            index
                .folded
                .entry(research_key(&entry.canonical_name_snapshot))
                .or_default()
                .push(position);
        }
        for key in index.folded.keys() {
            if typo_eligible(key) {
                for signature in deletion_signatures(key) {
                    index
                        .deletion_signatures
                        .entry(signature)
                        .or_default()
                        .insert(key.clone());
                }
            }
        }
        index
    }

    fn match_names(&self, row: &Value) -> (Vec<Target>, Vec<ResearchLead>) {
        // Literal, case-sensitive authority names are the frozen pilot signal.
        let exact = row
            .get("mineral")
            .and_then(Value::as_str)
            .and_then(|name| self.exact.get(name))
            .cloned()
            .unwrap_or_default();
        let exact_ids: BTreeSet<usize> = exact.iter().copied().collect();
        let mut leads = BTreeSet::new();
        for field in ["mineral", "commonname", "chemname"] {
            let Some(raw) = row.get(field).and_then(Value::as_str) else {
                continue;
            };
            let key = research_key(raw);
            if key.is_empty() {
                continue;
            }
            let mut suggestions = BTreeMap::new();
            if let Some(positions) = self.folded.get(&key) {
                for position in positions {
                    suggestions.insert(*position, "folded_name_hint");
                }
            }
            if typo_eligible(&key) {
                let mut nearby = BTreeSet::new();
                for signature in deletion_signatures(&key) {
                    if let Some(keys) = self.deletion_signatures.get(&signature) {
                        nearby.extend(keys.iter());
                    }
                }
                for other in nearby {
                    if other != &key && edit_distance_at_most_one(&key, other) {
                        for position in &self.folded[other] {
                            suggestions.entry(*position).or_insert("one_edit_name_hint");
                        }
                    }
                }
            }
            for (position, signal) in suggestions {
                if exact_ids.contains(&position) {
                    continue;
                }
                let target = &self.targets[position];
                leads.insert(ResearchLead {
                    material_public_id: target.material_public_id.clone(),
                    canonical_name_snapshot: target.canonical_name_snapshot.clone(),
                    source_field: field.to_string(),
                    source_value: raw.to_string(),
                    signal: signal.to_string(),
                });
            }
        }
        (
            exact
                .into_iter()
                .map(|position| self.targets[position].clone())
                .collect(),
            leads.into_iter().collect(),
        )
    }
}

fn research_key(value: &str) -> String {
    value
        .nfkd()
        .filter(|character| !is_combining_mark(*character))
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn typo_eligible(key: &str) -> bool {
    (5..=96).contains(&key.chars().count())
}

fn deletion_signatures(key: &str) -> BTreeSet<String> {
    let characters: Vec<char> = key.chars().collect();
    let mut signatures = BTreeSet::from([key.to_string()]);
    for skip in 0..characters.len() {
        signatures.insert(
            characters
                .iter()
                .enumerate()
                .filter_map(|(position, character)| (position != skip).then_some(*character))
                .collect(),
        );
    }
    signatures
}

fn edit_distance_at_most_one(left: &str, right: &str) -> bool {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    if left.len().abs_diff(right.len()) > 1 {
        return false;
    }
    let (mut i, mut j, mut edits) = (0, 0, 0);
    while i < left.len() && j < right.len() {
        if left[i] == right[j] {
            i += 1;
            j += 1;
            continue;
        }
        edits += 1;
        if edits > 1 {
            return false;
        }
        if left.len() >= right.len() {
            i += 1;
        }
        if right.len() >= left.len() {
            j += 1;
        }
    }
    edits + (left.len() - i) + (right.len() - j) <= 1
}

fn present(value: Option<&Value>) -> bool {
    value.is_some_and(|value| {
        !value.is_null() && value.as_str().is_none_or(|text| !text.trim().is_empty())
    })
}

fn source_record(
    row: &Value,
    row_index: usize,
    receipt: &SuccessfulReceipt,
    names: &NameIndex,
    seen: &mut BTreeSet<(String, u64)>,
) -> Result<SourceRecord> {
    let cod_id = parse_cod_file(&row["file"]).map_err(anyhow::Error::msg)?;
    let svn_revision = parse_positive_decimal(&row["svnrevision"]).map_err(anyhow::Error::msg)?;
    let repeated_metadata_row = !seen.insert((cod_id.clone(), svn_revision));
    let (exact_candidates, exploratory_leads) = names.match_names(row);
    let disposition = match exact_candidates.len() {
        1 => "exact_candidate_needs_review",
        2.. => "ambiguous_exact_candidates",
        _ if !exploratory_leads.is_empty() => "exploratory_leads_only",
        _ => "unlinked_retained",
    }
    .to_string();
    let mut flags = BTreeSet::new();
    for (field, flag) in [
        ("duplicateof", "duplicate_relation_reported"),
        ("optimal", "optimal_relation_reported"),
        ("status", "source_status_reported"),
        ("onhold", "source_onhold_reported"),
    ] {
        if present(row.get(field)) {
            flags.insert(flag.to_string());
        }
    }
    for field in ["compoundsource", "flags", "mineral"] {
        if let Some(text) = row.get(field).and_then(Value::as_str) {
            let text = text.to_lowercase();
            for term in ["synthetic", "theoretical"] {
                if text.contains(term) {
                    flags.insert(format!("{term}_text_in_{field}"));
                }
            }
        }
    }
    if exact_candidates.len() > 1 {
        flags.insert("multiple_exact_targets".to_string());
    }
    if !exploratory_leads.is_empty() {
        flags.insert("exploratory_identity_hints".to_string());
    }
    if repeated_metadata_row {
        flags.insert("repeated_metadata_row".to_string());
    }
    Ok(SourceRecord {
        cod_id,
        svn_revision,
        metadata_locator: MetadataLocator {
            raw_body: receipt.response.raw_body.clone(),
            json_pointer: format!("/{row_index}"),
            request_ordinal: receipt.ordinal,
            retrieved_at: receipt.response.retrieved_at.clone(),
        },
        raw_fields: RAW_FIELDS
            .iter()
            .filter_map(|field| {
                row.get(*field)
                    .map(|value| (field.to_string(), value.clone()))
            })
            .collect(),
        disposition,
        exact_candidates,
        exploratory_leads,
        review_flags: flags.into_iter().collect(),
        repeated_metadata_row,
        review_status: "unreviewed".to_string(),
    })
}

struct ArtifactWriter {
    path: String,
    file: Option<BufWriter<File>>,
    digest: DigestContext,
    bytes: u64,
}

impl ArtifactWriter {
    fn new(directory: Option<&Path>, name: &str) -> Result<Self> {
        let file = directory
            .map(|directory| {
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(directory.join(name))
                    .map(BufWriter::new)
            })
            .transpose()?;
        Ok(Self {
            path: name.to_string(),
            file,
            digest: DigestContext::new(&SHA256),
            bytes: 0,
        })
    }

    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        if let Some(file) = &mut self.file {
            file.write_all(bytes)?;
        }
        self.digest.update(bytes);
        self.bytes += bytes.len() as u64;
        Ok(())
    }

    fn row(&mut self, value: &impl Serialize) -> Result<()> {
        self.write(&canonical_file_bytes(value)?)
    }

    fn finish(mut self) -> Result<ArtifactBinding> {
        if let Some(file) = &mut self.file {
            file.flush()?;
            file.get_ref().sync_all()?;
        }
        Ok(ArtifactBinding {
            path: self.path,
            sha256: format_sha256_identifier(self.digest.finish().as_ref()),
            bytes: self.bytes,
        })
    }
}

fn render_artifacts(
    context: &ExecutionContext,
    index: &DiscoveryExecutionIndex,
    population: &PopulationSnapshot,
    directory: Option<&Path>,
) -> Result<(MatchingCounts, Vec<ArtifactBinding>)> {
    let names = NameIndex::new(&population.entries);
    let mut coverage: BTreeMap<String, MineralCoverage> = population
        .entries
        .iter()
        .map(|entry| {
            (
                entry.material_public_id.clone(),
                MineralCoverage {
                    material_public_id: entry.material_public_id.clone(),
                    canonical_name_snapshot: entry.canonical_name_snapshot.clone(),
                    ..MineralCoverage::default()
                },
            )
        })
        .collect();
    let mut counts = MatchingCounts {
        minerals_total: population.count,
        ..MatchingCounts::default()
    };
    let mut inventory = ArtifactWriter::new(directory, INVENTORY_FILE)?;
    let mut queue = ArtifactWriter::new(directory, QUEUE_FILE)?;
    for receipt in &index.successful_receipts {
        let bytes = read_and_validate_raw_body(context, &receipt.response.raw_body)?;
        let rows: Vec<Value> = serde_json::from_slice(&bytes)?;
        ensure!(
            rows.len() == receipt.response.json_validation.row_count as usize,
            "metadata row count changed"
        );
        // IDs are shard-prefixed, so an identical COD/revision pair cannot occur
        // in two different verified shards. Bound deduplication memory per shard.
        let mut seen = BTreeSet::new();
        for (position, row) in rows.iter().enumerate() {
            let record = source_record(row, position, receipt, &names, &mut seen)?;
            counts.source_rows += 1;
            counts.repeated_metadata_rows += u64::from(record.repeated_metadata_row);
            counts.unique_cod_revision_pairs += u64::from(!record.repeated_metadata_row);
            counts.rows_with_mineral_name += u64::from(
                row.get("mineral")
                    .and_then(Value::as_str)
                    .is_some_and(|name| !name.trim().is_empty()),
            );
            match record.disposition.as_str() {
                "exact_candidate_needs_review" => counts.unique_exact_candidate_rows += 1,
                "ambiguous_exact_candidates" => counts.ambiguous_exact_candidate_rows += 1,
                "exploratory_leads_only" => counts.exploratory_only_rows += 1,
                _ => counts.unlinked_rows += 1,
            }
            for flag in &record.review_flags {
                *counts.review_flags.entry(flag.clone()).or_default() += 1;
            }
            inventory.row(&record)?;
            if record.disposition != "unlinked_retained" {
                queue.row(&record)?;
                counts.review_queue_rows += 1;
            }
            if record.repeated_metadata_row {
                continue;
            }
            for target in &record.exact_candidates {
                let item = coverage
                    .get_mut(&target.material_public_id)
                    .context("candidate outside population")?;
                item.exact_candidate_records += 1;
                item.ambiguous_exact_candidate_records +=
                    u64::from(record.exact_candidates.len() > 1);
            }
            let lead_ids: BTreeSet<&str> = record
                .exploratory_leads
                .iter()
                .map(|lead| lead.material_public_id.as_str())
                .collect();
            for id in lead_ids {
                coverage
                    .get_mut(id)
                    .context("research lead outside population")?
                    .exploratory_lead_records += 1;
            }
        }
    }
    let expected_rows: u64 = index
        .successful_receipts
        .iter()
        .map(|receipt| u64::from(receipt.response.json_validation.row_count))
        .sum();
    ensure!(
        counts.source_rows == expected_rows,
        "not every metadata row was inventoried"
    );
    ensure!(
        counts.source_rows
            == counts.unique_exact_candidate_rows
                + counts.ambiguous_exact_candidate_rows
                + counts.exploratory_only_rows
                + counts.unlinked_rows,
        "source row partition does not balance"
    );
    let mut coverage_writer = ArtifactWriter::new(directory, COVERAGE_FILE)?;
    for item in coverage.values_mut() {
        counts.minerals_with_exact_candidates += u64::from(item.exact_candidate_records > 0);
        counts.minerals_with_ambiguous_exact_candidates +=
            u64::from(item.ambiguous_exact_candidate_records > 0);
        counts.minerals_with_exploratory_leads += u64::from(item.exploratory_lead_records > 0);
        counts.minerals_without_exact_candidates += u64::from(item.exact_candidate_records == 0);
        counts.minerals_without_any_links +=
            u64::from(item.exact_candidate_records == 0 && item.exploratory_lead_records == 0);
        item.research_state = if item.exact_candidate_records > 0 {
            "exact_candidates_need_review"
        } else if item.exploratory_lead_records > 0 {
            "exploratory_leads_need_research"
        } else {
            "no_name_candidate_further_research_needed"
        }
        .to_string();
        coverage_writer.row(item)?;
    }
    let snapshot = &index
        .snapshot_identity
        .as_ref()
        .context("missing completed snapshot")?
        .sha256;
    let mut report = ArtifactWriter::new(directory, REPORT_FILE)?;
    report.write(report_text(&counts, snapshot).as_bytes())?;
    Ok((
        counts,
        vec![
            inventory.finish()?,
            queue.finish()?,
            coverage_writer.finish()?,
            report.finish()?,
        ],
    ))
}

fn report_text(counts: &MatchingCounts, snapshot: &str) -> String {
    format!(
        "# COD offline mineral candidate report\n\n\
         Snapshot: `{snapshot}`\n\n\
         | Measure | Count |\n|---|---:|\n\
         | Source metadata rows retained in inventory | {} |\n\
         | Unique COD ID/revision pairs | {} |\n\
         | Repeated metadata rows retained | {} |\n\
         | Rows with a mineral-name label | {} |\n\
         | Rows with one literal exact candidate | {} |\n\
         | Rows with multiple literal exact candidates | {} |\n\
         | Rows with exploratory leads only | {} |\n\
         | Unlinked rows retained | {} |\n\
         | Mineral records examined | {} |\n\
         | Minerals with literal exact candidates | {} |\n\
         | Minerals with ambiguous exact candidates | {} |\n\
         | Minerals with exploratory leads (may overlap exact coverage) | {} |\n\
         | Minerals without literal exact candidates | {} |\n\
         | Minerals without any name links | {} |\n\n\
         These are discovery counts, not accepted crosswalks, reviewed facts,\n\
         independent corroborating sources, or the pilot's useful-yield metric.\n\
         All candidates and leads remain unreviewed. Every original metadata row,\n\
         including unlinked, duplicate, synthetic, theoretical, or flagged records,\n\
         is retained through its hashed raw-body locator and JSON row pointer.\n\n\
         `source-inventory.jsonl` contains every row; `review-queue.jsonl` contains\n\
         rows with proposed links; `mineral-coverage.jsonl` contains every mineral.\n\
         The original snapshot retains all fields, including those not repeated\n\
         in the inventory. No crystallographic values have been normalized.\n\n\
         Literal exact matching uses the metadata `mineral` label and the frozen\n\
         canonical name. The frozen population has no reviewed authority-alias\n\
         artifact; aliases are not invented. Case, diacritic, punctuation, and\n\
         one-edit spelling hints from `mineral`, `commonname`, or `chemname`\n\
         are separate exploratory leads and never accepted identity matches.\n\
         Name-only discovery misses unnamed or differently named determinations.\n\
         No candidate means further research is needed, not mineral absence.\n\n\
         Next: inspect source locators and publications, review mineral identity,\n\
         and prepare the pilot's reviewed challenge eligibility and selection.\n\
         Preserve unresolved links and all original evidence for later research.\n",
        counts.source_rows,
        counts.unique_cod_revision_pairs,
        counts.repeated_metadata_rows,
        counts.rows_with_mineral_name,
        counts.unique_exact_candidate_rows,
        counts.ambiguous_exact_candidate_rows,
        counts.exploratory_only_rows,
        counts.unlinked_rows,
        counts.minerals_total,
        counts.minerals_with_exact_candidates,
        counts.minerals_with_ambiguous_exact_candidates,
        counts.minerals_with_exploratory_leads,
        counts.minerals_without_exact_candidates,
        counts.minerals_without_any_links,
    )
}

/// Generate a new immutable matching report, or verify/reuse the existing report
/// for the identical inputs. No network, database writes, or identity acceptance.
pub fn match_metadata(
    repo_root: &Path,
    prepared: &Path,
    pilot_root: &Path,
) -> Result<MatchingManifest> {
    run_matching(repo_root, prepared, pilot_root, false)
}

/// Reproduce every report artifact offline and compare its exact bytes/hash.
pub fn verify_matching(
    repo_root: &Path,
    prepared: &Path,
    pilot_root: &Path,
) -> Result<MatchingManifest> {
    run_matching(repo_root, prepared, pilot_root, true)
}

fn run_matching(
    repo_root: &Path,
    prepared: &Path,
    pilot_root: &Path,
    verify_only: bool,
) -> Result<MatchingManifest> {
    let context = load_execution_context(repo_root, prepared, pilot_root)?;
    let _lock = FetchLock::acquire(&context.pilot_root)?;
    let index_path = context.pilot_root.join(DISCOVERY_INDEX_FILE);
    let index =
        load_execution_index(&index_path)?.context("metadata discovery has no saved index")?;
    validate_execution_index(&context, &index)?;
    ensure!(
        index.status == "complete",
        "finish metadata discovery before whole-snapshot matching"
    );
    let prepared_path = require_real_directory(
        &resolve_repo_argument(&context.repo_root, prepared),
        "prepared directory",
    )?;
    let population_path = prepared_path.join(POPULATION_SNAPSHOT_FILE);
    let population_bytes = read_regular_file(&population_path, "frozen population")?;
    let population: PopulationSnapshot = serde_json::from_slice(&population_bytes)?;
    let mut code = Vec::new();
    for path in ["src/cod_matching.rs", "src/cod_pilot.rs", "Cargo.lock"] {
        let file = safe_repo_file(&context.repo_root, path, "matching code input")?;
        let (sha256, bytes) = hash_file(&file)?;
        code.push(InputArtifact {
            path: path.to_string(),
            sha256,
            bytes,
        });
    }
    let (index_sha256, index_bytes) = hash_file(&index_path)?;
    let inputs = json!({
        "discovery_snapshot": index.snapshot_identity,
        "execution_index": { "path": repo_relative_path(&context.repo_root, &index_path)?, "sha256": index_sha256, "bytes": index_bytes },
        "query_plan": index.plan,
        "population_snapshot": { "path": repo_relative_path(&context.repo_root, &population_path)?, "sha256": sha256_identifier(&population_bytes), "bytes": population_bytes.len() },
        "population_release_id": population.source.release_id,
        "code_inputs": code,
    });
    let policy = json!({
        "candidate_algorithm": "exact_authority_names_v1",
        "exact_comparison": "literal_case_sensitive_metadata_mineral_to_frozen_canonical_name",
        "authority_alias_input": "not_available_in_frozen_population",
        "research_lead_algorithm": "research_name_hints_v1",
        "research_name_fields": ["mineral", "commonname", "chemname"],
        "research_fold": "unicode_nfkd_remove_combining_marks_lowercase_alphanumeric_only",
        "research_edit_distance": { "maximum": 1, "eligible_folded_name_length": [5, 96] },
        "formula_role": "raw_diagnostic_only",
        "all_source_rows_inventoried": true,
        "review_status": "unreviewed",
        "accepted_crosswalks_created": false,
        "scientific_value_normalization": false,
        "network_access": "none",
        "database_writes": "none",
        "public_projection": "none",
        "artifact_order": "receipt_ordinal_then_original_row_order; coverage_by_public_id",
    });
    let fingerprint = sha256_identifier(&canonical_json_bytes(
        &json!({"inputs":inputs, "policy":policy}),
    )?);
    let parent = context.pilot_root.join(MATCHING_ROOT);
    let output = parent.join(fingerprint.strip_prefix("sha256:").unwrap());
    if !parent.exists() && !verify_only {
        create_new_output_directory(&parent)?;
    }
    require_real_directory(&parent, "private matching reports")?;
    let output_exists = fs::symlink_metadata(&output).is_ok();
    ensure!(
        !verify_only || output_exists,
        "no matching report exists for these exact inputs; run match-metadata first"
    );
    let staging = if output_exists {
        require_real_directory(&output, "existing matching report")?;
        None
    } else {
        // A failed write preserves its partial report. A subsequent invocation
        // uses a new staging directory and never overwrites the preserved state.
        let staging = parent.join(format!(
            ".pending-{}-{}",
            std::process::id(),
            Utc::now()
                .timestamp_nanos_opt()
                .context("timestamp overflow")?
        ));
        create_new_output_directory(&staging)?;
        Some(staging)
    };
    let (counts, artifacts) = render_artifacts(&context, &index, &population, staging.as_deref())
        .with_context(|| {
        format!(
            "matching failed; any partial report is preserved at {}",
            staging.as_deref().unwrap_or(&output).display()
        )
    })?;
    ensure!(
        hash_file(&index_path)? == (index_sha256, index_bytes),
        "execution index changed during matching"
    );
    let manifest = MatchingManifest {
        format: "waajacu-cod-metadata-candidate-matching".to_string(),
        schema_version: 1,
        pilot_id: PILOT_ID.to_string(),
        status: "complete_unreviewed_candidates".to_string(),
        output_directory: format!(
            "{PILOT_ROOT_RELATIVE}/{MATCHING_ROOT}/{}",
            fingerprint.strip_prefix("sha256:").unwrap()
        ),
        inputs,
        policy,
        counts,
        artifacts,
    };
    if let Some(staging) = staging {
        write_new_file(&staging, MANIFEST_FILE, &canonical_file_bytes(&manifest)?)?;
        fs::rename(&staging, &output)
            .context("failed to finalize matching report; staging remains preserved")?;
    } else {
        verify_report_files(&output, &manifest)?;
    }
    Ok(manifest)
}

fn verify_report_files(output: &Path, manifest: &MatchingManifest) -> Result<()> {
    let expected: BTreeSet<String> = ARTIFACT_FILES
        .iter()
        .chain(std::iter::once(&MANIFEST_FILE))
        .map(|name| name.to_string())
        .collect();
    let mut actual = BTreeSet::new();
    for entry in fs::read_dir(output)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "matching report contains a symlink or non-file"
        );
        actual.insert(
            entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("non-Unicode artifact name"))?,
        );
    }
    ensure!(actual == expected, "matching report file set changed");
    for artifact in &manifest.artifacts {
        ensure!(
            hash_file(&output.join(&artifact.path))? == (artifact.sha256.clone(), artifact.bytes),
            "matching artifact drifted: {}",
            artifact.path
        );
    }
    ensure!(
        read_regular_file(&output.join(MANIFEST_FILE), "matching manifest")?
            == canonical_file_bytes(manifest)?,
        "matching manifest does not reproduce"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(entries: &[(&str, &str)]) -> NameIndex {
        NameIndex::new(
            &entries
                .iter()
                .map(|(id, name)| PopulationEntry {
                    material_public_id: id.to_string(),
                    canonical_name_snapshot: name.to_string(),
                })
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn exact_identity_and_exploratory_hints_remain_separate() {
        let index = names(&[("q", "Quartz"), ("s", "Söhngeite"), ("c", "Calcite")]);
        let (exact, leads) =
            index.match_names(&json!({"mineral":"Quartz", "commonname":"Calcite"}));
        assert_eq!(exact[0].material_public_id, "q");
        assert_eq!(leads.len(), 1);
        assert_eq!(leads[0].material_public_id, "c");
        for name in [" quartz ", "Quarz", "QUARTZ"] {
            let (exact, leads) = index.match_names(&json!({"mineral": name}));
            assert!(exact.is_empty());
            assert!(leads.iter().any(|lead| lead.material_public_id == "q"));
        }
        let (exact, leads) = index.match_names(&json!({"mineral":"Sohngeite"}));
        assert!(exact.is_empty());
        assert_eq!(leads[0].material_public_id, "s");
        assert!(index.match_names(&json!({"formula":"Si O2"})).0.is_empty());
        assert!(index.match_names(&json!({"formula":"Si O2"})).1.is_empty());
    }

    #[test]
    fn collisions_are_preserved_and_short_names_do_not_make_typo_leads() {
        let index = names(&[
            ("a", "Same"),
            ("b", "Same"),
            ("c", "Ice"),
            ("d", "Sohngeite"),
            ("e", "Söhngeite"),
        ]);
        assert_eq!(index.match_names(&json!({"mineral":"Same"})).0.len(), 2);
        assert_eq!(
            index.match_names(&json!({"mineral":"SOHNGEITE"})).1.len(),
            2
        );
        assert!(index.match_names(&json!({"mineral":"Ire"})).1.is_empty());
    }

    #[test]
    fn edit_distance_handles_unicode_and_rejects_shared_signature_false_positives() {
        for (a, b) in [
            ("quartz", "quarz"),
            ("calcite", "calxite"),
            ("münchen", "münchenx"),
            ("abcde", "abcde"),
        ] {
            assert!(edit_distance_at_most_one(a, b));
        }
        for (a, b) in [
            ("quartz", "quar"),
            ("abcde", "bacde"),
            ("calcite", "quartz"),
        ] {
            assert!(!edit_distance_at_most_one(a, b));
        }
    }

    #[test]
    fn every_row_keeps_locator_raw_evidence_flags_and_repeated_rows() -> Result<()> {
        let index = names(&[("q", "Quartz")]);
        let receipt = SuccessfulReceipt {
            ordinal: 900,
            ddd_prefix: "900".to_string(),
            method: "GET".to_string(),
            url: "unused".to_string(),
            request_sha256: "unused".to_string(),
            attempts: Vec::new(),
            successful_attempt_number: 1,
            response: SuccessfulResponse {
                status_code: 200,
                content_type: "application/json".to_string(),
                retrieved_at: "frozen-time".to_string(),
                raw_body: RawBodyBinding {
                    path: "raw-path".to_string(),
                    sha256: "raw-hash".to_string(),
                    bytes: 123,
                },
                json_validation: JsonValidation {
                    valid: true,
                    maximum_depth: 2,
                    row_count: 2,
                },
            },
        };
        let row = json!({"file":"9000001", "svnrevision":"123", "mineral":"Quartz", "compoundsource":"Synthetic", "duplicateof":"9000002", "formula":null, "status":"warning"});
        let mut seen = BTreeSet::new();
        let first = source_record(&row, 0, &receipt, &index, &mut seen)?;
        assert_eq!(first.disposition, "exact_candidate_needs_review");
        assert!(first
            .review_flags
            .contains(&"synthetic_text_in_compoundsource".to_string()));
        assert!(first
            .review_flags
            .contains(&"duplicate_relation_reported".to_string()));
        assert_eq!(first.raw_fields["formula"], Value::Null);
        assert!(!first.raw_fields.contains_key("chemname"));
        let second = source_record(&row, 1, &receipt, &index, &mut seen)?;
        assert!(second.repeated_metadata_row);
        assert_eq!(second.metadata_locator.json_pointer, "/1");
        let unrelated = source_record(
            &json!({"file":"9000003","svnrevision":"5","mineral":"unrelated"}),
            2,
            &receipt,
            &index,
            &mut seen,
        )?;
        assert_eq!(unrelated.disposition, "unlinked_retained");
        Ok(())
    }

    #[test]
    fn artifact_replay_is_identical_and_existing_files_are_never_overwritten() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let row = json!({"z":1,"a":{"b":2}});
        let mut writer = ArtifactWriter::new(Some(directory.path()), INVENTORY_FILE)?;
        writer.row(&row)?;
        let binding = writer.finish()?;
        let mut replay = ArtifactWriter::new(None, INVENTORY_FILE)?;
        replay.row(&row)?;
        assert_eq!(replay.finish()?, binding);
        assert_eq!(
            hash_file(&directory.path().join(INVENTORY_FILE))?,
            (binding.sha256, binding.bytes)
        );
        assert!(ArtifactWriter::new(Some(directory.path()), INVENTORY_FILE).is_err());
        Ok(())
    }
}
