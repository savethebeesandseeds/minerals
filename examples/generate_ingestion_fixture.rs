use anyhow::{bail, Context, Result};
use minerals::registry::{
    canonical_mineral_chunk_hash, canonical_mineral_manifest_hash, canonical_mineral_records_hash,
    MineralArtifactDescriptor, MineralDatasetDescriptor, MineralDatasetManifest,
    MineralIngestionChunk, MineralIngestionItem, MineralIngestionPolicy, MineralParserDescriptor,
    MineralReleaseDescriptor, MineralRetrievalDescriptor, MineralSnapshotKind,
    MineralSourceAttribution, MineralSourceDescriptor, MAX_MINERAL_INGESTION_CHUNK_ITEMS,
    MINERAL_INGESTION_SCHEMA_VERSION,
};
use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
struct GenerateOptions {
    count: usize,
    output: PathBuf,
    chunk_size: usize,
    variant: Variant,
    base_batch_id: Option<String>,
    policy: MineralIngestionPolicy,
    inject_conflicts: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variant {
    Baseline,
    Changed,
}

impl Variant {
    fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Changed => "changed",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureIndex {
    format: String,
    manifest_sha256: String,
    manifest_file_sha256: String,
    records_sha256: String,
    chunks: Vec<FixtureChunkIndex>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureChunkIndex {
    chunk_index: usize,
    content_sha256: String,
    file: String,
    file_sha256: String,
    item_count: usize,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("fixture error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("generate") => {
            let options = parse_generate_options(args.collect())?;
            generate(&options)
        }
        Some("check") => {
            let input = parse_check_options(args.collect())?;
            verify(&input)?;
            println!("fixture verified: {}", input.canonicalize()?.display());
            Ok(())
        }
        _ => {
            print_usage();
            bail!("expected the 'generate' or 'check' command")
        }
    }
}

fn print_usage() {
    eprintln!(
        "Usage:\n  \
         cargo run --locked --example generate_ingestion_fixture -- generate \
         --count 6500 --output .tmp/load-6500 [--chunk-size 500] \
         [--variant baseline|changed] [--base-batch-id ID] \
         [--policy create_only_v1|ima_identity_v1] [--inject-conflicts]\n  \
         cargo run --locked --example generate_ingestion_fixture -- check \
         --input .tmp/load-6500"
    );
}

fn parse_generate_options(args: Vec<String>) -> Result<GenerateOptions> {
    let mut count = None;
    let mut output = None;
    let mut chunk_size = MAX_MINERAL_INGESTION_CHUNK_ITEMS;
    let mut variant = Variant::Baseline;
    let mut base_batch_id = None;
    let mut policy = MineralIngestionPolicy::ImaIdentityV1;
    let mut inject_conflicts = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--count" => count = Some(parse_usize_value(&args, &mut index, "--count")?),
            "--output" => output = Some(PathBuf::from(next_value(&args, &mut index, "--output")?)),
            "--chunk-size" => chunk_size = parse_usize_value(&args, &mut index, "--chunk-size")?,
            "--variant" => {
                variant = match next_value(&args, &mut index, "--variant")?.as_str() {
                    "baseline" => Variant::Baseline,
                    "changed" => Variant::Changed,
                    value => bail!("unsupported --variant '{value}'"),
                }
            }
            "--base-batch-id" => {
                base_batch_id = Some(next_value(&args, &mut index, "--base-batch-id")?)
            }
            "--policy" => {
                policy = match next_value(&args, &mut index, "--policy")?.as_str() {
                    "create_only_v1" => MineralIngestionPolicy::CreateOnlyV1,
                    "ima_identity_v1" => MineralIngestionPolicy::ImaIdentityV1,
                    value => bail!("unsupported --policy '{value}'"),
                }
            }
            "--inject-conflicts" => inject_conflicts = true,
            value => bail!("unknown generate option '{value}'"),
        }
        index += 1;
    }
    let count = count.context("generate requires --count")?;
    let output = output.context("generate requires --output")?;
    if !(1..=100_000).contains(&count) {
        bail!("--count must be between 1 and 100000");
    }
    if !(1..=MAX_MINERAL_INGESTION_CHUNK_ITEMS).contains(&chunk_size) {
        bail!(
            "--chunk-size must be between 1 and {}",
            MAX_MINERAL_INGESTION_CHUNK_ITEMS
        );
    }
    match (variant, base_batch_id.as_ref()) {
        (Variant::Baseline, Some(_)) => bail!("baseline must not specify --base-batch-id"),
        (Variant::Changed, None) => {
            bail!("changed variant requires the approved --base-batch-id")
        }
        _ => {}
    }
    if inject_conflicts && policy == MineralIngestionPolicy::CreateOnlyV1 {
        bail!(
            "--inject-conflicts requires --policy ima_identity_v1; create_only_v1 forbids official authority identifiers"
        );
    }
    Ok(GenerateOptions {
        count,
        output,
        chunk_size,
        variant,
        base_batch_id,
        policy,
        inject_conflicts,
    })
}

