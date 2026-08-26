use std::{
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::{Component, Path, PathBuf},
    process::ExitCode,
};

use anyhow::{bail, Context, Result};
use getrandom::getrandom;
use minerals_public_catalog::{
    export_public_catalog, validate_public_catalog_release, PublicCatalogManifest,
    PUBLIC_CATALOG_MANIFEST_FILE,
};

const PUBLIC_APP_FILES: &[&str] = &[
    "index.html",
    "app.css",
    "app.js",
    "app-core.mjs",
    "webmcp.mjs",
    "catalog-worker.js",
    "THIRD_PARTY_NOTICES.md",
    "assets/atlas-chemical-family-v2.png",
    "assets/atlas-crystal-system-v2.png",
    "assets/atlas-method-v2.png",
    "assets/atlas-mountain-v2.png",
    "assets/atlas-place-origin-v2.png",
    "assets/atlas-quartz-v2.png",
    "assets/atlas-source-v2.png",
    "assets/waajacu-minerals-social.png",
    "vendor/sqlite/index.mjs",
    "vendor/sqlite/sqlite3.wasm",
    "vendor/sqlite/LICENSE.txt",
    "map/map-loader.js",
    "map/map.css",
    "map/minerals_map.wasm",
];

fn main() -> ExitCode {
    match run(env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("export-public failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<OsString>) -> Result<()> {
    let Some(options) = Options::parse(arguments)? else {
        print_help();
        return Ok(());
    };
    if options.mode == Mode::ListAppFiles {
        for relative in PUBLIC_APP_FILES {
            println!("{relative}");
        }
        return Ok(());
    }

    let app_root = require_real_directory(&options.app_root, "public app root")?;
    let manifest = match options.mode {
        Mode::Export { data_root, output } => {
            let output = resolve_fresh_output(&output)?;
            let data_root = validate_private_output_separation(&data_root, &output)?;
            if output.starts_with(&app_root) {
                bail!(
                    "--output cannot be inside --app-root: {} and {}",
                    output.display(),
                    app_root.display()
                );
            }
            stage_and_promote(&output, |staging| {
                copy_public_app(&app_root, staging)?;
                let expected = export_public_catalog(&data_root, staging)?;
                let published = validate_existing_release(staging, &app_root)?;
                if published != expected {
                    bail!("completed public catalog manifest differs from the exported release");
                }
                Ok(published)
            })?
        }
        Mode::Assemble { source, output } => assemble_catalog_release(&source, &app_root, &output)?,
        Mode::Validate { release } => {
            let release = require_real_directory(&release, "public release")?;
            validate_release_app_separation(&release, &app_root)?;
            validate_existing_release(&release, &app_root)?
        }
        Mode::ListAppFiles => unreachable!("list mode returns before app-root validation"),
    };
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}

fn validate_release_app_separation(release: &Path, app_root: &Path) -> Result<()> {
    if release.starts_with(app_root) || app_root.starts_with(release) {
        bail!(
            "--validate-release and --app-root must be separate, non-nested directories: {} and {}",
            release.display(),
            app_root.display()
        );
    }
    Ok(())
}

fn assemble_catalog_release(
    source: &Path,
    app_root: &Path,
    output: &Path,
) -> Result<PublicCatalogManifest> {
    let source = require_real_directory(source, "catalog-only source")?;
    let app_root = require_real_directory(app_root, "public app root")?;
    let output = resolve_fresh_output(output)?;
    validate_assembly_separation(&source, &app_root, &output)?;
    validate_catalog_source_hygiene(&source)?;
    let expected = validate_public_catalog_release(&source)?;

    stage_and_promote(&output, |staging| {
        copy_public_app(&app_root, staging)?;
        let copied_files = copy_catalog_snapshot(&source, staging, &expected)?;
        let published = validate_existing_release(staging, &app_root)?;
        if published != expected {
            bail!("assembled public catalog manifest differs from the validated source");
        }
        for relative in copied_files {
            if !files_are_identical(&source.join(&relative), &staging.join(&relative))? {
                bail!(
                    "catalog-only source changed while it was assembled: {}",
                    relative.display()
                );
            }
        }
        Ok(published)
    })
}

fn validate_assembly_separation(source: &Path, app_root: &Path, output: &Path) -> Result<()> {
    for (left_label, left, right_label, right) in [
        ("--assemble-catalog", source, "--app-root", app_root),
        ("--assemble-catalog", source, "--output", output),
        ("--app-root", app_root, "--output", output),
    ] {
        if left.starts_with(right) || right.starts_with(left) {
            bail!(
                "{left_label} and {right_label} must be separate, non-nested directories: {} and {}",
                left.display(),
                right.display()
            );
        }
    }
    Ok(())
}

fn validate_catalog_source_hygiene(source: &Path) -> Result<()> {
    let mut found_manifest = false;
    let mut found_data = false;
    for entry in fs::read_dir(source)
        .with_context(|| format!("failed to inspect catalog-only source {}", source.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).with_context(|| {
            format!(
                "failed to inspect catalog-only source entry {}",
                path.display()
            )
        })?;
        if metadata.file_type().is_symlink() {
            bail!(
                "catalog-only source cannot contain symlinks: {}",
                path.display()
            );
        }
        if entry.file_name() == PUBLIC_CATALOG_MANIFEST_FILE {
            if !metadata.is_file() {
                bail!(
                    "catalog-only source manifest must be a regular non-symlink file: {}",
                    path.display()
                );
            }
            found_manifest = true;
        } else if entry.file_name() == "data" {
            if !metadata.is_dir() {
                bail!(
                    "catalog-only source data must be a real directory: {}",
                    path.display()
                );
            }
            found_data = true;
        } else {
            bail!(
                "catalog-only source contains unexpected entry: {}",
                path.display()
            );
        }
    }
    if !found_manifest {
        bail!(
            "catalog-only source is missing {}",
            PUBLIC_CATALOG_MANIFEST_FILE
        );
    }
    if !found_data {
        bail!("catalog-only source is missing data directory");
    }
    Ok(())
}

fn copy_catalog_snapshot(
    source: &Path,
    output: &Path,
    manifest: &PublicCatalogManifest,
) -> Result<Vec<PathBuf>> {
    let files = catalog_snapshot_files(manifest)?;

    // Validate every explicit source path before writing anything. This is an
    // intentionally fixed copy, never a recursive directory traversal.
    let sources = files
        .iter()
        .map(|relative| {
            require_relative_file_path(source, relative, "catalog-only source file")
                .map(|path| (relative.clone(), path))
        })
        .collect::<Result<Vec<_>>>()?;

    for (relative, source_file) in &sources {
        let destination = output.join(relative);
        let parent = destination
            .parent()
            .context("catalog destination has no parent")?;
        prepare_real_directory(parent, "catalog destination directory")?;
        copy_file_atomically(source_file, &destination)?;
    }
    Ok(sources.into_iter().map(|(relative, _)| relative).collect())
}

fn catalog_snapshot_files(manifest: &PublicCatalogManifest) -> Result<Vec<PathBuf>> {
    let database = safe_catalog_relative_path(&manifest.database.path)?;
    Ok(vec![
        PathBuf::from(PUBLIC_CATALOG_MANIFEST_FILE),
        database.clone(),
        PathBuf::from(format!("{}.br", database.to_string_lossy())),
        PathBuf::from(format!("{}.gz", database.to_string_lossy())),
    ])
}

fn safe_catalog_relative_path(value: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || path.parent() != Some(Path::new("data"))
    {
        bail!("invalid catalog database path '{value}'");
    }
    Ok(path)
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    app_root: PathBuf,
    mode: Mode,
}

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Export { data_root: PathBuf, output: PathBuf },
    Assemble { source: PathBuf, output: PathBuf },
    Validate { release: PathBuf },
    ListAppFiles,
}

impl Options {
    fn parse(arguments: Vec<OsString>) -> Result<Option<Self>> {
        if arguments
            .iter()
            .any(|argument| argument == "--help" || argument == "-h")
        {
            return Ok(None);
        }
        let mut data_root = None;
        let mut output = None;
        let mut app_root = None;
        let mut validate_release = None;
        let mut assemble_catalog = None;
        let mut list_app_files = false;
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            let name = argument
                .to_str()
                .context("option names must be valid Unicode")?;
            if name == "--list-app-files" {
                if list_app_files {
                    bail!("duplicate option '--list-app-files'");
                }
                list_app_files = true;
                continue;
            }
            let slot = match name {
                "--data-root" => &mut data_root,
                "--output" => &mut output,
                "--app-root" => &mut app_root,
                "--validate-release" => &mut validate_release,
                "--assemble-catalog" => &mut assemble_catalog,
                _ if name.starts_with('-') => bail!("unknown option '{name}'"),
                _ => bail!("unexpected positional argument '{name}'"),
            };
            if slot.is_some() {
                bail!("duplicate option '{name}'");
            }
            let value = arguments
                .next()
                .with_context(|| format!("missing value for '{name}'"))?;
            if value.is_empty() {
                bail!("'{name}' cannot be empty");
            }
            *slot = Some(PathBuf::from(value));
        }
        let mode = if list_app_files {
            if data_root.is_some()
                || output.is_some()
                || app_root.is_some()
                || validate_release.is_some()
                || assemble_catalog.is_some()
            {
                bail!("--list-app-files cannot be combined with any other option");
            }
            Mode::ListAppFiles
        } else if let Some(release) = validate_release {
            if data_root.is_some() || output.is_some() || assemble_catalog.is_some() {
                bail!(
                    "--validate-release cannot be combined with --data-root, --output, or --assemble-catalog"
                );
            }
            Mode::Validate { release }
        } else if let Some(source) = assemble_catalog {
            if data_root.is_some() {
                bail!("--assemble-catalog cannot be combined with --data-root");
            }
            Mode::Assemble {
                source,
                output: output.context("missing required --output PATH")?,
            }
        } else {
            Mode::Export {
                data_root: data_root.context("missing required --data-root PATH")?,
                output: output.context("missing required --output PATH")?,
            }
        };
        Ok(Some(Self {
            app_root: app_root.unwrap_or_else(|| PathBuf::from("public-app")),
            mode,
        }))
    }
}

