use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mineral {
    pub slug: String,
    pub folder_name: String,
    pub common_name: String,
    pub description: String,
    pub mineral_family: String,
    pub formula: String,
    pub hardness_mohs: f32,
    pub density_g_cm3: f32,
    pub crystal_system: String,
    pub color: String,
    pub streak: String,
    pub luster: String,
    pub major_elements_pct: BTreeMap<String, f32>,
    pub notes: String,
    pub image_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ReportRequest {
    pub audience: String,
    pub purpose: String,
    pub site_context: String,
}

impl Default for ReportRequest {
    fn default() -> Self {
        Self {
            audience: "technical geologist".to_string(),
            purpose: "exploration briefing".to_string(),
            site_context: "pilot drill campaign".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MineralFormData {
    pub draft_id: Option<String>,
    pub common_name: String,
    pub description: String,
    pub suggestion_context: String,
    pub preview_image_data_url: String,
    pub mineral_family: String,
    pub formula: String,
    pub hardness_mohs: String,
    pub density_g_cm3: String,
    pub crystal_system: String,
    pub color: String,
    pub streak: String,
    pub luster: String,
    pub major_elements_pct_text: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MineralDiskRecord {
    pub common_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(alias = "mineral_group")]
    pub mineral_family: String,
    pub formula: String,
    pub hardness_mohs: f32,
    pub density_g_cm3: f32,
    pub crystal_system: String,
    pub color: String,
    pub streak: String,
    pub luster: String,
    #[serde(default)]
    pub major_elements_pct: BTreeMap<String, f32>,
    pub notes: String,
    #[serde(default)]
    pub image_file: Option<String>,
}

pub fn load_minerals(data_root: &Path, lang_code: &str) -> Result<Vec<Mineral>> {
    let minerals_root = data_root.join("minerals");
    if !minerals_root.exists() {
        fs::create_dir_all(&minerals_root)
            .with_context(|| format!("failed to create {}", minerals_root.display()))?;
    }

    let mut minerals = Vec::new();
    for entry in fs::read_dir(&minerals_root)
        .with_context(|| format!("failed to read {}", minerals_root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let folder_name = entry.file_name().to_string_lossy().to_string();
        if !is_valid_mineral_folder_name(&folder_name) {
            continue;
        }

        let metadata_path = select_metadata_path(&path, lang_code);
        let Some(metadata_path) = metadata_path else {
            continue;
        };

        let raw = fs::read_to_string(&metadata_path)
            .with_context(|| format!("failed to read {}", metadata_path.display()))?;
        let record: MineralDiskRecord = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse {}", metadata_path.display()))?;

        minerals.push(Mineral {
            slug: folder_name.clone(),
            folder_name: folder_name.clone(),
            common_name: record.common_name,
            description: record.description,
            mineral_family: record.mineral_family,
            formula: record.formula,
            hardness_mohs: record.hardness_mohs,
            density_g_cm3: record.density_g_cm3,
            crystal_system: record.crystal_system,
            color: record.color,
            streak: record.streak,
            luster: record.luster,
            major_elements_pct: record.major_elements_pct,
            notes: record.notes,
            image_path: record
                .image_file
                .map(|file| format!("/data/minerals/{}/{}", folder_name, file)),
        });
    }

    minerals.sort_by(|a, b| a.common_name.cmp(&b.common_name));
    Ok(minerals)
}

fn select_metadata_path(folder: &Path, lang_code: &str) -> Option<std::path::PathBuf> {
    let preferred = folder.join(format!("mineral.{lang_code}.json"));
    if preferred.exists() {
        return Some(preferred);
    }

    if lang_code != "en" {
        let english = folder.join("mineral.en.json");
        if english.exists() {
            return Some(english);
        }
    }

    let legacy = folder.join("mineral.json");
    if legacy.exists() {
        return Some(legacy);
    }

    None
}

pub fn is_valid_mineral_folder_name(name: &str) -> bool {
    let mut parts = name.split('.');
    let prefix = parts.next();
    let family = parts.next();
    let id = parts.next();

    if prefix != Some("mineral") || family.is_none() || id.is_none() || parts.next().is_some() {
        return false;
    }

    let family = family.unwrap_or_default();
    let id = id.unwrap_or_default();
    if family.is_empty() || !id.starts_with("0x") || id.len() < 5 {
        return false;
    }

    id[2..].chars().all(|c| c.is_ascii_hexdigit())
}

pub fn parse_major_elements(raw: &str) -> Result<BTreeMap<String, f32>, String> {
    let mut values = BTreeMap::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let separator = if line.contains('=') { '=' } else { ':' };
        let mut parts = line.splitn(2, separator);
        let key = parts.next().unwrap_or_default().trim();
        let value = parts.next().unwrap_or_default().trim();

        if key.is_empty() || value.is_empty() {
            return Err("major_elements_pct lines must be like 'Si=46.7'".to_string());
        }

        let parsed = value
            .parse::<f32>()
            .map_err(|_| format!("invalid percentage for '{key}'"))?;
        values.insert(key.to_string(), parsed);
    }
    Ok(values)
}

pub fn major_elements_to_text(values: &BTreeMap<String, f32>) -> String {
    values
        .iter()
        .map(|(name, value)| format!("{name}={value:.2}"))
        .collect::<Vec<_>>()
        .join("\n")
}