fn parse_check_options(args: Vec<String>) -> Result<PathBuf> {
    if args.len() == 2 && args[0] == "--input" {
        Ok(PathBuf::from(&args[1]))
    } else {
        bail!("check requires exactly --input DIRECTORY")
    }
}

fn next_value(args: &[String], index: &mut usize, option: &str) -> Result<String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .with_context(|| format!("{option} requires a value"))
}

fn parse_usize_value(args: &[String], index: &mut usize, option: &str) -> Result<usize> {
    next_value(args, index, option)?
        .parse::<usize>()
        .with_context(|| format!("{option} must be an unsigned integer"))
}

fn generate(options: &GenerateOptions) -> Result<()> {
    if options.output.exists() && fs::read_dir(&options.output)?.next().is_some() {
        bail!(
            "output directory is not empty: {}",
            options.output.display()
        );
    }
    let chunks_dir = options.output.join("chunks");
    fs::create_dir_all(&chunks_dir)
        .with_context(|| format!("failed to create {}", chunks_dir.display()))?;

    let items = match options.variant {
        Variant::Baseline => (1..=options.count)
            .map(|record_number| baseline_item(record_number, options.policy))
            .collect(),
        Variant::Changed => changed_items(options.count, options.inject_conflicts, options.policy),
    };
    let chunks = items
        .chunks(options.chunk_size)
        .enumerate()
        .map(|(chunk_index, items)| MineralIngestionChunk {
            schema_version: MINERAL_INGESTION_SCHEMA_VERSION,
            chunk_index,
            items: items.to_vec(),
        })
        .collect::<Vec<_>>();
    let records_sha256 = canonical_mineral_records_hash(&items)?;
    let conflict_suffix = if options.inject_conflicts {
        "-conflict"
    } else {
        ""
    };
    let release_version = format!(
        "load-{}-{}-v2{}",
        options.count,
        options.variant.as_str(),
        conflict_suffix
    );
    let configuration = serde_json::json!({
        "chunk_size": options.chunk_size,
        "count": options.count,
        "inject_conflicts": options.inject_conflicts,
        "policy": match options.policy {
            MineralIngestionPolicy::CreateOnlyV1 => "create_only_v1",
            MineralIngestionPolicy::ImaIdentityV1 => "ima_identity_v1",
        },
        "variant": options.variant.as_str(),
    });
    let manifest = MineralDatasetManifest {
        schema_version: MINERAL_INGESTION_SCHEMA_VERSION,
        dataset: MineralDatasetDescriptor {
            key: "waajacu.load_fixture.minerals".to_string(),
            title: "Waajacu synthetic mineral ingestion load fixture".to_string(),
        },
        source: MineralSourceDescriptor {
            key: "waajacu.load_fixture.generator".to_string(),
            url: "https://example.invalid/waajacu/mineral-load-fixture".to_string(),
            license_spdx: "CC0-1.0".to_string(),
            attribution: Some(MineralSourceAttribution {
                attribution_party: "Waajacu deterministic fixture generator".to_string(),
                work_title: "Waajacu synthetic mineral ingestion load fixture".to_string(),
                work_url: "https://example.invalid/waajacu/mineral-load-fixture".to_string(),
                license_url:
                    "https://creativecommons.org/publicdomain/zero/1.0/".to_string(),
                changes_notice: "Generated deterministically for load and recovery testing; no source mineral records were transformed.".to_string(),
                no_endorsement_notice: "This synthetic fixture is not endorsed by any mineralogical authority.".to_string(),
                derived_output_license_spdx: "CC0-1.0".to_string(),
            }),
        },
        release: MineralReleaseDescriptor {
            version: release_version.clone(),
            released_at: "2026-01-01T00:00:00Z".to_string(),
        },
        retrieval: MineralRetrievalDescriptor {
            retrieved_at: "2026-01-01T00:00:00Z".to_string(),
        },
        artifact: MineralArtifactDescriptor {
            url: format!("https://example.invalid/waajacu/{release_version}.json"),
            sha256: records_sha256.clone(),
        },
        parser: MineralParserDescriptor {
            name: "waajacu_ingestion_fixture_generator".to_string(),
            version: "1.1.0".to_string(),
            code_revision: "examples/generate_ingestion_fixture.rs:v2".to_string(),
            configuration_sha256: sha256_bytes(&canonical_json_bytes(&configuration)?),
        },
        policy: options.policy,
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
        chunk_index.push(FixtureChunkIndex {
            chunk_index: chunk.chunk_index,
            content_sha256: canonical_mineral_chunk_hash(chunk)?,
            file: format!("chunks/{filename}"),
            file_sha256: sha256_bytes(&fs::read(&path)?),
            item_count: chunk.items.len(),
        });
    }
    let index = FixtureIndex {
        format: "waajacu-load-fixture-index-v1".to_string(),
        manifest_sha256: canonical_mineral_manifest_hash(&manifest)?,
        manifest_file_sha256: sha256_bytes(&fs::read(&manifest_path)?),
        records_sha256: records_sha256.clone(),
        chunks: chunk_index,
    };
    write_pretty_json(&options.output.join("fixture-index.json"), &index)?;
    verify(&options.output)?;
    println!(
        "generated {} records in {} chunks at {}\nmanifest {}\nrecords  {}",
        items.len(),
        chunks.len(),
        options.output.canonicalize()?.display(),
        index.manifest_sha256,
        records_sha256
    );
    Ok(())
}