fn copy_public_app(app_root: &Path, output: &Path) -> Result<()> {
    let app_root = require_real_directory(app_root, "public app root")?;
    let output = prepare_real_directory(output, "public export output")?;
    if app_root == output {
        bail!("--app-root and --output must be different directories");
    }
    validate_output_hygiene(&output)?;

    // Validate the complete source allowlist before changing the destination.
    let assets = PUBLIC_APP_FILES
        .iter()
        .map(|relative| {
            let relative = safe_relative_path(relative)?;
            let source = require_asset_path(&app_root, &relative)?;
            Ok((relative, source))
        })
        .collect::<Result<Vec<_>>>()?;

    for (relative, source) in assets {
        let destination = output.join(&relative);
        let parent = destination
            .parent()
            .context("public app destination has no parent")?;
        prepare_real_directory(parent, "public app destination directory")?;
        match fs::symlink_metadata(&destination) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => bail!(
                "public app destination must be a regular non-symlink file: {}",
                destination.display()
            ),
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect public app destination {}",
                        destination.display()
                    )
                })
            }
        }
        copy_file_atomically(&source, &destination)?;
    }
    Ok(())
}

fn validate_private_output_separation(data_root: &Path, output: &Path) -> Result<PathBuf> {
    let data_root = require_real_directory(data_root, "private data root")?;
    let output = resolve_path_location(output, "public output location")?;
    if output.starts_with(&data_root) || data_root.starts_with(output.as_path()) {
        bail!(
            "--output and --data-root must be separate, non-nested directories: {} and {}",
            output.display(),
            data_root.display()
        );
    }
    Ok(data_root)
}

