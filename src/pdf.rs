use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use askama::Template;
use tokio::{fs, process::Command};

use crate::agent::{ElementShare, MineralReport};
use crate::i18n::{ui_text, Language, UiText};

#[derive(Clone)]
pub struct PdfGenerator {
    reports_root: PathBuf,
    private_work_root: PathBuf,
    images_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GeneratedArtifacts {
    pub pdf_path: String,
    pub html_path: String,
}

impl PdfGenerator {
    pub fn new(data_root: impl Into<PathBuf>) -> Self {
        let data_root = data_root.into();
        let private_work_root = data_root.join(".report-work");
        if let Err(error) = std::fs::remove_dir_all(&private_work_root) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(
                    path = %private_work_root.display(),
                    %error,
                    "failed to remove stale private report workspaces at startup"
                );
            }
        }
        Self {
            reports_root: data_root.join("reports"),
            private_work_root,
            images_root: data_root.join("images"),
        }
    }

    pub async fn generate_pdf(
        &self,
        report: &MineralReport,
        language: Language,
        run_id: &str,
    ) -> Result<GeneratedArtifacts> {
        if !is_safe_path_segment(&report.mineral.folder_name) || !is_safe_path_segment(run_id) {
            return Err(anyhow!("unsafe report output path"));
        }

        let mineral_reports_dir = self.reports_root.join(&report.mineral.folder_name);
        let public_run_dir = mineral_reports_dir.join(run_id);
        if fs::try_exists(&public_run_dir)
            .await
            .with_context(|| format!("failed to inspect {}", public_run_dir.display()))?
        {
            return Err(anyhow!(
                "report output directory already exists: {}",
                public_run_dir.display()
            ));
        }

        let work_parent = self.private_work_root.join(&report.mineral.folder_name);
        fs::create_dir_all(&work_parent).await.with_context(|| {
            format!(
                "failed to create private report work root {}",
                work_parent.display()
            )
        })?;
        let work_dir = work_parent.join(run_id);
        // Create synchronously so there is no cancellation window between the
        // directory appearing and its cleanup guard being armed.
        std::fs::create_dir(&work_dir).with_context(|| {
            format!(
                "failed to create private report work directory {}",
                work_dir.display()
            )
        })?;
        let mut work_guard = WorkDirGuard::new(work_dir.clone());

        let result = self
            .generate_in_private_work_dir(
                report,
                language,
                run_id,
                &work_dir,
                &mineral_reports_dir,
                &public_run_dir,
            )
            .await;

        match remove_work_dir(&work_dir).await {
            Ok(()) => work_guard.disarm(),
            Err(error) => {
                tracing::warn!(
                    path = %work_dir.display(),
                    %error,
                    "failed to remove private report work directory; drop cleanup will retry"
                );
            }
        }

        if result.is_ok() {
            if let Err(error) =
                prune_old_report_runs(&mineral_reports_dir, run_id, MAX_REPORT_RUNS_PER_MINERAL)
                    .await
            {
                tracing::warn!(
                    path = %mineral_reports_dir.display(),
                    %error,
                    "report was published, but old report retention cleanup failed"
                );
            }
        }

        result
    }

    async fn generate_in_private_work_dir(
        &self,
        report: &MineralReport,
        language: Language,
        run_id: &str,
        work_dir: &Path,
        mineral_reports_dir: &Path,
        public_run_dir: &Path,
    ) -> Result<GeneratedArtifacts> {
        let staged_image_file =
            stage_image_for_latex(work_dir, &report.mineral.image_path, &self.images_root).await?;

        let html = ReportHtmlTemplate::from_report(report, language).render()?;
        let tex = ReportTexTemplate::from_report(report, language, staged_image_file).render()?;
        let tex_file = work_dir.join("report.tex");
        fs::write(&tex_file, tex)
            .await
            .with_context(|| format!("failed to write {}", tex_file.display()))?;

        let mut command = Command::new("latexmk");
        command
            .kill_on_drop(true)
            .arg("-xelatex")
            .arg("-interaction=nonstopmode")
            .arg("-halt-on-error")
            .arg("report.tex")
            .current_dir(work_dir);
        let output = command.output().await.with_context(|| {
            "failed to execute 'latexmk'; install latexmk + XeLaTeX + required fonts"
        })?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "latexmk failed in private report workspace\nstdout:\n{}\nstderr:\n{}",
                stdout.trim(),
                stderr.trim()
            ));
        }

        let generated_pdf = work_dir.join("report.pdf");
        let pdf_metadata = fs::metadata(&generated_pdf).await.with_context(|| {
            format!(
                "latexmk completed but {} was not generated",
                generated_pdf.display()
            )
        })?;
        if !pdf_metadata.is_file() {
            return Err(anyhow!(
                "latexmk output is not a regular file: {}",
                generated_pdf.display()
            ));
        }

        // Build the exact public payload privately and publish the complete
        // directory in one rename. TeX sources, logs, staged images, and auxiliary
        // files never enter the served reports tree.
        let publish_dir = work_dir.join("publish");
        fs::create_dir(&publish_dir)
            .await
            .with_context(|| format!("failed to create {}", publish_dir.display()))?;
        fs::rename(&generated_pdf, publish_dir.join("report.pdf"))
            .await
            .with_context(|| "failed to prepare generated PDF for publication")?;
        fs::write(publish_dir.join("report.html"), html)
            .await
            .with_context(|| "failed to prepare generated HTML for publication")?;

        fs::create_dir_all(mineral_reports_dir)
            .await
            .with_context(|| {
                format!(
                    "failed to create report publication directory {}",
                    mineral_reports_dir.display()
                )
            })?;
        if fs::try_exists(public_run_dir)
            .await
            .with_context(|| format!("failed to inspect {}", public_run_dir.display()))?
        {
            return Err(anyhow!(
                "report output directory already exists: {}",
                public_run_dir.display()
            ));
        }
        fs::rename(&publish_dir, public_run_dir)
            .await
            .with_context(|| {
                format!(
                    "failed to publish report directory {}",
                    public_run_dir.display()
                )
            })?;

        Ok(GeneratedArtifacts {
            pdf_path: format!(
                "/artifacts/{}/{run_id}/report.pdf",
                report.mineral.folder_name
            ),
            html_path: format!(
                "/artifacts/{}/{run_id}/report.html",
                report.mineral.folder_name
            ),
        })
    }
}

