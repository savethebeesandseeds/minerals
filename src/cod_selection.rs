//! Pure, deterministic challenge-sample selection for the COD crystallography pilot.
//!
//! This module performs no filesystem, database, or network I/O. Callers provide
//! the exact frozen eligibility-manifest bytes and the 60 already-selected
//! baseline public IDs. The returned entries can then be embedded in the frozen
//! 96-mineral selection manifest.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, ensure, Context, Result};
use ring::digest::{Context as DigestContext, SHA256};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

pub const CHALLENGE_SELECTION_SALT: &str = "cod-crystallography-v1-challenge-selection";
pub const CHALLENGE_SPLIT_SALT: &str = "cod-crystallography-v1-challenge-split";
pub const BASELINE_ENTRY_COUNT: usize = 60;
pub const CHALLENGE_ENTRY_COUNT: usize = 36;

const PILOT_ID: &str = "cod-crystallography-v1";
const EXPECTED_POPULATION_COUNT: u64 = 6_226;
const EXPECTED_POPULATION_RELEASE_ID: &str =
    "sha256:684c2830a145bfa0f0b0ecaf4f80bac64ddb51ffcb4a75035304840a868bccc4";
const GROUP_ENTRY_COUNT: usize = 12;
const GROUP_HOLDOUT_COUNT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeGroup {
    NomenclatureCrosswalk,
    ValueShapeApplicability,
    ProvenanceRisk,
}

impl ChallengeGroup {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NomenclatureCrosswalk => "nomenclature_crosswalk",
            Self::ValueShapeApplicability => "value_shape_applicability",
            Self::ProvenanceRisk => "provenance_risk",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeCriterion {
    AcceptedNameExactUnique,
    AuthorityAliasExactUnique,
    MultipleExactOrFormerNameAmbiguity,
    NoExactNameCandidate,
    MultipleDeterminationsOrDataBlocks,
    ParenthesizedOrSeparateUncertainty,
    UnknownOrInapplicablePlaceholder,
    NonambientTemperatureOrPressure,
    IncompleteSixParameterCell,
    ConflictingOrMultipleSpaceGroupForms,
    DuplicateOrSuboptimalRelation,
    WarningErrorOrRetractionStatus,
    TheoreticalOrSyntheticOrigin,
    MissingOrNonstandardPublicationCitation,
}

impl ChallengeCriterion {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AcceptedNameExactUnique => "accepted_name_exact_unique",
            Self::AuthorityAliasExactUnique => "authority_alias_exact_unique",
            Self::MultipleExactOrFormerNameAmbiguity => "multiple_exact_or_former_name_ambiguity",
            Self::NoExactNameCandidate => "no_exact_name_candidate",
            Self::MultipleDeterminationsOrDataBlocks => "multiple_determinations_or_data_blocks",
            Self::ParenthesizedOrSeparateUncertainty => "parenthesized_or_separate_uncertainty",
            Self::UnknownOrInapplicablePlaceholder => "unknown_or_inapplicable_placeholder",
            Self::NonambientTemperatureOrPressure => "nonambient_temperature_or_pressure",
            Self::IncompleteSixParameterCell => "incomplete_six_parameter_cell",
            Self::ConflictingOrMultipleSpaceGroupForms => {
                "conflicting_or_multiple_space_group_forms"
            }
            Self::DuplicateOrSuboptimalRelation => "duplicate_or_suboptimal_relation",
            Self::WarningErrorOrRetractionStatus => "warning_error_or_retraction_status",
            Self::TheoreticalOrSyntheticOrigin => "theoretical_or_synthetic_origin",
            Self::MissingOrNonstandardPublicationCitation => {
                "missing_or_nonstandard_publication_citation"
            }
        }
    }

    const fn group(self) -> ChallengeGroup {
        match self {
            Self::AcceptedNameExactUnique
            | Self::AuthorityAliasExactUnique
            | Self::MultipleExactOrFormerNameAmbiguity
            | Self::NoExactNameCandidate => ChallengeGroup::NomenclatureCrosswalk,
            Self::MultipleDeterminationsOrDataBlocks
            | Self::ParenthesizedOrSeparateUncertainty
            | Self::UnknownOrInapplicablePlaceholder
            | Self::NonambientTemperatureOrPressure
            | Self::IncompleteSixParameterCell
            | Self::ConflictingOrMultipleSpaceGroupForms => ChallengeGroup::ValueShapeApplicability,
            Self::DuplicateOrSuboptimalRelation
            | Self::WarningErrorOrRetractionStatus
            | Self::TheoreticalOrSyntheticOrigin
            | Self::MissingOrNonstandardPublicationCitation => ChallengeGroup::ProvenanceRisk,
        }
    }
}

const NOMENCLATURE_STRATA: &[(ChallengeCriterion, usize)] = &[
    (ChallengeCriterion::AcceptedNameExactUnique, 3),
    (ChallengeCriterion::AuthorityAliasExactUnique, 3),
    (ChallengeCriterion::MultipleExactOrFormerNameAmbiguity, 3),
    (ChallengeCriterion::NoExactNameCandidate, 3),
];

const VALUE_SHAPE_STRATA: &[(ChallengeCriterion, usize)] = &[
    (ChallengeCriterion::MultipleDeterminationsOrDataBlocks, 2),
    (ChallengeCriterion::ParenthesizedOrSeparateUncertainty, 2),
    (ChallengeCriterion::UnknownOrInapplicablePlaceholder, 2),
    (ChallengeCriterion::NonambientTemperatureOrPressure, 2),
    (ChallengeCriterion::IncompleteSixParameterCell, 2),
    (ChallengeCriterion::ConflictingOrMultipleSpaceGroupForms, 2),
];