fn resolve_fresh_output(output: &Path) -> Result<PathBuf> {
    let output = resolve_path_location(output, "public output location")?;
    require_absent_path(&output, "--output")?;
    Ok(output)
}

fn resolve_path_location(path: &Path, label: &str) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .context("failed to resolve the current directory")?
            .join(path)
    };
    reject_symlink_components(&absolute)?;
    match fs::symlink_metadata(&absolute) {
        Ok(_) => {
            return absolute
                .canonicalize()
                .with_context(|| format!("failed to canonicalize {label} {}", absolute.display()));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect {label} {}", absolute.display()));
        }
    }
    let file_name = absolute
        .file_name()
        .filter(|name| !name.is_empty())
        .context("--output must name a new release directory")?
        .to_os_string();
    let parent = absolute
        .parent()
        .context("--output must have an existing parent directory")?;
    let parent = require_real_directory(parent, "public output parent")?;
    Ok(parent.join(file_name))
}

fn require_absent_path(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => bail!(
            "{label} must not already exist; export each release to a fresh directory: {}",
            path.display()
        ),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to inspect {label} path {}", path.display()))
        }
    }
}

struct StagingDirectory {
    path: PathBuf,
    promoted: bool,
}

impl StagingDirectory {
    fn create(parent: &Path) -> Result<Self> {
        for _ in 0..32 {
            let candidate = unique_temporary_path(parent, "public-release", "staging")?;
            match fs::create_dir(&candidate) {
                Ok(()) => {
                    return Ok(Self {
                        path: candidate,
                        promoted: false,
                    });
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "failed to create public release staging directory {}",
                            candidate.display()
                        )
                    });
                }
            }
        }
        bail!(
            "failed to create a public release staging directory in {}",
            parent.display()
        )
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn promote(mut self, output: &Path) -> Result<()> {
        require_absent_path(output, "--output")?;
        fs::rename(&self.path, output).with_context(|| {
            format!(
                "failed to promote completed public release {} to {}",
                self.path.display(),
                output.display()
            )
        })?;
        self.promoted = true;
        Ok(())
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if !self.promoted {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn stage_and_promote<T>(output: &Path, build: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    let parent = output
        .parent()
        .context("public release output has no parent")?;
    let staging = StagingDirectory::create(parent)?;
    let value = build(staging.path())?;
    staging.promote(output)?;
    Ok(value)
}

fn validate_existing_release(output: &Path, app_root: &Path) -> Result<PublicCatalogManifest> {
    let output = require_real_directory(output, "public release")?;
    let app_root = require_real_directory(app_root, "public app root")?;
    validate_output_hygiene(&output)?;
    for relative in PUBLIC_APP_FILES {
        let relative = safe_relative_path(relative)?;
        let published = require_asset_path(&output, &relative)?;
        let source = require_asset_path(&app_root, &relative)?;
        if !files_are_identical(&published, &source)? {
            bail!(
                "published public app asset differs from checked-out source: {}",
                relative.display()
            );
        }
    }
    validate_public_catalog_release(&output)
}

fn files_are_identical(left: &Path, right: &Path) -> Result<bool> {
    let left_metadata = fs::symlink_metadata(left)
        .with_context(|| format!("failed to inspect public app asset {}", left.display()))?;
    let right_metadata = fs::symlink_metadata(right)
        .with_context(|| format!("failed to inspect public app asset {}", right.display()))?;
    if left_metadata.file_type().is_symlink()
        || !left_metadata.is_file()
        || right_metadata.file_type().is_symlink()
        || !right_metadata.is_file()
    {
        bail!(
            "compared assets must be regular non-symlink files: {} and {}",
            left.display(),
            right.display()
        );
    }
    if left_metadata.len() != right_metadata.len() {
        return Ok(false);
    }
    let mut left = std::io::BufReader::new(
        File::open(left).context("failed to open published public app asset")?,
    );
    let mut right = std::io::BufReader::new(
        File::open(right).context("failed to open checked-out public app asset")?,
    );
    let mut left_buffer = [0_u8; 64 * 1024];
    let mut right_buffer = [0_u8; 64 * 1024];
    loop {
        let left_bytes = left
            .read(&mut left_buffer)
            .context("failed to read published public app asset")?;
        let right_bytes = right
            .read(&mut right_buffer)
            .context("failed to read checked-out public app asset")?;
        if left_bytes != right_bytes || left_buffer[..left_bytes] != right_buffer[..right_bytes] {
            return Ok(false);
        }
        if left_bytes == 0 {
            return Ok(true);
        }
    }
}

fn validate_output_hygiene(output: &Path) -> Result<()> {
    let allowed_files = PUBLIC_APP_FILES
        .iter()
        .map(PathBuf::from)
        .chain([PathBuf::from(PUBLIC_CATALOG_MANIFEST_FILE)])
        .collect::<std::collections::BTreeSet<_>>();
    validate_output_directory(output, Path::new(""), &allowed_files)
}

fn validate_output_directory(
    root: &Path,
    relative: &Path,
    allowed_files: &std::collections::BTreeSet<PathBuf>,
) -> Result<()> {
    let directory = root.join(relative);
    for entry in fs::read_dir(&directory)
        .with_context(|| format!("failed to inspect public output {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let child_relative = relative.join(entry.file_name());
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            bail!("public output cannot contain symlinks: {}", path.display());
        }
        if metadata.is_dir() {
            let is_managed_directory = child_relative == Path::new("data")
                || allowed_files
                    .iter()
                    .any(|allowed| allowed.starts_with(&child_relative));
            if !is_managed_directory {
                bail!(
                    "public output contains unexpected directory: {}",
                    path.display()
                );
            }
            validate_output_directory(root, &child_relative, allowed_files)?;
            continue;
        }
        if !metadata.is_file() {
            bail!(
                "public output contains a non-file entry: {}",
                path.display()
            );
        }
        let managed_database = child_relative
            .parent()
            .is_some_and(|parent| parent == Path::new("data"))
            && child_relative
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_content_addressed_database_name);
        if !allowed_files.contains(&child_relative) && !managed_database {
            bail!("public output contains unexpected file: {}", path.display());
        }
    }
    Ok(())
}

fn is_content_addressed_database_name(name: &str) -> bool {
    let base = name
        .strip_suffix(".br")
        .or_else(|| name.strip_suffix(".gz"))
        .unwrap_or(name);
    let Some(digest) = base
        .strip_prefix("catalog-")
        .and_then(|name| name.strip_suffix(".sqlite3"))
    else {
        return false;
    };
    digest.len() == 64
        && digest
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
}

fn safe_relative_path(value: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("invalid public app allowlist path '{value}'");
    }
    Ok(path)
}

