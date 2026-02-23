use std::cmp::Ordering;

use chrono::Utc;

use crate::models::{Mineral, ReportRequest};

#[derive(Debug, Clone)]
pub struct ElementShare {
    pub name: String,
    pub percent: f32,
}

#[derive(Debug, Clone)]
pub struct MineralReport {
    pub mineral: Mineral,
    pub audience: String,
    pub purpose: String,
    pub site_context: String,
    pub generated_utc: String,
    pub dominant_element: String,
    pub dominant_element_pct: f32,
    pub hardness_band: String,
    pub density_band: String,
    pub summary: String,
    pub recommendations: Vec<String>,
    pub element_breakdown: Vec<ElementShare>,
}

#[derive(Debug, Clone)]
struct DerivedMetrics {
    dominant_element: String,
    dominant_element_pct: f32,
    hardness_band: String,
    density_band: String,
    element_breakdown: Vec<ElementShare>,
}

pub fn run_agentic_chain(mineral: &Mineral, request: &ReportRequest) -> MineralReport {
    let metrics = derive_metrics(mineral);
    let summary = compose_summary(mineral, request, &metrics);
    let recommendations = propose_recommendations(mineral, request, &metrics);

    MineralReport {
        mineral: mineral.clone(),
        audience: request.audience.clone(),
        purpose: request.purpose.clone(),
        site_context: request.site_context.clone(),
        generated_utc: Utc::now().to_rfc3339(),
        dominant_element: metrics.dominant_element,
        dominant_element_pct: metrics.dominant_element_pct,
        hardness_band: metrics.hardness_band,
        density_band: metrics.density_band,
        summary,
        recommendations,
        element_breakdown: metrics.element_breakdown,
    }
}

fn derive_metrics(mineral: &Mineral) -> DerivedMetrics {
    let mut element_breakdown: Vec<ElementShare> = mineral
        .major_elements_pct
        .iter()
        .map(|(name, percent)| ElementShare {
            name: name.clone(),
            percent: *percent,
        })
        .collect();

    element_breakdown.sort_by(|a, b| b.percent.partial_cmp(&a.percent).unwrap_or(Ordering::Equal));

    let dominant = element_breakdown.first().cloned().unwrap_or(ElementShare {
        name: "Unknown".to_string(),
        percent: 0.0,
    });

    let hardness_band = match mineral.hardness_mohs {
        h if h < 3.0 => "soft".to_string(),
        h if h < 6.0 => "medium".to_string(),
        h if h < 7.5 => "hard".to_string(),
        _ => "very hard".to_string(),
    };

    let density_band = match mineral.density_g_cm3 {
        d if d < 2.6 => "light".to_string(),
        d if d < 3.2 => "moderate".to_string(),
        _ => "dense".to_string(),
    };

    DerivedMetrics {
        dominant_element: dominant.name,
        dominant_element_pct: dominant.percent,
        hardness_band,
        density_band,
        element_breakdown,
    }
}

fn compose_summary(mineral: &Mineral, request: &ReportRequest, metrics: &DerivedMetrics) -> String {
    format!(
        "For {audience} and the {site} context, {mineral} is classified as {hardness} with {density} density behavior. The chemistry is led by {element} ({pct:.1} wt%), supporting {purpose} decisions.",
        audience = request.audience,
        site = request.site_context,
        mineral = mineral.common_name,
        hardness = metrics.hardness_band,
        density = metrics.density_band,
        element = metrics.dominant_element,
        pct = metrics.dominant_element_pct,
        purpose = request.purpose,
    )
}

fn propose_recommendations(
    mineral: &Mineral,
    request: &ReportRequest,
    metrics: &DerivedMetrics,
) -> Vec<String> {
    let mut recs = Vec::new();

    recs.push(format!(
        "Prioritize samples of {} where {} enrichment is strongest.",
        mineral.common_name, metrics.dominant_element
    ));

    if metrics.hardness_band == "very hard" || metrics.hardness_band == "hard" {
        recs.push(
            "Use abrasion-resistant tooling and adjust comminution energy estimates upward."
                .to_string(),
        );
    } else {
        recs.push(
            "Validate breakage and weathering rates early, as softer material can bias grade control."
                .to_string(),
        );
    }

    if metrics.density_band == "dense" {
        recs.push(
            "Run density separation testwork to confirm recovery uplift potential in early flowsheets."
                .to_string(),
        );
    } else {
        recs.push(
            "Combine XRD with geochemistry to avoid over-reliance on density-based separation."
                .to_string(),
        );
    }

    recs.push(format!(
        "Archive this report against '{}' objectives for reproducible decision records.",
        request.purpose
    ));

    recs
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::run_agentic_chain;
    use crate::models::{Mineral, ReportRequest};

    #[test]
    fn chain_sorts_elements_and_sets_dominant() {
        let mut elements = BTreeMap::new();
        elements.insert("Si".to_string(), 46.7);
        elements.insert("O".to_string(), 53.3);

        let mineral = Mineral {
            slug: "mineral.silicate.0xaaaaaa".to_string(),
            folder_name: "mineral.silicate.0xaaaaaa".to_string(),
            common_name: "Test Mineral".to_string(),
            description: "Test description".to_string(),
            mineral_family: "silicate".to_string(),
            formula: "SiO2".to_string(),
            hardness_mohs: 7.0,
            density_g_cm3: 2.65,
            crystal_system: "trigonal".to_string(),
            color: "colorless".to_string(),
            streak: "white".to_string(),
            luster: "vitreous".to_string(),
            major_elements_pct: elements,
            notes: "n/a".to_string(),
            image_path: None,
        };

        let report = run_agentic_chain(&mineral, &ReportRequest::default());

        assert_eq!(report.dominant_element, "O");
        assert_eq!(report.element_breakdown[0].name, "O");
        assert_eq!(report.hardness_band, "hard");
    }
}