fn baseline_item(record_number: usize, policy: MineralIngestionPolicy) -> MineralIngestionItem {
    let stable_id = format!("LOAD-{record_number:08}");
    let official_identifiers = match policy {
        MineralIngestionPolicy::CreateOnlyV1 => BTreeMap::new(),
        MineralIngestionPolicy::ImaIdentityV1 => {
            BTreeMap::from([("ima_number".to_string(), stable_id.clone())])
        }
    };
    MineralIngestionItem {
        source_record_id: stable_id.clone(),
        source_locator: Some(format!("synthetic-row:{record_number}")),
        slug: format!("mineral.load-fixture-{record_number:08}"),
        canonical_name: format!("Load fixture mineral {record_number:08}"),
        formula: "SiO2".to_string(),
        nomenclature_status: "approved".to_string(),
        is_valid_species: true,
        official_identifiers,
        synonyms: (1..=5)
            .map(|alias| format!("Load alias {alias} for {record_number:08}"))
            .collect(),
        official_facts: Default::default(),
    }
}

fn changed_items(
    count: usize,
    inject_conflicts: bool,
    policy: MineralIngestionPolicy,
) -> Vec<MineralIngestionItem> {
    let omitted = (100..=count).step_by(100).collect::<HashSet<_>>();
    let mut items = (1..=count)
        .filter(|number| !omitted.contains(number))
        .map(|record_number| baseline_item(record_number, policy))
        .collect::<Vec<_>>();
    for item in &mut items {
        let number = item
            .source_record_id
            .strip_prefix("LOAD-")
            .and_then(|value| value.parse::<usize>().ok())
            .expect("fixture IDs are generated locally");
        if number % 101 == 0 {
            item.canonical_name.push_str(" revised");
            item.synonyms.push(format!("Former load name {number:08}"));
        }
        if number % 103 == 0 {
            item.formula = "Al2O3".to_string();
        }
        if number % 107 == 0 {
            item.nomenclature_status = "discredited".to_string();
            item.is_valid_species = false;
        }
    }
    for offset in 1..=omitted.len() {
        items.push(baseline_item(count + offset, policy));
    }
    if inject_conflicts && items.len() >= 2 {
        items
            .last_mut()
            .expect("checked length")
            .official_identifiers = items
            .first()
            .expect("checked length")
            .official_identifiers
            .clone();
    }
    items
}

