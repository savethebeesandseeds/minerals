use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use askama::Template;
use tokio::{fs, process::Command};

use crate::agent::{ElementShare, MineralReport};
use crate::i18n::{ui_text, Language, UiText};

#[derive(Clone)]
pub struct PdfGenerator {
    minerals_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GeneratedArtifacts {
    pub pdf_path: String,
    pub html_path: String,
}

impl PdfGenerator {
    pub fn new(minerals_root: impl Into<PathBuf>) -> Self {
        Self {
            minerals_root: minerals_root.into(),
        }
    }

    pub async fn generate_pdf(
        &self,
        report: &MineralReport,
        language: Language,
    ) -> Result<GeneratedArtifacts> {
        let run_dir = self.minerals_root.join(&report.mineral.folder_name);
        fs::create_dir_all(&run_dir)
            .await
            .with_context(|| format!("failed to create output directory {}", run_dir.display()))?;

        let html = ReportHtmlTemplate::from_report(report, language).render()?;
        let html_file = run_dir.join("report.html");
        fs::write(&html_file, html)
            .await
            .with_context(|| format!("failed to write {}", html_file.display()))?;

        let tex = ReportTexTemplate::from_report(report, language).render()?;
        let tex_file = run_dir.join("report.tex");
        fs::write(&tex_file, tex)
            .await
            .with_context(|| format!("failed to write {}", tex_file.display()))?;

        let output = Command::new("latexmk")
            .arg("-xelatex")
            .arg("-interaction=nonstopmode")
            .arg("-halt-on-error")
            .arg("report.tex")
            .current_dir(&run_dir)
            .output()
            .await
            .with_context(|| {
                "failed to execute 'latexmk'; install latexmk + XeLaTeX + required fonts"
            })?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "latexmk failed in {}\nstdout:\n{}\nstderr:\n{}",
                run_dir.display(),
                stdout.trim(),
                stderr.trim()
            ));
        }

        let pdf_file = run_dir.join("report.pdf");
        if !pdf_file.exists() {
            return Err(anyhow!(
                "latexmk completed but {} was not generated",
                pdf_file.display()
            ));
        }

        Ok(GeneratedArtifacts {
            pdf_path: format!("/data/minerals/{}/report.pdf", report.mineral.folder_name),
            html_path: format!("/data/minerals/{}/report.html", report.mineral.folder_name),
        })
    }
}

#[derive(Debug, Clone)]
struct LatexElementShare {
    name: String,
    percent: String,
}

#[derive(Debug, Clone)]
struct HtmlElementShare {
    name: String,
    percent: String,
}

#[derive(Template)]
#[template(path = "report.tex", escape = "none")]
struct ReportTexTemplate {
    lang_code: String,
    txt: UiText,
    generated_utc: String,
    mineral_name: String,
    mineral_family: String,
    description: String,
    formula: String,
    hardness_mohs: String,
    hardness_band: String,
    density_g_cm3: String,
    density_band: String,
    crystal_system: String,
    color: String,
    streak: String,
    luster: String,
    dominant_element: String,
    dominant_element_pct: String,
    audience: String,
    purpose: String,
    site_context: String,
    summary: String,
    notes: String,
    image_file: Option<String>,
    recommendations: Vec<String>,
    element_breakdown: Vec<LatexElementShare>,
}

#[derive(Template)]
#[template(path = "report.html")]
struct ReportHtmlTemplate {
    lang_code: String,
    lang_dir: String,
    txt: UiText,
    generated_utc: String,
    mineral_name: String,
    mineral_family: String,
    description: String,
    formula: String,
    hardness_mohs: String,
    hardness_band: String,
    density_g_cm3: String,
    density_band: String,
    crystal_system: String,
    color: String,
    streak: String,
    luster: String,
    dominant_element: String,
    dominant_element_pct: String,
    audience: String,
    purpose: String,
    site_context: String,
    summary: String,
    notes: String,
    image_path: Option<String>,
    recommendations: Vec<String>,
    element_breakdown: Vec<HtmlElementShare>,
}

