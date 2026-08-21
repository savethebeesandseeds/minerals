use std::{
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::{Component, Path, PathBuf},
    process::ExitCode,
};

use anyhow::{bail, Context, Result};
use getrandom::getrandom;
use minerals_public_catalog::{
    export_public_catalog, PublicCatalogManifest, PUBLIC_CATALOG_MANIFEST_FILE,
};

const PUBLIC_APP_FILES: &[&str] = &[
    "index.html",
    "app.css",
    "app.js",
    "app-core.mjs",
    "catalog-worker.js",
    "THIRD_PARTY_NOTICES.md",
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
    let output = resolve_fresh_output(&options.output)?;
    let data_root = validate_private_output_separation(&options.data_root, &output)?;
    let app_root = require_real_directory(&options.app_root, "public app root")?;
    if output.starts_with(&app_root) {
        bail!(
            "--output cannot be inside --app-root: {} and {}",
            output.display(),
            app_root.display()
        );
    }

    let manifest = stage_and_promote(&output, |staging| {
        copy_public_app(&app_root, staging)?;
        let manifest = export_public_catalog(&data_root, staging)?;
        validate_complete_release(staging, &manifest)?;
        Ok(manifest)
    })?;
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    data_root: PathBuf,
    output: PathBuf,
    app_root: PathBuf,
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
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            let name = argument
                .to_str()
                .context("option names must be valid Unicode")?;
            let slot = match name {
                "--data-root" => &mut data_root,
                "--output" => &mut output,
                "--app-root" => &mut app_root,
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
        Ok(Some(Self {
            data_root: data_root.context("missing required --data-root PATH")?,
            output: output.context("missing required --output PATH")?,
            app_root: app_root.unwrap_or_else(|| PathBuf::from("public-app")),
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

fn validate_complete_release(output: &Path, expected: &PublicCatalogManifest) -> Result<()> {
    validate_output_hygiene(output)?;
    for relative in PUBLIC_APP_FILES {
        let relative = safe_relative_path(relative)?;
        require_asset_path(output, &relative)?;
    }

    let manifest_path = require_asset_path(output, Path::new(PUBLIC_CATALOG_MANIFEST_FILE))?;
    let published: PublicCatalogManifest =
        serde_json::from_reader(File::open(&manifest_path).with_context(|| {
            format!(
                "failed to open completed public catalog manifest {}",
                manifest_path.display()
            )
        })?)
        .context("failed to parse completed public catalog manifest")?;
    if &published != expected {
        bail!("completed public catalog manifest differs from the exported release");
    }

    let database_relative = safe_relative_path(&published.database.path)?;
    let database_digest = published
        .database
        .sha256
        .strip_prefix("sha256:")
        .filter(|digest| {
            digest.len() == 64
                && digest
                    .chars()
                    .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
        });
    let expected_database_relative =
        database_digest.map(|digest| PathBuf::from(format!("data/catalog-{digest}.sqlite3")));
    if database_relative.parent() != Some(Path::new("data"))
        || database_relative
            .file_name()
            .and_then(|name| name.to_str())
            .is_none_or(|name| !is_content_addressed_database_name(name))
        || expected_database_relative.as_ref() != Some(&database_relative)
    {
        bail!("completed public catalog manifest names an invalid or mismatched database path");
    }
    let database_path = require_asset_path(output, &database_relative)?;
    let database_bytes = fs::metadata(&database_path)
        .with_context(|| {
            format!(
                "failed to inspect completed public catalog database {}",
                database_path.display()
            )
        })?
        .len();
    if database_bytes != published.database.bytes {
        bail!(
            "completed public catalog database size is {database_bytes}, expected {}",
            published.database.bytes
        );
    }

    let data_directory = output.join("data");
    let database_count = fs::read_dir(&data_directory)
        .with_context(|| {
            format!(
                "failed to inspect completed public catalog data directory {}",
                data_directory.display()
            )
        })?
        .count();
    if database_count != 1 {
        bail!("completed public catalog must contain exactly one database, found {database_count}");
    }
    Ok(())
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
    let Some(digest) = name
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
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            bail!("invalid public app asset path {}", relative.display());
        };
        current.push(component);
        let metadata = fs::symlink_metadata(&current)
            .with_context(|| format!("missing required public app asset {}", current.display()))?;
        if metadata.file_type().is_symlink() {
            bail!(
                "public app assets cannot use symlinks: {}",
                current.display()
            );
        }
        let is_last = current == root.join(relative);
        if is_last && !metadata.is_file() {
            bail!(
                "public app asset is not a regular file: {}",
                current.display()
            );
        }
        if !is_last && !metadata.is_dir() {
            bail!(
                "public app asset parent is not a directory: {}",
                current.display()
            );
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
        let mut input = File::open(source)
            .with_context(|| format!("failed to open public app asset {}", source.display()))?;
        let input_metadata = input
            .metadata()
            .with_context(|| format!("failed to inspect public app asset {}", source.display()))?;
        if !input_metadata.is_file() {
            bail!(
                "public app asset is not a regular file: {}",
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
            .with_context(|| format!("failed to copy public app asset {}", source.display()))?;
        if copied != input_metadata.len() {
            bail!(
                "public app asset changed while it was copied: {}",
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
         Usage:\n  export-public --data-root PATH --output PATH [--app-root PATH]\n\n\
         Options:\n  --data-root PATH  Directory containing the live minerals.db\n  \
         --output PATH     New static release directory; must not already exist\n  \
         --app-root PATH   Public app source directory (default: public-app)\n  \
         -h, --help        Show this help"
    );
}

#[cfg(test)]
mod tests {
    use std::fs;

    use anyhow::Result;
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
        assert_eq!(options.data_root, PathBuf::from("data"));
        assert_eq!(options.output, PathBuf::from("dist"));
        assert_eq!(options.app_root, PathBuf::from("public-app"));
        assert!(Options::parse(vec!["--help".into()])?.is_none());
        assert!(Options::parse(vec!["--data-root".into(), "data".into()]).is_err());
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
    fn completed_release_validation_requires_exact_manifest_database_and_assets() -> Result<()> {
        let output = TempDir::new()?;
        for relative in PUBLIC_APP_FILES {
            let path = output.path().join(relative);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, "asset")?;
        }
        let digest = "a".repeat(64);
        let database_relative = format!("data/catalog-{digest}.sqlite3");
        let database_path = output.path().join(&database_relative);
        fs::create_dir_all(database_path.parent().unwrap())?;
        fs::write(&database_path, "database")?;
        let manifest = PublicCatalogManifest {
            format: "waajacu-public-catalog-v1".to_string(),
            schema_version: 1,
            generated_at: "2026-08-21T00:00:00.000Z".to_string(),
            release_id: format!("sha256:{}", "b".repeat(64)),
            mineral_count: 0,
            database: minerals_public_catalog::PublicCatalogDatabase {
                path: database_relative,
                sha256: format!("sha256:{digest}"),
                bytes: 8,
            },
        };
        fs::write(
            output.path().join(PUBLIC_CATALOG_MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        validate_complete_release(output.path(), &manifest)?;

        fs::write(output.path().join("unexpected.txt"), "not public")?;
        assert!(validate_complete_release(output.path(), &manifest).is_err());
        Ok(())
    }
}