const PROVENANCE_STRATA: &[(ChallengeCriterion, usize)] = &[
    (ChallengeCriterion::DuplicateOrSuboptimalRelation, 3),
    (ChallengeCriterion::WarningErrorOrRetractionStatus, 3),
    (ChallengeCriterion::TheoreticalOrSyntheticOrigin, 3),
    (
        ChallengeCriterion::MissingOrNonstandardPublicationCitation,
        3,
    ),
];

const GROUP_SPECS: &[(ChallengeGroup, &[(ChallengeCriterion, usize)])] = &[
    (ChallengeGroup::NomenclatureCrosswalk, NOMENCLATURE_STRATA),
    (ChallengeGroup::ValueShapeApplicability, VALUE_SHAPE_STRATA),
    (ChallengeGroup::ProvenanceRisk, PROVENANCE_STRATA),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeEligibilityManifest {
    pub format: String,
    pub schema_version: u32,
    pub pilot_id: String,
    pub manifest_revision: u32,
    pub status: String,
    pub frozen_at: String,
    pub pilot_contract: PilotContractBinding,
    pub population: EligibilityPopulationBinding,
    pub discovery_snapshot: DiscoverySnapshotBinding,
    pub freeze_guard: FreezeGuard,
    pub eligibility_rule: EligibilityRule,
    pub entries: Vec<ChallengeEligibilityEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PilotContractBinding {
    pub path: String,
    pub sha256: String,
    pub contract_revision: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EligibilityPopulationBinding {
    pub release_id: String,
    pub mineral_count: u64,
    pub identity_policy: String,
    pub snapshot_path: String,
    pub snapshot_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoverySnapshotBinding {
    pub path: String,
    pub sha256: String,
    pub query_configuration_sha256: String,
    pub frozen_at: String,
    pub raw_candidate_discovery_complete: bool,
    pub contains_normalized_adapter_output: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreezeGuard {
    pub raw_candidate_discovery_complete: bool,
    pub discovery_snapshot_frozen: bool,
    pub all_entries_reviewed: bool,
    pub normalized_adapter_output_inspected: bool,
    pub normalized_adapter_output_used_for_eligibility: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EligibilityRule {
    pub id: String,
    pub version: u32,
    pub assignment: String,
    pub allowed_evidence: Vec<String>,
    pub normalized_output_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeEligibilityEntry {
    pub material_public_id: String,
    pub canonical_name_snapshot: String,
    pub decision: String,
    pub challenge_group: ChallengeGroup,
    pub primary_stratum: ChallengeCriterion,
    pub criterion_tags: Vec<ChallengeCriterion>,
    pub qualifying_evidence: Vec<EligibilityEvidence>,
    pub rule_version: u32,
    pub review: EligibilityReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSourceKind {
    PopulationIdentitySnapshot,
    CodCandidateDiscoveryResponse,
    CodRevisionPinnedCif,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceBasis {
    DirectRawFact,
    AbsenceInCompleteFrozenResult,
    ComparisonOfRawMetadataToAuthorityIdentity,
    ContentAddressedRelationOrStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EligibilityEvidence {
    pub evidence_id: String,
    pub source_kind: EvidenceSourceKind,
    pub source_artifact_sha256: String,
    pub source_locator: String,
    pub basis: EvidenceBasis,
    pub supports_criterion: ChallengeCriterion,
    pub observed_fact: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EligibilityReview {
    pub status: String,
    pub curator: String,
    pub reviewed_at: String,
    pub rationale: String,
    pub evidence_verified: bool,
    pub normalized_adapter_output_inspected: bool,
}

/// A challenge entry in the exact shape required by the selection-manifest schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeSelectionEntry {
    pub position: u32,
    pub material_public_id: String,
    pub canonical_name_snapshot: String,
    pub selection_kind: String,
    pub split: String,
    pub selection_digest: String,
    pub challenge_group: String,
    pub primary_stratum: String,
    pub criterion_tags: Vec<String>,
    pub qualifying_evidence_ids: Vec<String>,
}

/// One exact identity row from the frozen 6,226-mineral population snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopulationIdentity {
    pub material_public_id: String,
    pub canonical_name_snapshot: String,
}

/// Borrowed population identities plus the independently verified artifact binding.
#[derive(Debug, Clone, Copy)]
pub struct FrozenPopulationView<'a> {
    pub release_id: &'a str,
    pub mineral_count: u64,
    pub snapshot_path: &'a str,
    pub snapshot_sha256: &'a str,
    pub entries: &'a [PopulationIdentity],
}

#[derive(Debug)]
struct RankedEligibility<'a> {
    digest: [u8; 32],
    entry: &'a ChallengeEligibilityEntry,
}

/// Parse, fully validate, and deterministically select the 36 challenge entries.
pub fn select_challenge_entries_from_json(
    eligibility_json: &[u8],
    population: &FrozenPopulationView<'_>,
    baseline_public_ids: &[String],
) -> Result<Vec<ChallengeSelectionEntry>> {
    let manifest: ChallengeEligibilityManifest = serde_json::from_slice(eligibility_json)
        .context("failed to parse frozen challenge eligibility manifest")?;
    select_challenge_entries(&manifest, population, baseline_public_ids)
}

/// Validate a parsed eligibility manifest and select exactly 36 challenge entries.
pub fn select_challenge_entries(
    manifest: &ChallengeEligibilityManifest,
    population: &FrozenPopulationView<'_>,
    baseline_public_ids: &[String],
) -> Result<Vec<ChallengeSelectionEntry>> {
    let population_identities = validate_population(population)?;
    let baseline = validate_baseline_ids(baseline_public_ids, &population_identities)?;
    validate_eligibility_manifest(manifest, population, &population_identities, &baseline)?;

    let mut candidates =
        BTreeMap::<(ChallengeGroup, ChallengeCriterion), Vec<&ChallengeEligibilityEntry>>::new();
    for entry in &manifest.entries {
        if !baseline.contains(&entry.material_public_id) {
            candidates
                .entry((entry.challenge_group, entry.primary_stratum))
                .or_default()
                .push(entry);
        }
    }

    let mut selected = Vec::<RankedEligibility<'_>>::with_capacity(CHALLENGE_ENTRY_COUNT);
    let mut selected_ids = BTreeSet::new();
    for &(group, strata) in GROUP_SPECS {
        let group_start = selected.len();
        for &(stratum, quota) in strata {
            let available = candidates
                .get(&(group, stratum))
                .map(Vec::as_slice)
                .unwrap_or_default();
            ensure!(
                available.len() >= quota,
                "challenge stratum {}/{} has {} non-baseline eligible entries, requires {quota}",
                group.as_str(),
                stratum.as_str(),
                available.len()
            );

            let mut ranked = available
                .iter()
                .map(|entry| RankedEligibility {
                    digest: challenge_selection_digest(group, stratum, &entry.material_public_id),
                    entry,
                })
                .collect::<Vec<_>>();
            ranked.sort_unstable_by(|left, right| {
                left.digest.cmp(&right.digest).then_with(|| {
                    left.entry
                        .material_public_id
                        .as_bytes()
                        .cmp(right.entry.material_public_id.as_bytes())
                })
            });

            for ranked in ranked.into_iter().take(quota) {
                ensure!(
                    selected_ids.insert(ranked.entry.material_public_id.as_str()),
                    "challenge selection attempted to select {} more than once",
                    ranked.entry.material_public_id
                );
                selected.push(ranked);
            }
        }
        ensure!(
            selected.len() - group_start == GROUP_ENTRY_COUNT,
            "challenge group {} did not select exactly 12 entries",
            group.as_str()
        );
    }
    ensure!(
        selected.len() == CHALLENGE_ENTRY_COUNT,
        "challenge selection produced {} entries, expected 36",
        selected.len()
    );

    let holdout_ids = challenge_holdout_ids(&selected)?;
    let mut output = Vec::with_capacity(CHALLENGE_ENTRY_COUNT);
    for (index, selected) in selected.into_iter().enumerate() {
        let mut criterion_tags = selected
            .entry
            .criterion_tags
            .iter()
            .map(|criterion| criterion.as_str().to_string())
            .collect::<Vec<_>>();
        criterion_tags.sort_unstable_by(|left, right| left.as_bytes().cmp(right.as_bytes()));

        let mut evidence_ids = selected
            .entry
            .qualifying_evidence
            .iter()
            .map(|evidence| evidence.evidence_id.clone())
            .collect::<Vec<_>>();
        evidence_ids.sort_unstable_by(|left, right| left.as_bytes().cmp(right.as_bytes()));

        output.push(ChallengeSelectionEntry {
            position: (BASELINE_ENTRY_COUNT + index + 1) as u32,
            material_public_id: selected.entry.material_public_id.clone(),
            canonical_name_snapshot: selected.entry.canonical_name_snapshot.clone(),
            selection_kind: "challenge".to_string(),
            split: if holdout_ids.contains(&selected.entry.material_public_id) {
                "holdout"
            } else {
                "development"
            }
            .to_string(),
            selection_digest: format_sha256_identifier(&selected.digest),
            challenge_group: selected.entry.challenge_group.as_str().to_string(),
            primary_stratum: selected.entry.primary_stratum.as_str().to_string(),
            criterion_tags,
            qualifying_evidence_ids: evidence_ids,
        });
    }

    let development_count = output
        .iter()
        .filter(|entry| entry.split == "development")
        .count();
    let holdout_count = output
        .iter()
        .filter(|entry| entry.split == "holdout")
        .count();
    ensure!(
        development_count == 27 && holdout_count == 9,
        "challenge split produced {development_count} development and {holdout_count} holdout entries"
    );
    Ok(output)
}

/// Exact challenge-ranking digest from the frozen pilot contract.
pub fn challenge_selection_digest(
    group: ChallengeGroup,
    stratum: ChallengeCriterion,
    material_public_id: &str,
) -> [u8; 32] {
    digest_components(
        CHALLENGE_SELECTION_SALT,
        &[group.as_str(), stratum.as_str(), material_public_id],
    )
}

/// Exact per-group challenge split digest from the frozen pilot contract.
pub fn challenge_split_digest(group: ChallengeGroup, material_public_id: &str) -> [u8; 32] {
    digest_components(CHALLENGE_SPLIT_SALT, &[group.as_str(), material_public_id])
}

fn digest_components(salt: &str, components: &[&str]) -> [u8; 32] {
    let mut context = DigestContext::new(&SHA256);
    context.update(salt.as_bytes());
    for component in components {
        context.update(&[0_u8]);
        context.update(component.as_bytes());
    }
    context
        .finish()
        .as_ref()
        .try_into()
        .expect("SHA-256 always has 32 bytes")
}

fn challenge_holdout_ids(selected: &[RankedEligibility<'_>]) -> Result<BTreeSet<String>> {
    let mut holdouts = BTreeSet::new();
    for &(group, _) in GROUP_SPECS {
        let mut ranked = selected
            .iter()
            .filter(|selected| selected.entry.challenge_group == group)
            .map(|selected| {
                (
                    challenge_split_digest(group, &selected.entry.material_public_id),
                    selected.entry.material_public_id.as_str(),
                )
            })
            .collect::<Vec<_>>();
        ensure!(
            ranked.len() == GROUP_ENTRY_COUNT,
            "challenge group {} has {} selected entries, expected 12",
            group.as_str(),
            ranked.len()
        );
        ranked.sort_unstable_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.as_bytes().cmp(right.1.as_bytes()))
        });
        for (_, material_public_id) in ranked.into_iter().take(GROUP_HOLDOUT_COUNT) {
            ensure!(
                holdouts.insert(material_public_id.to_string()),
                "challenge split repeated material_public_id {material_public_id}"
            );
        }
    }
    ensure!(
        holdouts.len() == 9,
        "challenge split must contain 9 holdouts"
    );
    Ok(holdouts)
}

fn validate_population<'a>(
    population: &'a FrozenPopulationView<'a>,
) -> Result<BTreeMap<&'a str, &'a str>> {
    validate_sha256_identifier(population.release_id, "population release_id")?;
    ensure!(
        population.release_id == EXPECTED_POPULATION_RELEASE_ID,
        "frozen population release_id does not match the pilot contract"
    );
    ensure!(
        population.mineral_count == EXPECTED_POPULATION_COUNT,
        "frozen population binding count is not 6226"
    );
    validate_artifact_path(population.snapshot_path, "population snapshot path")?;
    validate_sha256_identifier(population.snapshot_sha256, "population snapshot sha256")?;
    ensure!(
        population.entries.len() as u64 == population.mineral_count,
        "frozen population contains {} identities, binding declares {}",
        population.entries.len(),
        population.mineral_count
    );

    let mut identities = BTreeMap::new();
    let mut names = BTreeSet::new();
    for entry in population.entries {
        validate_material_public_id(&entry.material_public_id)?;
        validate_nonempty_text(
            &entry.canonical_name_snapshot,
            500,
            "population canonical_name_snapshot",
        )?;
        ensure!(
            entry
                .canonical_name_snapshot
                .nfc()
                .eq(entry.canonical_name_snapshot.chars()),
            "population canonical_name_snapshot is not NFC for {}",
            entry.material_public_id
        );
        ensure!(
            identities
                .insert(
                    entry.material_public_id.as_str(),
                    entry.canonical_name_snapshot.as_str()
                )
                .is_none(),
            "frozen population repeats material_public_id {}",
            entry.material_public_id
        );
        ensure!(
            names.insert(entry.canonical_name_snapshot.as_str()),
            "frozen population repeats canonical_name_snapshot {}",
            entry.canonical_name_snapshot
        );
    }
    Ok(identities)
}

fn validate_baseline_ids(
    ids: &[String],
    population: &BTreeMap<&str, &str>,
) -> Result<BTreeSet<String>> {
    ensure!(
        ids.len() == BASELINE_ENTRY_COUNT,
        "baseline exclusion set contains {} IDs, expected 60",
        ids.len()
    );
    let mut unique = BTreeSet::new();
    for id in ids {
        validate_material_public_id(id)?;
        ensure!(
            population.contains_key(id.as_str()),
            "baseline material_public_id {id} is absent from the frozen population"
        );
        ensure!(
            unique.insert(id.clone()),
            "baseline exclusion set repeats material_public_id {id}"
        );
    }
    Ok(unique)
}

fn validate_eligibility_manifest(
    manifest: &ChallengeEligibilityManifest,
    population_binding: &FrozenPopulationView<'_>,
    population: &BTreeMap<&str, &str>,
    baseline: &BTreeSet<String>,
) -> Result<()> {
    ensure!(
        manifest.format == "waajacu-cod-crystallography-challenge-eligibility-manifest",
        "invalid challenge eligibility format"
    );
    ensure!(manifest.schema_version == 1, "invalid schema_version");
    ensure!(manifest.pilot_id == PILOT_ID, "invalid pilot_id");
    ensure!(manifest.manifest_revision >= 1, "invalid manifest_revision");
    ensure!(
        manifest.status == "frozen_before_normalized_output",
        "eligibility manifest is not frozen before normalized output"
    );
    validate_nonempty_text(&manifest.frozen_at, 100, "frozen_at")?;

    ensure!(
        manifest.pilot_contract.path == "schemas/pilots/cod-crystallography-pilot-v1.json",
        "invalid pilot contract path"
    );
    validate_sha256_identifier(&manifest.pilot_contract.sha256, "pilot contract sha256")?;
    ensure!(
        manifest.pilot_contract.contract_revision == 1,
        "invalid pilot contract revision"
    );

    validate_sha256_identifier(&manifest.population.release_id, "population release_id")?;
    ensure!(
        manifest.population.release_id == population_binding.release_id,
        "eligibility population release_id does not match the frozen population binding"
    );
    ensure!(
        manifest.population.mineral_count == population_binding.mineral_count
            && manifest.population.mineral_count == EXPECTED_POPULATION_COUNT,
        "eligibility population count does not match the frozen population binding"
    );
    ensure!(
        manifest.population.identity_policy == "fixed_existing_public_ids_only",
        "invalid eligibility population identity policy"
    );
    validate_artifact_path(
        &manifest.population.snapshot_path,
        "population snapshot path",
    )?;
    ensure!(
        manifest.population.snapshot_path == population_binding.snapshot_path,
        "eligibility population snapshot path does not match the frozen population binding"
    );
    validate_sha256_identifier(
        &manifest.population.snapshot_sha256,
        "population snapshot sha256",
    )?;
    ensure!(
        manifest.population.snapshot_sha256 == population_binding.snapshot_sha256,
        "eligibility population snapshot sha256 does not match the frozen population binding"
    );

    validate_artifact_path(&manifest.discovery_snapshot.path, "discovery snapshot path")?;
    validate_sha256_identifier(
        &manifest.discovery_snapshot.sha256,
        "discovery snapshot sha256",
    )?;
    validate_sha256_identifier(
        &manifest.discovery_snapshot.query_configuration_sha256,
        "discovery query configuration sha256",
    )?;
    validate_nonempty_text(
        &manifest.discovery_snapshot.frozen_at,
        100,
        "discovery snapshot frozen_at",
    )?;
    ensure!(
        manifest.discovery_snapshot.raw_candidate_discovery_complete,
        "raw candidate discovery is not complete"
    );
    ensure!(
        !manifest
            .discovery_snapshot
            .contains_normalized_adapter_output,
        "discovery snapshot contains normalized adapter output"
    );

    ensure!(
        manifest.freeze_guard.raw_candidate_discovery_complete
            && manifest.freeze_guard.discovery_snapshot_frozen
            && manifest.freeze_guard.all_entries_reviewed,
        "eligibility freeze guard is incomplete"
    );
    ensure!(
        !manifest.freeze_guard.normalized_adapter_output_inspected
            && !manifest
                .freeze_guard
                .normalized_adapter_output_used_for_eligibility,
        "normalized adapter output crossed the eligibility freeze boundary"
    );

    ensure!(
        manifest.eligibility_rule.id == "cod-crystallography-challenge-eligibility-v1"
            && manifest.eligibility_rule.version == 1,
        "invalid eligibility rule"
    );
    ensure!(
        manifest.eligibility_rule.assignment
            == "one_reviewed_primary_group_and_stratum_per_exact_public_id",
        "invalid eligibility assignment rule"
    );
    ensure!(
        manifest.eligibility_rule.allowed_evidence.len() == 2
            && manifest.eligibility_rule.allowed_evidence[0]
                == "frozen_population_identity_snapshot"
            && manifest.eligibility_rule.allowed_evidence[1]
                == "frozen_raw_candidate_discovery_snapshot",
        "invalid or reordered eligibility allowed_evidence"
    );
    ensure!(
        manifest.eligibility_rule.normalized_output_policy
            == "inspection_and_use_for_eligibility_forbidden",
        "invalid normalized output policy"
    );
    ensure!(
        (CHALLENGE_ENTRY_COUNT..=EXPECTED_POPULATION_COUNT as usize)
            .contains(&manifest.entries.len()),
        "eligibility manifest contains {} entries, expected 36..=6226",
        manifest.entries.len()
    );

    let mut material_ids = BTreeSet::new();
    let mut canonical_names = BTreeSet::new();
    let mut evidence_ids = BTreeSet::new();
    let mut nonbaseline_counts = BTreeMap::<(ChallengeGroup, ChallengeCriterion), usize>::new();

    for entry in &manifest.entries {
        validate_material_public_id(&entry.material_public_id)?;
        ensure!(
            material_ids.insert(entry.material_public_id.as_str()),
            "eligibility manifest repeats material_public_id {}",
            entry.material_public_id
        );
        validate_nonempty_text(
            &entry.canonical_name_snapshot,
            500,
            "canonical_name_snapshot",
        )?;
        ensure!(
            entry
                .canonical_name_snapshot
                .nfc()
                .eq(entry.canonical_name_snapshot.chars()),
            "canonical_name_snapshot is not NFC for {}",
            entry.material_public_id
        );
        ensure!(
            canonical_names.insert(entry.canonical_name_snapshot.as_str()),
            "eligibility manifest repeats canonical_name_snapshot {}",
            entry.canonical_name_snapshot
        );
        let population_name = population
            .get(entry.material_public_id.as_str())
            .with_context(|| {
                format!(
                    "eligibility material_public_id {} is absent from the frozen population",
                    entry.material_public_id
                )
            })?;
        ensure!(
            *population_name == entry.canonical_name_snapshot.as_str(),
            "eligibility canonical_name_snapshot for {} does not match the frozen population",
            entry.material_public_id
        );
        ensure!(
            entry.decision == "eligible",
            "entry decision is not eligible"
        );
        ensure!(
            entry.primary_stratum.group() == entry.challenge_group,
            "invalid group/stratum pairing {}/{} for {}",
            entry.challenge_group.as_str(),
            entry.primary_stratum.as_str(),
            entry.material_public_id
        );
        ensure!(entry.rule_version == 1, "invalid entry rule_version");

        ensure!(
            !entry.criterion_tags.is_empty(),
            "entry {} has no criterion tags",
            entry.material_public_id
        );
        let tags = entry
            .criterion_tags
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        ensure!(
            tags.len() == entry.criterion_tags.len(),
            "entry {} repeats a criterion tag",
            entry.material_public_id
        );
        ensure!(
            tags.contains(&entry.primary_stratum),
            "entry {} omits its primary stratum from criterion_tags",
            entry.material_public_id
        );

        ensure!(
            (1..=100).contains(&entry.qualifying_evidence.len()),
            "entry {} must contain 1..=100 qualifying evidence items",
            entry.material_public_id
        );
        let mut supports_primary = false;
        for evidence in &entry.qualifying_evidence {
            validate_sha256_identifier(&evidence.evidence_id, "evidence_id")?;
            ensure!(
                evidence_ids.insert(evidence.evidence_id.as_str()),
                "eligibility manifest repeats evidence_id {}",
                evidence.evidence_id
            );
            validate_sha256_identifier(
                &evidence.source_artifact_sha256,
                "evidence source_artifact_sha256",
            )?;
            validate_nonempty_text(&evidence.source_locator, 2_000, "evidence source_locator")?;
            validate_nonempty_text(&evidence.observed_fact, 4_000, "evidence observed_fact")?;
            ensure!(
                tags.contains(&evidence.supports_criterion),
                "evidence {} supports a criterion absent from entry {} tags",
                evidence.evidence_id,
                entry.material_public_id
            );
            supports_primary |= evidence.supports_criterion == entry.primary_stratum;
        }
        ensure!(
            supports_primary,
            "entry {} has no evidence supporting its primary stratum",
            entry.material_public_id
        );

        ensure!(entry.review.status == "reviewed", "entry is not reviewed");
        validate_nonempty_text(&entry.review.curator, 200, "review curator")?;
        validate_nonempty_text(&entry.review.reviewed_at, 100, "reviewed_at")?;
        validate_nonempty_text(&entry.review.rationale, 4_000, "review rationale")?;
        ensure!(
            entry.review.evidence_verified,
            "entry review does not verify evidence"
        );
        ensure!(
            !entry.review.normalized_adapter_output_inspected,
            "entry review inspected normalized adapter output"
        );

        if !baseline.contains(&entry.material_public_id) {
            *nonbaseline_counts
                .entry((entry.challenge_group, entry.primary_stratum))
                .or_default() += 1;
        }
    }

    for &(group, strata) in GROUP_SPECS {
        for &(stratum, quota) in strata {
            let count = nonbaseline_counts
                .get(&(group, stratum))
                .copied()
                .unwrap_or_default();
            ensure!(
                count >= quota,
                "challenge stratum {}/{} has {count} eligible non-baseline entries, requires {quota}",
                group.as_str(),
                stratum.as_str()
            );
        }
    }
    Ok(())
}

fn validate_material_public_id(value: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("mat_") else {
        bail!("invalid material_public_id {value}");
    };
    ensure!(
        hex.len() == 32
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "invalid material_public_id {value}"
    );
    Ok(())
}

fn validate_sha256_identifier(value: &str, label: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        bail!("{label} is not a sha256 identifier");
    };
    ensure!(
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{label} is not a lowercase sha256 identifier"
    );
    Ok(())
}

fn validate_artifact_path(value: &str, label: &str) -> Result<()> {
    ensure!(
        value.len() <= 1_000
            && value.starts_with("data/pilots/cod-crystallography-v1/")
            && !value.contains('\\')
            && value
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."),
        "invalid {label}"
    );
    Ok(())
}

fn validate_nonempty_text(value: &str, maximum_chars: usize, label: &str) -> Result<()> {
    let count = value.chars().count();
    ensure!(
        count > 0 && count <= maximum_chars,
        "{label} must contain 1..={maximum_chars} Unicode scalars"
    );
    Ok(())
}

fn format_sha256_identifier(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity("sha256:".len() + bytes.len() * 2);
    output.push_str("sha256:");
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_digest_vectors_preserve_every_nul_separator() {
        let id = "mat_00000000000000000000000000000001";
        assert_eq!(
            format_sha256_identifier(&challenge_selection_digest(
                ChallengeGroup::NomenclatureCrosswalk,
                ChallengeCriterion::AcceptedNameExactUnique,
                id,
            )),
            "sha256:521e0aa699811fc211ac623e8298d9aa9d6c38d23392dbad52d9ea3eb5595476"
        );
        assert_eq!(
            format_sha256_identifier(&challenge_split_digest(
                ChallengeGroup::NomenclatureCrosswalk,
                id,
            )),
            "sha256:60292cd77bb803b691f23abf8e2732326053dcbddab5878853a707577b80d49e"
        );
    }

    #[test]
    fn selects_36_in_declared_order_and_splits_each_group_nine_three() -> Result<()> {
        let (manifest, population, baseline) = fixture();
        let population = population_view(&population);
        let selected = select_challenge_entries(&manifest, &population, &baseline)?;

        assert_eq!(selected.len(), 36);
        assert_eq!(selected.first().unwrap().position, 61);
        assert_eq!(selected.last().unwrap().position, 96);
        assert!(selected
            .iter()
            .all(|entry| !baseline.contains(&entry.material_public_id)));
        assert_eq!(
            selected
                .iter()
                .map(|entry| entry.material_public_id.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            36
        );

        for (group, _) in GROUP_SPECS {
            let entries = selected
                .iter()
                .filter(|entry| entry.challenge_group == group.as_str())
                .collect::<Vec<_>>();
            assert_eq!(entries.len(), 12);
            assert_eq!(
                entries
                    .iter()
                    .filter(|entry| entry.split == "development")
                    .count(),
                9
            );
            assert_eq!(
                entries
                    .iter()
                    .filter(|entry| entry.split == "holdout")
                    .count(),
                3
            );

            let mut expected_holdouts = entries
                .iter()
                .map(|entry| {
                    (
                        challenge_split_digest(*group, &entry.material_public_id),
                        entry.material_public_id.as_str(),
                    )
                })
                .collect::<Vec<_>>();
            expected_holdouts.sort_unstable_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then_with(|| left.1.as_bytes().cmp(right.1.as_bytes()))
            });
            let expected_holdouts = expected_holdouts
                .into_iter()
                .take(3)
                .map(|(_, id)| id)
                .collect::<BTreeSet<_>>();
            let actual_holdouts = entries
                .iter()
                .filter(|entry| entry.split == "holdout")
                .map(|entry| entry.material_public_id.as_str())
                .collect::<BTreeSet<_>>();
            assert_eq!(actual_holdouts, expected_holdouts);
        }
        Ok(())
    }

    #[test]
    fn equivalent_entry_order_produces_identical_selection() -> Result<()> {
        let (manifest, population, baseline) = fixture();
        let population = population_view(&population);
        let expected = select_challenge_entries(&manifest, &population, &baseline)?;
        let mut reversed = manifest;
        reversed.entries.reverse();
        assert_eq!(
            select_challenge_entries(&reversed, &population, &baseline)?,
            expected
        );
        Ok(())
    }

    #[test]
    fn rejects_duplicate_material_name_and_evidence_keys() {
        let (manifest, population, baseline) = fixture();
        let population = population_view(&population);

        let mut duplicate_id = manifest.clone();
        duplicate_id.entries[1].material_public_id =
            duplicate_id.entries[0].material_public_id.clone();
        assert!(
            select_challenge_entries(&duplicate_id, &population, &baseline)
                .unwrap_err()
                .to_string()
                .contains("repeats material_public_id")
        );

        let mut duplicate_name = manifest.clone();
        duplicate_name.entries[1].canonical_name_snapshot =
            duplicate_name.entries[0].canonical_name_snapshot.clone();
        assert!(
            select_challenge_entries(&duplicate_name, &population, &baseline)
                .unwrap_err()
                .to_string()
                .contains("repeats canonical_name_snapshot")
        );

        let mut duplicate_evidence = manifest;
        duplicate_evidence.entries[1].qualifying_evidence[0].evidence_id =
            duplicate_evidence.entries[0].qualifying_evidence[0]
                .evidence_id
                .clone();
        assert!(
            select_challenge_entries(&duplicate_evidence, &population, &baseline)
                .unwrap_err()
                .to_string()
                .contains("repeats evidence_id")
        );
    }

    #[test]
    fn rejects_wrong_group_pairing_and_post_baseline_shortfall() {
        let (manifest, population, baseline) = fixture();
        let population = population_view(&population);

        let mut wrong_group = manifest.clone();
        wrong_group.entries[0].challenge_group = ChallengeGroup::ProvenanceRisk;
        assert!(
            select_challenge_entries(&wrong_group, &population, &baseline)
                .unwrap_err()
                .to_string()
                .contains("invalid group/stratum pairing")
        );

        let mut short = manifest;
        short.entries.retain(|entry| {
            entry.primary_stratum != ChallengeCriterion::AcceptedNameExactUnique
                || baseline.contains(&entry.material_public_id)
        });
        assert!(select_challenge_entries(&short, &population, &baseline)
            .unwrap_err()
            .to_string()
            .contains("requires 3"));
    }

    #[test]
    fn rejects_population_binding_drift_absent_ids_and_name_drift() {
        let (manifest, population, baseline) = fixture();
        let population = population_view(&population);

        let mut wrong_binding = manifest.clone();
        wrong_binding.population.snapshot_sha256 = sha256_id(999_999);
        assert!(
            select_challenge_entries(&wrong_binding, &population, &baseline)
                .unwrap_err()
                .to_string()
                .contains("snapshot sha256 does not match")
        );

        let mut absent = manifest.clone();
        absent.entries[0].material_public_id = material_id(99_999);
        assert!(select_challenge_entries(&absent, &population, &baseline)
            .unwrap_err()
            .to_string()
            .contains("absent from the frozen population"));

        let mut changed_name = manifest;
        changed_name.entries[0].canonical_name_snapshot = "Changed authority name".to_string();
        assert!(
            select_challenge_entries(&changed_name, &population, &baseline)
                .unwrap_err()
                .to_string()
                .contains("does not match the frozen population")
        );
    }

    #[test]
    fn json_entry_point_denies_unknown_fields() {
        let (manifest, population, baseline) = fixture();
        let population = population_view(&population);
        let mut value = serde_json::to_value(manifest).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_string(), serde_json::Value::Bool(true));
        assert!(select_challenge_entries_from_json(
            &serde_json::to_vec(&value).unwrap(),
            &population,
            &baseline
        )
        .unwrap_err()
        .to_string()
        .contains("failed to parse"));
    }

    fn fixture() -> (
        ChallengeEligibilityManifest,
        Vec<PopulationIdentity>,
        Vec<String>,
    ) {
        let population = (0..EXPECTED_POPULATION_COUNT)
            .map(|number| PopulationIdentity {
                material_public_id: material_id(number),
                canonical_name_snapshot: population_name(number),
            })
            .collect::<Vec<_>>();
        let baseline = (0..BASELINE_ENTRY_COUNT)
            .map(|index| material_id(index as u64))
            .collect::<Vec<_>>();
        let mut entries = Vec::new();
        let mut next_id = 1_000_u64;
        let mut evidence_number = 1_u64;

        for &(group, strata) in GROUP_SPECS {
            for &(stratum, quota) in strata {
                for candidate_index in 0..=quota {
                    let number = if group == ChallengeGroup::NomenclatureCrosswalk
                        && stratum == ChallengeCriterion::AcceptedNameExactUnique
                        && candidate_index == 0
                    {
                        0
                    } else {
                        let number = next_id;
                        next_id += 1;
                        number
                    };
                    entries.push(ChallengeEligibilityEntry {
                        material_public_id: material_id(number),
                        canonical_name_snapshot: population_name(number),
                        decision: "eligible".to_string(),
                        challenge_group: group,
                        primary_stratum: stratum,
                        criterion_tags: vec![stratum],
                        qualifying_evidence: vec![EligibilityEvidence {
                            evidence_id: sha256_id(evidence_number),
                            source_kind: EvidenceSourceKind::CodCandidateDiscoveryResponse,
                            source_artifact_sha256: sha256_id(9_000 + evidence_number),
                            source_locator: format!("request/{evidence_number}"),
                            basis: EvidenceBasis::DirectRawFact,
                            supports_criterion: stratum,
                            observed_fact: "Reviewed frozen raw fact".to_string(),
                        }],
                        rule_version: 1,
                        review: EligibilityReview {
                            status: "reviewed".to_string(),
                            curator: "Fixture curator".to_string(),
                            reviewed_at: "2026-08-29T12:00:00Z".to_string(),
                            rationale: "Fixture eligibility rationale".to_string(),
                            evidence_verified: true,
                            normalized_adapter_output_inspected: false,
                        },
                    });
                    evidence_number += 1;
                }
            }
        }

        (
            ChallengeEligibilityManifest {
                format: "waajacu-cod-crystallography-challenge-eligibility-manifest".to_string(),
                schema_version: 1,
                pilot_id: PILOT_ID.to_string(),
                manifest_revision: 1,
                status: "frozen_before_normalized_output".to_string(),
                frozen_at: "2026-08-29T13:00:00Z".to_string(),
                pilot_contract: PilotContractBinding {
                    path: "schemas/pilots/cod-crystallography-pilot-v1.json".to_string(),
                    sha256: sha256_id(10_001),
                    contract_revision: 1,
                },
                population: EligibilityPopulationBinding {
                    release_id: POPULATION_RELEASE_ID.to_string(),
                    mineral_count: EXPECTED_POPULATION_COUNT,
                    identity_policy: "fixed_existing_public_ids_only".to_string(),
                    snapshot_path: POPULATION_SNAPSHOT_PATH.to_string(),
                    snapshot_sha256: POPULATION_SNAPSHOT_SHA256.to_string(),
                },
                discovery_snapshot: DiscoverySnapshotBinding {
                    path: "data/pilots/cod-crystallography-v1/discovery-index.json".to_string(),
                    sha256: sha256_id(10_004),
                    query_configuration_sha256: sha256_id(10_005),
                    frozen_at: "2026-08-29T11:00:00Z".to_string(),
                    raw_candidate_discovery_complete: true,
                    contains_normalized_adapter_output: false,
                },
                freeze_guard: FreezeGuard {
                    raw_candidate_discovery_complete: true,
                    discovery_snapshot_frozen: true,
                    all_entries_reviewed: true,
                    normalized_adapter_output_inspected: false,
                    normalized_adapter_output_used_for_eligibility: false,
                },
                eligibility_rule: EligibilityRule {
                    id: "cod-crystallography-challenge-eligibility-v1".to_string(),
                    version: 1,
                    assignment: "one_reviewed_primary_group_and_stratum_per_exact_public_id"
                        .to_string(),
                    allowed_evidence: vec![
                        "frozen_population_identity_snapshot".to_string(),
                        "frozen_raw_candidate_discovery_snapshot".to_string(),
                    ],
                    normalized_output_policy: "inspection_and_use_for_eligibility_forbidden"
                        .to_string(),
                },
                entries,
            },
            population,
            baseline,
        )
    }

    const POPULATION_RELEASE_ID: &str =
        "sha256:684c2830a145bfa0f0b0ecaf4f80bac64ddb51ffcb4a75035304840a868bccc4";
    const POPULATION_SNAPSHOT_PATH: &str =
        "data/pilots/cod-crystallography-v1/population-snapshot.json";
    const POPULATION_SNAPSHOT_SHA256: &str =
        "sha256:0983ee8ed58468c821845caed0b0e396ffd2dc06ef3fde7b04dbbd6a677f2753";

    fn population_view(population: &[PopulationIdentity]) -> FrozenPopulationView<'_> {
        FrozenPopulationView {
            release_id: POPULATION_RELEASE_ID,
            mineral_count: EXPECTED_POPULATION_COUNT,
            snapshot_path: POPULATION_SNAPSHOT_PATH,
            snapshot_sha256: POPULATION_SNAPSHOT_SHA256,
            entries: population,
        }
    }

    fn material_id(number: u64) -> String {
        format!("mat_{number:032x}")
    }

    fn population_name(number: u64) -> String {
        format!("Population mineral {number}")
    }

    fn sha256_id(number: u64) -> String {
        format!("sha256:{number:064x}")
    }
}