fn require_asset_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    require_relative_file_path(root, relative, "public app asset")
}

fn require_relative_file_path(root: &Path, relative: &Path, label: &str) -> Result<PathBuf> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            bail!("invalid {label} path {}", relative.display());
        };
        current.push(component);
        let metadata = fs::symlink_metadata(&current)
            .with_context(|| format!("missing required {label} {}", current.display()))?;
        if metadata.file_type().is_symlink() {
            bail!("{label} cannot use symlinks: {}", current.display());
        }
        let is_last = current == root.join(relative);
        if is_last && !metadata.is_file() {
            bail!("{label} is not a regular file: {}", current.display());
        }
        if !is_last && !metadata.is_dir() {
            bail!("{label} parent is not a directory: {}", current.display());
        }
    }
    Ok(current)
}

fn require_real_directory(path: &Path, label: &str) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {label} {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("{label} must be a real directory: {}", path.display());
    }
    path.canonicalize()
        .with_context(|| format!("failed to canonicalize {label} {}", path.display()))
}

fn prepare_real_directory(path: &Path, label: &str) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .context("failed to resolve current directory")?
            .join(path)
    };
    reject_symlink_components(&absolute)?;
    fs::create_dir_all(&absolute)
        .with_context(|| format!("failed to create {label} {}", absolute.display()))?;
    reject_symlink_components(&absolute)?;
    require_real_directory(&absolute, label)
}