fn verify(directory: &Path) -> Result<()> {
    let manifest_path = directory.join("manifest.json");
    let index_path = directory.join("fixture-index.json");
    let manifest: MineralDatasetManifest = read_json(&manifest_path)?;
    let index: FixtureIndex = read_json(&index_path)?;
    if manifest.schema_version != MINERAL_INGESTION_SCHEMA_VERSION {
        bail!("unsupported manifest schema version");
    }
    if index.format != "waajacu-load-fixture-index-v1" {
        bail!("unsupported fixture index format");
    }
    if canonical_mineral_manifest_hash(&manifest)? != index.manifest_sha256 {
        bail!("canonical manifest hash mismatch");
    }
    if sha256_bytes(&fs::read(&manifest_path)?) != index.manifest_file_sha256 {
        bail!("manifest file-byte hash mismatch");
    }
    if index.chunks.len() != manifest.expected_chunk_count {
        bail!("manifest chunk count does not match fixture index");
    }

    let is_conflict_fixture = manifest.release.version.ends_with("-conflict");
    let mut chunks = Vec::with_capacity(index.chunks.len());
    let mut all_items = Vec::with_capacity(manifest.expected_record_count);
    let mut source_ids = HashSet::new();
    let mut slugs = HashSet::new();
    let mut authority_ids = HashSet::new();
    for (expected_index, metadata) in index.chunks.iter().enumerate() {
        let relative = Path::new(&metadata.file);
        validate_relative_chunk_path(relative)?;
        let path = directory.join(relative);
        let chunk: MineralIngestionChunk = read_json(&path)?;
        if chunk.schema_version != MINERAL_INGESTION_SCHEMA_VERSION
            || chunk.chunk_index != expected_index
            || metadata.chunk_index != expected_index
        {
            bail!(
                "non-contiguous or invalid chunk index at {}",
                path.display()
            );
        }
        if chunk.items.is_empty() || chunk.items.len() > MAX_MINERAL_INGESTION_CHUNK_ITEMS {
            bail!("invalid chunk size at {}", path.display());
        }
        if chunk.items.len() != metadata.item_count {
            bail!("chunk item count mismatch at {}", path.display());
        }
        if canonical_mineral_chunk_hash(&chunk)? != metadata.content_sha256 {
            bail!("canonical chunk hash mismatch at {}", path.display());
        }
        if sha256_bytes(&fs::read(&path)?) != metadata.file_sha256 {
            bail!("chunk file-byte hash mismatch at {}", path.display());
        }
        for item in &chunk.items {
            if manifest.policy == MineralIngestionPolicy::CreateOnlyV1
                && !item.official_identifiers.is_empty()
            {
                bail!(
                    "create_only_v1 fixture item '{}' contains an official authority identifier",
                    item.source_record_id
                );
            }
            if !source_ids.insert(item.source_record_id.clone()) {
                bail!("duplicate source_record_id '{}'", item.source_record_id);
            }
            if !slugs.insert(item.slug.clone()) {
                bail!("duplicate slug '{}'", item.slug);
            }
            if item.synonyms.len() < 5 {
                bail!(
                    "fixture item '{}' has fewer than five aliases",
                    item.source_record_id
                );
            }
            for (key, value) in &item.official_identifiers {
                if !authority_ids.insert((key.clone(), value.clone())) && !is_conflict_fixture {
                    bail!("duplicate authority identity {key}={value}");
                }
            }
        }
        all_items.extend(chunk.items.iter().cloned());
        chunks.push(chunk);
    }
    if all_items.len() != manifest.expected_record_count {
        bail!("manifest record count does not match chunks");
    }
    let records_hash = canonical_mineral_records_hash(&all_items)?;
    if records_hash != manifest.records_sha256 || records_hash != index.records_sha256 {
        bail!("canonical records hash mismatch");
    }
    if manifest.artifact.sha256 != records_hash {
        bail!("fixture artifact hash must equal its canonical records hash");
    }
    Ok(())
}

fn validate_relative_chunk_path(path: &Path) -> Result<()> {
    if path.is_absolute()
        || !path.starts_with("chunks")
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!(
            "fixture index contains unsafe chunk path {}",
            path.display()
        );
    }
    Ok(())
}

fn canonical_json_bytes(value: &serde_json::Value) -> Result<Vec<u8>> {
    fn sort(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let sorted = map
                    .iter()
                    .map(|(key, value)| (key.clone(), sort(value)))
                    .collect::<BTreeMap<_, _>>();
                serde_json::to_value(sorted).expect("BTreeMap serialization cannot fail")
            }
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.iter().map(sort).collect())
            }
            value => value.clone(),
        }
    }
    serde_json::to_vec(&sort(value)).context("failed to canonicalize fixture configuration")
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

fn write_pretty_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("invalid JSON in {}", path.display()))
}
