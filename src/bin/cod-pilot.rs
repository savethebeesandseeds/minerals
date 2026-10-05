use std::{env, ffi::OsString, path::PathBuf, process::ExitCode};

use anyhow::{bail, Context, Result};
use minerals_cod_pilot::{fetch, prepare, verify, verify_execution};

#[tokio::main]
async fn main() -> ExitCode {
    match run(env::args_os().skip(1).collect()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cod-pilot failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(arguments: Vec<OsString>) -> Result<()> {
    let Some(command) = Options::parse(arguments)? else {
        print_help();
        return Ok(());
    };
    let output = match command {
        Options::Prepare { repo_root, output } => {
            serde_json::to_value(prepare(&repo_root, &output)?)?
        }
        Options::Verify { repo_root, input } => serde_json::to_value(verify(&repo_root, &input)?)?,
        Options::Fetch {
            repo_root,
            prepared,
            pilot_root,
            max_new_requests,
        } => serde_json::to_value(
            fetch(&repo_root, &prepared, &pilot_root, max_new_requests).await?,
        )?,
        Options::VerifyExecution {
            repo_root,
            prepared,
            pilot_root,
        } => serde_json::to_value(verify_execution(&repo_root, &prepared, &pilot_root)?)?,
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum Options {
    Prepare {
        repo_root: PathBuf,
        output: PathBuf,
    },
    Verify {
        repo_root: PathBuf,
        input: PathBuf,
    },
    Fetch {
        repo_root: PathBuf,
        prepared: PathBuf,
        pilot_root: PathBuf,
        max_new_requests: Option<u32>,
    },
    VerifyExecution {
        repo_root: PathBuf,
        prepared: PathBuf,
        pilot_root: PathBuf,
    },
}

impl Options {
    fn parse(arguments: Vec<OsString>) -> Result<Option<Self>> {
        if arguments
            .iter()
            .any(|argument| argument == "--help" || argument == "-h")
        {
            return Ok(None);
        }
        let mut arguments = arguments.into_iter();
        let command = arguments
            .next()
            .context("missing command; expected prepare, verify, fetch, or verify-execution")?;
        let command = command.to_str().context("command must be valid Unicode")?;
        if !["prepare", "verify", "fetch", "verify-execution"].contains(&command) {
            bail!(
                "unknown command '{command}'; expected prepare, verify, fetch, or verify-execution"
            );
        }

        let mut repo_root = None;
        let mut artifact_path = None;
        let mut pilot_root = None;
        let mut max_new_requests = None;
        while let Some(argument) = arguments.next() {
            let option = argument
                .to_str()
                .context("option names must be valid Unicode")?;
            if option == "--max-new-requests" {
                ensure_command(command, "fetch", option)?;
                if max_new_requests.is_some() {
                    bail!("duplicate option '{option}'");
                }
                let value = arguments
                    .next()
                    .with_context(|| format!("missing value for '{option}'"))?;
                let value = value
                    .to_str()
                    .context("--max-new-requests must be valid Unicode")?
                    .parse::<u32>()
                    .context("--max-new-requests must be an unsigned integer")?;
                if value > 1_000 {
                    bail!("--max-new-requests cannot exceed 1000");
                }
                if value == 0 {
                    bail!("--max-new-requests must be at least 1");
                }
                max_new_requests = Some(value);
                continue;
            }
            let slot = match option {
                "--repo-root" => &mut repo_root,
                "--output" if command == "prepare" => &mut artifact_path,
                "--input" if command == "verify" => &mut artifact_path,
                "--prepared" if command == "fetch" || command == "verify-execution" => {
                    &mut artifact_path
                }
                "--pilot-root" if command == "fetch" || command == "verify-execution" => {
                    &mut pilot_root
                }
                _ => bail!("option '{option}' is not valid for {command}"),
            };
            if slot.is_some() {
                bail!("duplicate option '{option}'");
            }
            let value = arguments
                .next()
                .with_context(|| format!("missing value for '{option}'"))?;
            if value.is_empty() {
                bail!("'{option}' cannot be empty");
            }
            *slot = Some(PathBuf::from(value));
        }
        let repo_root = repo_root.context("missing required --repo-root PATH")?;
        let artifact_path = artifact_path.with_context(|| match command {
            "prepare" => "missing required --output PATH",
            "verify" => "missing required --input PATH",
            _ => "missing required --prepared PATH",
        })?;
        Ok(Some(match command {
            "prepare" => Self::Prepare {
                repo_root,
                output: artifact_path,
            },
            "verify" => Self::Verify {
                repo_root,
                input: artifact_path,
            },
            "fetch" => Self::Fetch {
                repo_root,
                prepared: artifact_path,
                pilot_root: pilot_root.context("missing required --pilot-root PATH")?,
                max_new_requests,
            },
            "verify-execution" => Self::VerifyExecution {
                repo_root,
                prepared: artifact_path,
                pilot_root: pilot_root.context("missing required --pilot-root PATH")?,
            },
            _ => unreachable!("command was validated"),
        }))
    }
}

fn ensure_command(actual: &str, expected: &str, option: &str) -> Result<()> {
    if actual != expected {
        bail!("option '{option}' is only valid for {expected}");
    }
    Ok(())
}

fn print_help() {
    println!(
        "Prepare, verify, or explicitly fetch the private COD pilot\n\n\
         Usage:\n  cod-pilot prepare --repo-root PATH --output NEW_DIRECTORY\n  \
         cod-pilot verify --repo-root PATH --input PREPARED_DIRECTORY\n  \
         cod-pilot fetch --repo-root PATH --prepared PREPARED_DIRECTORY --pilot-root data/pilots/cod-crystallography-v1 [--max-new-requests N]\n  \
         cod-pilot verify-execution --repo-root PATH --prepared PREPARED_DIRECTORY --pilot-root data/pilots/cod-crystallography-v1\n\n\
         Prepare, verify, and verify-execution perform no network requests. Fetch is\n\
         sequential and writes only immutable raw bodies plus its execution index;\n\
         no command opens data/minerals.db or writes any database."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_explicit_prepare_and_verify_paths() -> Result<()> {
        assert_eq!(
            Options::parse(vec![
                "prepare".into(),
                "--repo-root".into(),
                "repo".into(),
                "--output".into(),
                "prepared".into(),
            ])?,
            Some(Options::Prepare {
                repo_root: "repo".into(),
                output: "prepared".into(),
            })
        );
        assert_eq!(
            Options::parse(vec![
                "verify".into(),
                "--input".into(),
                "prepared".into(),
                "--repo-root".into(),
                "repo".into(),
            ])?,
            Some(Options::Verify {
                repo_root: "repo".into(),
                input: "prepared".into(),
            })
        );
        assert!(Options::parse(vec!["prepare".into()]).is_err());
        assert!(Options::parse(vec!["verify".into(), "--output".into()]).is_err());
        assert_eq!(
            Options::parse(vec![
                "fetch".into(),
                "--repo-root".into(),
                "repo".into(),
                "--prepared".into(),
                "prepared".into(),
                "--pilot-root".into(),
                "data/pilots/cod-crystallography-v1".into(),
                "--max-new-requests".into(),
                "3".into(),
            ])?,
            Some(Options::Fetch {
                repo_root: "repo".into(),
                prepared: "prepared".into(),
                pilot_root: "data/pilots/cod-crystallography-v1".into(),
                max_new_requests: Some(3),
            })
        );
        assert!(Options::parse(vec![
            "fetch".into(),
            "--repo-root".into(),
            "repo".into(),
            "--prepared".into(),
            "prepared".into(),
            "--pilot-root".into(),
            "data/pilots/cod-crystallography-v1".into(),
            "--max-new-requests".into(),
            "0".into(),
        ])
        .is_err());
        Ok(())
    }
}