fn reject_symlink_components(path: &Path) -> Result<()> {
    let mut ancestors = path.ancestors().collect::<Vec<_>>();
    ancestors.reverse();
    for ancestor in ancestors {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "refusing path with symlink component: {}",
                    ancestor.display()
                )
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to inspect path component {}", ancestor.display())
                })
            }
        }
    }
    Ok(())
}

fn copy_file_atomically(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .context("public app destination has no parent")?;
    let temporary = unique_temporary_path(parent, "public-app-asset", "tmp")?;
    let result = (|| -> Result<()> {
        let source_path_metadata = fs::symlink_metadata(source)
            .with_context(|| format!("failed to inspect source file {}", source.display()))?;
        if source_path_metadata.file_type().is_symlink() || !source_path_metadata.is_file() {
            bail!(
                "source must be a regular non-symlink file: {}",
                source.display()
            );
        }
        let mut input = File::open(source)
            .with_context(|| format!("failed to open source file {}", source.display()))?;
        let input_metadata = input
            .metadata()
            .with_context(|| format!("failed to inspect source file {}", source.display()))?;
        let current_path_metadata = fs::symlink_metadata(source)
            .with_context(|| format!("failed to re-inspect source file {}", source.display()))?;
        if !input_metadata.is_file()
            || current_path_metadata.file_type().is_symlink()
            || !current_path_metadata.is_file()
            || source_path_metadata.len() != input_metadata.len()
            || current_path_metadata.len() != input_metadata.len()
        {
            bail!(
                "source file changed before it was copied: {}",
                source.display()
            );
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| {
                format!(
                    "failed to create temporary public app asset {}",
                    temporary.display()
                )
            })?;
        let copied = std::io::copy(&mut input, &mut output)
            .with_context(|| format!("failed to copy source file {}", source.display()))?;
        if copied != input_metadata.len() {
            bail!(
                "source file changed while it was copied: {}",
                source.display()
            );
        }
        output
            .flush()
            .context("failed to flush a public app asset")?;
        output
            .sync_all()
            .context("failed to synchronize a public app asset")?;
        drop(output);
        atomic_replace(&temporary, destination).with_context(|| {
            format!(
                "failed to atomically publish public app asset {}",
                destination.display()
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn unique_temporary_path(directory: &Path, label: &str, extension: &str) -> Result<PathBuf> {
    for _ in 0..32 {
        let mut random = [0_u8; 16];
        getrandom(&mut random).map_err(|error| {
            anyhow::anyhow!("failed to obtain randomness for asset publication: {error}")
        })?;
        let token = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let candidate = directory.join(format!(".{label}-{token}.{extension}"));
        match fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(candidate),
            Ok(_) => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to inspect temporary path {}", candidate.display())
                })
            }
        }
    }
    bail!(
        "failed to allocate a temporary asset path in {}",
        directory.display()
    )
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "Kernel32")]
    extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both paths are owned, NUL-terminated UTF-16 buffers that remain
    // alive for the duration of the call.
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn print_help() {
    println!(
        "Export the sanitized public catalog and static app\n\n\
         Usage:\n  export-public --data-root PATH --output PATH [--app-root PATH]\n  \
         export-public --assemble-catalog SOURCE --output NEW [--app-root PATH]\n  \
         export-public --validate-release PATH [--app-root PATH]\n  \
         export-public --list-app-files\n\n\
         Options:\n  --data-root PATH  Directory containing the live minerals.db\n  \
         --output PATH     New static release directory; must not already exist\n  \
         --assemble-catalog SOURCE\n                    Assemble a full release from a catalog-only source\n  \
         --validate-release PATH\n                    Validate an existing static release without changing it\n  \
         --app-root PATH   Public app source directory (default: public-app)\n  \
         --list-app-files  Print the public app allowlist, one path per line\n  \
         -h, --help        Show this help"
    );
}

#[cfg(test)]
mod tests {
    use std::fs;

    use anyhow::Result;
    use rusqlite::Connection;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn options_require_roots_and_default_the_app_root() -> Result<()> {
        let options = Options::parse(vec![
            "--data-root".into(),
            "data".into(),
            "--output".into(),
            "dist".into(),
        ])?
        .unwrap();
        assert_eq!(
            options.mode,
            Mode::Export {
                data_root: PathBuf::from("data"),
                output: PathBuf::from("dist"),
            }
        );
        assert_eq!(options.app_root, PathBuf::from("public-app"));
        assert!(Options::parse(vec!["--help".into()])?.is_none());
        assert!(Options::parse(vec!["--data-root".into(), "data".into()]).is_err());
        Ok(())
    }

    #[test]
    fn validation_mode_is_mutually_exclusive_with_export_mode() -> Result<()> {
        let options = Options::parse(vec![
            "--validate-release".into(),
            "release".into(),
            "--app-root".into(),
            "app".into(),
        ])?
        .unwrap();
        assert_eq!(
            options,
            Options {
                app_root: PathBuf::from("app"),
                mode: Mode::Validate {
                    release: PathBuf::from("release"),
                },
            }
        );
        assert!(Options::parse(vec![
            "--validate-release".into(),
            "release".into(),
            "--output".into(),
            "dist".into(),
        ])
        .is_err());
        assert!(Options::parse(vec![
            "--validate-release".into(),
            "release".into(),
            "--data-root".into(),
            "data".into(),
        ])
        .is_err());
        Ok(())
    }

    #[test]
    fn assembly_mode_is_mutually_exclusive_and_requires_a_fresh_output_name() -> Result<()> {
        let options = Options::parse(vec![
            "--assemble-catalog".into(),
            "catalog-source".into(),
            "--output".into(),
            "release".into(),
            "--app-root".into(),
            "app".into(),
        ])?
        .unwrap();
        assert_eq!(
            options,
            Options {
                app_root: PathBuf::from("app"),
                mode: Mode::Assemble {
                    source: PathBuf::from("catalog-source"),
                    output: PathBuf::from("release"),
                },
            }
        );
        assert!(
            Options::parse(vec!["--assemble-catalog".into(), "catalog-source".into(),]).is_err()
        );
        assert!(Options::parse(vec![
            "--assemble-catalog".into(),
            "catalog-source".into(),
            "--data-root".into(),
            "private".into(),
            "--output".into(),
            "release".into(),
        ])
        .is_err());
        assert!(Options::parse(vec![
            "--assemble-catalog".into(),
            "catalog-source".into(),
            "--validate-release".into(),
            "release".into(),
        ])
        .is_err());
        Ok(())
    }

    #[test]
    fn app_file_listing_is_a_standalone_mode_without_an_app_root() -> Result<()> {
        assert_eq!(
            Options::parse(vec!["--list-app-files".into()])?.unwrap(),
            Options {
                app_root: PathBuf::from("public-app"),
                mode: Mode::ListAppFiles,
            }
        );
        assert!(Options::parse(vec![
            "--list-app-files".into(),
            "--app-root".into(),
            "missing".into(),
        ])
        .is_err());
        assert!(
            Options::parse(vec!["--list-app-files".into(), "--list-app-files".into(),]).is_err()
        );
        Ok(())
    }

    #[test]
    fn validation_release_and_source_must_be_separate() -> Result<()> {
        let root = TempDir::new()?;
        let app = root.path().join("app");
        let release = root.path().join("release");
        let nested = release.join("app");
        fs::create_dir(&app)?;
        fs::create_dir(&release)?;
        fs::create_dir(&nested)?;
        validate_release_app_separation(&release, &app)?;
        assert!(validate_release_app_separation(&release, &nested).is_err());
        assert!(validate_release_app_separation(&release, &release).is_err());
        Ok(())
    }

    #[test]
    fn assembly_source_app_and_output_must_be_pairwise_non_nested() -> Result<()> {
        let root = TempDir::new()?;
        let app = root.path().join("app");
        let nested_source = app.join("catalog-source");
        let separate_source = root.path().join("catalog-source");
        let output = root.path().join("release");
        fs::create_dir(&app)?;
        fs::create_dir(&nested_source)?;
        fs::create_dir(&separate_source)?;

        assert!(validate_assembly_separation(&nested_source, &app, &output).is_err());
        assert!(validate_assembly_separation(
            &separate_source,
            &app,
            &separate_source.join("nested-release")
        )
        .is_err());
        validate_assembly_separation(&separate_source, &app, &output)?;
        Ok(())
    }

    #[test]
    fn public_app_copy_is_a_strict_allowlist_and_validates_every_asset() -> Result<()> {
        let app = TempDir::new()?;
        let output = TempDir::new()?;
        for relative in PUBLIC_APP_FILES {
            let path = app.path().join(relative);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(&path, format!("asset:{relative}"))?;
        }
        fs::write(app.path().join("private-template.html"), "must not copy")?;

        copy_public_app(app.path(), output.path())?;
        for relative in PUBLIC_APP_FILES {
            assert_eq!(
                fs::read_to_string(output.path().join(relative))?,
                format!("asset:{relative}")
            );
        }
        assert!(!output.path().join("private-template.html").exists());

        fs::remove_file(app.path().join("catalog-worker.js"))?;
        let error = copy_public_app(app.path(), output.path()).unwrap_err();
        assert!(error
            .to_string()
            .contains("missing required public app asset"));
        Ok(())
    }

    #[test]
    fn public_app_copy_rejects_unmanaged_destination_content() -> Result<()> {
        let app = TempDir::new()?;
        let output = TempDir::new()?;
        for relative in PUBLIC_APP_FILES {
            let path = app.path().join(relative);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, "managed")?;
        }
        let private = output.path().join("private-backup.sqlite3");
        fs::write(&private, "private")?;

        let error = copy_public_app(app.path(), output.path()).unwrap_err();
        assert!(error.to_string().contains("unexpected file"));
        assert_eq!(fs::read_to_string(private)?, "private");
        assert!(!output.path().join("index.html").exists());
        Ok(())
    }

    #[test]
    fn public_app_copy_preserves_atlas_assets_byte_for_byte() -> Result<()> {
        let app = TempDir::new()?;
        let output = TempDir::new()?;
        for relative in PUBLIC_APP_FILES {
            let path = app.path().join(relative);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, "managed")?;
        }

        let quartz = b"\x89PNG\r\n\x1a\nquartz\0plate";
        let source = b"\x89PNG\r\n\x1a\nsource\0strata";
        fs::write(app.path().join("assets/atlas-quartz-v2.png"), quartz)?;
        fs::write(app.path().join("assets/atlas-source-v2.png"), source)?;

        copy_public_app(app.path(), output.path())?;

        assert_eq!(
            fs::read(output.path().join("assets/atlas-quartz-v2.png"))?,
            quartz
        );
        assert_eq!(
            fs::read(output.path().join("assets/atlas-source-v2.png"))?,
            source
        );
        Ok(())
    }

    #[test]
    fn data_root_and_output_must_not_be_nested() -> Result<()> {
        let root = TempDir::new()?;
        let data = root.path().join("private-data");
        fs::create_dir(&data)?;
        let nested_output = data.join("public");
        assert!(validate_private_output_separation(&data, &nested_output).is_err());

        let public = root.path().join("public");
        fs::create_dir(&public)?;
        let nested_data = public.join("private-data");
        fs::create_dir(&nested_data)?;
        assert!(validate_private_output_separation(&nested_data, &public).is_err());
        Ok(())
    }

    #[test]
    fn fresh_output_rejects_existing_paths() -> Result<()> {
        let root = TempDir::new()?;
        let fresh = root.path().join("release-v1");
        assert_eq!(
            resolve_fresh_output(&fresh)?,
            root.path().canonicalize()?.join("release-v1")
        );

        fs::create_dir(&fresh)?;
        let error = resolve_fresh_output(&fresh).unwrap_err();
        assert!(error.to_string().contains("must not already exist"));

        let missing_parent = root.path().join("missing").join("release-v2");
        let error = resolve_fresh_output(&missing_parent).unwrap_err();
        assert!(error.to_string().contains("public output parent"));
        Ok(())
    }

    #[test]
    fn staging_promotes_only_complete_success_and_cleans_failures() -> Result<()> {
        let root = TempDir::new()?;
        let successful = root.path().join("release-success");
        let value = stage_and_promote(&successful, |staging| {
            assert!(!successful.exists());
            fs::write(staging.join("complete.txt"), "complete")?;
            Ok(42)
        })?;
        assert_eq!(value, 42);
        assert_eq!(
            fs::read_to_string(successful.join("complete.txt"))?,
            "complete"
        );

        let failed = root.path().join("release-failed");
        let error = stage_and_promote(&failed, |staging| -> Result<()> {
            fs::write(staging.join("partial.txt"), "partial")?;
            bail!("simulated export failure")
        })
        .unwrap_err();
        assert!(error.to_string().contains("simulated export failure"));
        assert!(!failed.exists());
        assert!(!fs::read_dir(root.path())?.any(|entry| {
            entry.is_ok_and(|entry| entry.file_name().to_string_lossy().contains(".staging"))
        }));
        Ok(())
    }

    #[test]
    fn staging_never_replaces_an_existing_output() -> Result<()> {
        let root = TempDir::new()?;
        let output = root.path().join("deployed-release");
        fs::create_dir(&output)?;
        fs::write(output.join("marker.txt"), "original")?;

        let error = stage_and_promote(&output, |staging| {
            fs::write(staging.join("marker.txt"), "replacement")?;
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("must not already exist"));
        assert_eq!(fs::read_to_string(output.join("marker.txt"))?, "original");
        assert_eq!(fs::read_dir(root.path())?.count(), 1);
        Ok(())
    }

    #[test]
    fn static_asset_comparison_is_byte_exact() -> Result<()> {
        let root = TempDir::new()?;
        let left = root.path().join("left");
        let right = root.path().join("right");
        fs::write(&left, b"same bytes")?;
        fs::write(&right, b"same bytes")?;
        assert!(files_are_identical(&left, &right)?);
        fs::write(&right, b"same bytex")?;
        assert!(!files_are_identical(&left, &right)?);
        fs::write(&right, b"different!")?;
        assert!(!files_are_identical(&left, &right)?);
        Ok(())
    }

    #[test]
    fn assembles_validated_catalog_and_app_bytes_into_a_fresh_release() -> Result<()> {
        let root = TempDir::new()?;
        let source = root.path().join("catalog-source");
        let app = root.path().join("public-app");
        let output = root.path().join("release");
        let expected = prepare_test_catalog(root.path(), &source)?;
        prepare_test_app(&app)?;

        let published = assemble_catalog_release(&source, &app, &output)?;

        assert_eq!(published, expected);
        assert_eq!(validate_existing_release(&output, &app)?, expected);
        for relative in catalog_snapshot_files(&expected)? {
            assert_eq!(
                fs::read(source.join(&relative))?,
                fs::read(output.join(&relative))?,
                "assembled catalog bytes changed for {}",
                relative.display()
            );
        }
        for relative in PUBLIC_APP_FILES {
            assert_eq!(
                fs::read(app.join(relative))?,
                fs::read(output.join(relative))?,
                "assembled app bytes changed for {relative}"
            );
        }
        Ok(())
    }

    #[test]
    fn catalog_assembly_rejects_extra_source_entries_without_creating_output() -> Result<()> {
        let root = TempDir::new()?;
        let source = root.path().join("catalog-source");
        let app = root.path().join("public-app");
        let output = root.path().join("release");
        prepare_test_catalog(root.path(), &source)?;
        fs::create_dir(&app)?;
        fs::write(source.join("private-notes.txt"), b"must not publish")?;

        let error = assemble_catalog_release(&source, &app, &output).unwrap_err();
        assert!(format!("{error:#}").contains("unexpected entry"));
        assert!(!output.exists());
        Ok(())
    }

    #[test]
    fn catalog_assembly_never_replaces_an_existing_output() -> Result<()> {
        let root = TempDir::new()?;
        let source = root.path().join("catalog-source");
        let app = root.path().join("public-app");
        let output = root.path().join("release");
        fs::create_dir(&source)?;
        fs::create_dir(&app)?;
        fs::create_dir(&output)?;
        fs::write(output.join("marker.txt"), b"keep me")?;

        let error = assemble_catalog_release(&source, &app, &output).unwrap_err();
        assert!(format!("{error:#}").contains("must not already exist"));
        assert_eq!(fs::read(output.join("marker.txt"))?, b"keep me");
        Ok(())
    }

    #[test]
    fn catalog_assembly_rejects_symlinked_source_files_where_supported() -> Result<()> {
        let root = TempDir::new()?;
        let source = root.path().join("catalog-source");
        let app = root.path().join("public-app");
        let output = root.path().join("release");
        let external_manifest = root.path().join("external-manifest.json");
        fs::create_dir(&source)?;
        fs::create_dir(source.join("data"))?;
        fs::create_dir(&app)?;
        fs::write(&external_manifest, b"{}")?;
        if !try_create_file_symlink(
            &external_manifest,
            &source.join(PUBLIC_CATALOG_MANIFEST_FILE),
        )? {
            return Ok(());
        }

        let error = assemble_catalog_release(&source, &app, &output).unwrap_err();
        assert!(format!("{error:#}").contains("cannot contain symlinks"));
        assert!(!output.exists());
        Ok(())
    }

    fn prepare_test_app(app: &Path) -> Result<()> {
        for (position, relative) in PUBLIC_APP_FILES.iter().enumerate() {
            let path = app.join(relative);
            fs::create_dir_all(path.parent().context("test app asset has no parent")?)?;
            fs::write(path, format!("test app asset {position}: {relative}\0"))?;
        }
        Ok(())
    }

    fn prepare_test_catalog(root: &Path, source: &Path) -> Result<PublicCatalogManifest> {
        let data_root = root.join("private-registry");
        fs::create_dir(&data_root)?;
        fs::create_dir(source)?;
        let connection = Connection::open(data_root.join("minerals.db"))?;
        connection.execute_batch(
            r#"
            CREATE TABLE images (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                stored_name TEXT NOT NULL UNIQUE
            );
            CREATE TABLE minerals (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                slug TEXT NOT NULL UNIQUE,
                common_name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                mineral_family TEXT NOT NULL,
                formula TEXT NOT NULL,
                hardness_mohs REAL NOT NULL,
                density_g_cm3 REAL NOT NULL,
                crystal_system TEXT NOT NULL,
                color TEXT NOT NULL,
                streak TEXT NOT NULL,
                luster TEXT NOT NULL,
                major_elements_pct_json TEXT NOT NULL,
                notes TEXT NOT NULL,
                image_id INTEGER
            );
            "#,
        )?;
        drop(connection);
        minerals::registry::init_registry_database_with_options(&data_root, false)?;
        export_public_catalog(&data_root, source)
    }

    #[cfg(unix)]
    fn try_create_file_symlink(target: &Path, link: &Path) -> Result<bool> {
        std::os::unix::fs::symlink(target, link)?;
        Ok(true)
    }

    #[cfg(windows)]
    fn try_create_file_symlink(target: &Path, link: &Path) -> Result<bool> {
        match std::os::windows::fs::symlink_file(target, link) {
            Ok(()) => Ok(true),
            Err(error)
                if error.kind() == ErrorKind::PermissionDenied
                    || error.raw_os_error() == Some(1314) =>
            {
                Ok(false)
            }
            Err(error) => Err(error.into()),
        }
    }

    #[cfg(not(any(unix, windows)))]
    fn try_create_file_symlink(_target: &Path, _link: &Path) -> Result<bool> {
        Ok(false)
    }
}