const MAX_REPORT_RUNS_PER_MINERAL: usize = 25;

#[derive(Debug)]
struct WorkDirGuard {
    path: PathBuf,
    armed: bool,
}

impl WorkDirGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for WorkDirGuard {
    fn drop(&mut self) {
        if self.armed {
            // This synchronous best-effort retry also runs when the async future is
            // cancelled (for example by the report timeout) or unwinds.
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

async fn remove_work_dir(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

#[derive(Debug)]
struct ReportRunDirectory {
    name: String,
    path: PathBuf,
    modified: std::time::SystemTime,
}

async fn prune_old_report_runs(
    mineral_reports_dir: &Path,
    current_run_id: &str,
    retain: usize,
) -> Result<usize> {
    let mut reader = match fs::read_dir(mineral_reports_dir).await {
        Ok(reader) => reader,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to list {}", mineral_reports_dir.display()));
        }
    };
    let mut runs = Vec::new();
    while let Some(entry) = reader
        .next_entry()
        .await
        .with_context(|| format!("failed to read {}", mineral_reports_dir.display()))?
    {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if !is_opaque_run_id(&name) {
            continue;
        }
        let file_type = entry
            .file_type()
            .await
            .with_context(|| format!("failed to inspect {}", entry.path().display()))?;
        if !file_type.is_dir() {
            continue;
        }
        let modified = entry
            .metadata()
            .await
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        runs.push(ReportRunDirectory {
            name,
            path: entry.path(),
            modified,
        });
    }

    // Always rank the just-published run first. The remaining run directories
    // are newest-first, with their opaque ID as a deterministic tie-breaker.
    runs.sort_by(|left, right| {
        let left_current = left.name == current_run_id;
        let right_current = right.name == current_run_id;
        right_current
            .cmp(&left_current)
            .then_with(|| right.modified.cmp(&left.modified))
            .then_with(|| right.name.cmp(&left.name))
    });

    let mut removed = 0;
    for stale in runs.into_iter().skip(retain.max(1)) {
        match fs::remove_dir_all(&stale.path).await {
            Ok(()) => removed += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(
                    path = %stale.path.display(),
                    %error,
                    "failed to remove stale report run"
                );
            }
        }
    }
    Ok(removed)
}

fn is_opaque_run_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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
    fn from_report(
        report: &MineralReport,
        language: Language,
        staged_image_file: Option<String>,
    ) -> Self {
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
            image_file: staged_image_file,
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

async fn stage_image_for_latex(
    run_dir: &Path,
    image_path: &Option<String>,
    images_root: &Path,
) -> Result<Option<String>> {
    let Some(image_path) = image_path.as_ref() else {
        return Ok(None);
    };

    let Some(file_name) = image_path
        .rsplit('/')
        .next()
        .filter(|value| is_safe_path_segment(value))
    else {
        return Ok(None);
    };
    let source_path = images_root.join(file_name);
    if !source_path.exists() {
        return Ok(None);
    }

    let file_name = file_name.to_string();

    let target_path = run_dir.join(&file_name);
    if source_path != target_path {
        fs::copy(&source_path, &target_path)
            .await
            .with_context(|| {
                format!(
                    "failed to stage image for latex {} -> {}",
                    source_path.display(),
                    target_path.display()
                )
            })?;
    }

    Ok(Some(file_name))
}

fn is_safe_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
        && value != "."
        && value != ".."
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
    use super::{
        is_opaque_run_id, is_safe_path_segment, latex_escape, prune_old_report_runs,
        remove_work_dir, PdfGenerator, WorkDirGuard,
    };

    #[test]
    fn escapes_special_characters() {
        let raw = r"50% Fe_2O_3 & quartz";
        let escaped = latex_escape(raw);
        assert_eq!(escaped, r"50\% Fe\_2O\_3 \& quartz");
    }

    #[test]
    fn validates_path_segments_and_opaque_run_ids() {
        assert!(is_safe_path_segment("mineral.silicates"));
        assert!(!is_safe_path_segment("../silicates"));
        assert!(!is_safe_path_segment(".."));
        assert!(is_opaque_run_id("0123456789abcdef0123456789abcdef"));
        assert!(!is_opaque_run_id("0123456789abcdef"));
        assert!(!is_opaque_run_id("0123456789abcdef0123456789abcdeg"));
    }

    #[test]
    fn private_workspace_is_outside_public_reports_tree() {
        let temp = tempfile::tempdir().expect("temporary data root");
        std::fs::create_dir_all(temp.path().join(".report-work/stale"))
            .expect("create stale workspace");
        std::fs::write(temp.path().join(".report-work/stale/report.log"), b"stale")
            .expect("write stale workspace file");
        let pdf = PdfGenerator::new(temp.path());
        assert_eq!(pdf.reports_root, temp.path().join("reports"));
        assert_eq!(pdf.private_work_root, temp.path().join(".report-work"));
        assert!(!pdf.private_work_root.starts_with(&pdf.reports_root));
        assert!(!pdf.private_work_root.exists());
    }

    #[test]
    fn drop_guard_removes_workspace_after_cancellation_or_unwind() {
        let temp = tempfile::tempdir().expect("temporary data root");
        let work_dir = temp.path().join("work");
        std::fs::create_dir(&work_dir).expect("create work directory");
        std::fs::write(work_dir.join("report.aux"), b"temporary").expect("write auxiliary file");

        {
            let _guard = WorkDirGuard::new(work_dir.clone());
        }

        assert!(!work_dir.exists());
    }

    #[tokio::test]
    async fn async_cleanup_removes_private_workspace() {
        let temp = tempfile::tempdir().expect("temporary data root");
        let work_dir = temp.path().join("work");
        tokio::fs::create_dir(&work_dir)
            .await
            .expect("create work directory");
        tokio::fs::write(work_dir.join("report.log"), b"temporary")
            .await
            .expect("write log");

        remove_work_dir(&work_dir).await.expect("cleanup succeeds");
        assert!(!work_dir.exists());
        remove_work_dir(&work_dir)
            .await
            .expect("cleanup is idempotent");
    }

    #[tokio::test]
    async fn retention_keeps_current_and_only_prunes_opaque_run_directories() {
        let temp = tempfile::tempdir().expect("temporary reports root");
        let mineral_dir = temp.path().join("phenakite");
        tokio::fs::create_dir(&mineral_dir)
            .await
            .expect("create mineral reports directory");
        for index in 0_u32..27 {
            let run_dir = mineral_dir.join(format!("{index:032x}"));
            tokio::fs::create_dir(&run_dir)
                .await
                .expect("create run directory");
            tokio::fs::write(run_dir.join("report.pdf"), b"pdf")
                .await
                .expect("write report");
        }
        let unrelated_dir = mineral_dir.join("notes");
        tokio::fs::create_dir(&unrelated_dir)
            .await
            .expect("create unrelated directory");
        tokio::fs::write(unrelated_dir.join("keep.txt"), b"keep")
            .await
            .expect("write unrelated file");

        let current_run_id = format!("{:032x}", 26_u32);
        let removed = prune_old_report_runs(&mineral_dir, &current_run_id, 25)
            .await
            .expect("retention succeeds");

        assert_eq!(removed, 2);
        assert!(mineral_dir.join(current_run_id).exists());
        assert!(unrelated_dir.join("keep.txt").exists());
        let remaining_runs = std::fs::read_dir(&mineral_dir)
            .expect("list report directories")
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.file_type().is_ok_and(|kind| kind.is_dir())
                    && entry.file_name().to_str().is_some_and(is_opaque_run_id)
            })
            .count();
        assert_eq!(remaining_runs, 25);
    }
}