impl ReportTexTemplate {
    fn from_report(report: &MineralReport, language: Language) -> Self {
        let txt = ui_text(language);
        Self {
            lang_code: language.code().to_string(),
            txt,
            generated_utc: latex_escape(&report.generated_utc),
            mineral_name: latex_escape(&report.mineral.common_name),
            mineral_family: latex_escape(&report.mineral.mineral_family),
            description: latex_escape(&report.mineral.description),
            formula: latex_escape(&report.mineral.formula),
            hardness_mohs: format!("{:.2}", report.mineral.hardness_mohs),
            hardness_band: latex_escape(&report.hardness_band),
            density_g_cm3: format!("{:.2}", report.mineral.density_g_cm3),
            density_band: latex_escape(&report.density_band),
            crystal_system: latex_escape(&report.mineral.crystal_system),
            color: latex_escape(&report.mineral.color),
            streak: latex_escape(&report.mineral.streak),
            luster: latex_escape(&report.mineral.luster),
            dominant_element: latex_escape(&report.dominant_element),
            dominant_element_pct: format!("{:.1}", report.dominant_element_pct),
            audience: latex_escape(&report.audience),
            purpose: latex_escape(&report.purpose),
            site_context: latex_escape(&report.site_context),
            summary: latex_escape(&report.summary),
            notes: latex_escape(&report.mineral.notes),
            image_file: image_file_name(&report.mineral.image_path),
            recommendations: report
                .recommendations
                .iter()
                .map(|rec| latex_escape(rec))
                .collect(),
            element_breakdown: report
                .element_breakdown
                .iter()
                .map(to_latex_share)
                .collect(),
        }
    }
}

impl ReportHtmlTemplate {
    fn from_report(report: &MineralReport, language: Language) -> Self {
        let txt = ui_text(language);
        Self {
            lang_code: language.code().to_string(),
            lang_dir: language.dir().to_string(),
            txt,
            generated_utc: report.generated_utc.clone(),
            mineral_name: report.mineral.common_name.clone(),
            mineral_family: report.mineral.mineral_family.clone(),
            description: report.mineral.description.clone(),
            formula: report.mineral.formula.clone(),
            hardness_mohs: format!("{:.2}", report.mineral.hardness_mohs),
            hardness_band: report.hardness_band.clone(),
            density_g_cm3: format!("{:.2}", report.mineral.density_g_cm3),
            density_band: report.density_band.clone(),
            crystal_system: report.mineral.crystal_system.clone(),
            color: report.mineral.color.clone(),
            streak: report.mineral.streak.clone(),
            luster: report.mineral.luster.clone(),
            dominant_element: report.dominant_element.clone(),
            dominant_element_pct: format!("{:.1}", report.dominant_element_pct),
            audience: report.audience.clone(),
            purpose: report.purpose.clone(),
            site_context: report.site_context.clone(),
            summary: report.summary.clone(),
            notes: report.mineral.notes.clone(),
            image_path: report.mineral.image_path.clone(),
            recommendations: report.recommendations.clone(),
            element_breakdown: report.element_breakdown.iter().map(to_html_share).collect(),
        }
    }
}

fn image_file_name(path: &Option<String>) -> Option<String> {
    path.as_ref()
        .and_then(|value| value.rsplit('/').next())
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn to_latex_share(elem: &ElementShare) -> LatexElementShare {
    LatexElementShare {
        name: latex_escape(&elem.name),
        percent: format!("{:.2}", elem.percent),
    }
}

fn to_html_share(elem: &ElementShare) -> HtmlElementShare {
    HtmlElementShare {
        name: elem.name.clone(),
        percent: format!("{:.2}", elem.percent),
    }
}

fn latex_escape(input: &str) -> String {
    input
        .replace('\\', "\\textbackslash{}")
        .replace('&', "\\&")
        .replace('%', "\\%")
        .replace('$', "\\$")
        .replace('#', "\\#")
        .replace('_', "\\_")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('~', "\\textasciitilde{}")
        .replace('^', "\\textasciicircum{}")
}

#[cfg(test)]
mod tests {
    use super::latex_escape;

    #[test]
    fn escapes_special_characters() {
        let raw = r"50% Fe_2O_3 & quartz";
        let escaped = latex_escape(raw);
        assert_eq!(escaped, r"50\% Fe\_2O\_3 \& quartz");
    }
}
