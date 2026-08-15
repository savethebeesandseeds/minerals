use anyhow::{bail, Context, Result};
use minerals::ima_adapter::{
    build_release_bundle, evolve_identity_ledger, initialize_identity_ledger,
    initialize_identity_ledger_with_overrides, load_identity_ledger, load_identity_overrides,
    load_verified_extraction, stage_release_bundle, verify_release_bundle, write_identity_ledger,
    ImaBundleBuildOptions,
};
use minerals::registry::MAX_MINERAL_INGESTION_CHUNK_ITEMS;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

const DEFAULT_TOKEN_ENV: &str = "INGESTION_API_TOKEN";

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ima-release failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    let mut arguments = env::args().skip(1);
    let Some(command) = arguments.next() else {
        print_help();
        return Ok(());
    };
    let rest = arguments.collect::<Vec<_>>();
    if command == "--help" || command == "-h" || command == "help" {
        print_help();
        return Ok(());
    }

    match command.as_str() {
        "ledger-init" => {
            let options = Options::parse(
                rest,
                &["extraction-index", "artifact", "overrides", "output"],
                &[],
            )?;
            let extraction = load_verified_extraction(
                &options.path("extraction-index")?,
                &options.path("artifact")?,
            )?;
            let ledger = if let Some(path) = options.optional("overrides") {
                let overrides = load_identity_overrides(&PathBuf::from(path))?;
                initialize_identity_ledger_with_overrides(&extraction.document, &overrides)?
            } else {
                initialize_identity_ledger(&extraction.document)?
            };
            let output = options.path("output")?;
            write_identity_ledger(&output, &ledger)?;
            print_json(&serde_json::json!({
                "entry_count": ledger.entries.len(),
                "output": output,
                "revision": ledger.revision,
            }))?;
        }
        "ledger-evolve" => {
            let options = Options::parse(
                rest,
                &["extraction-index", "artifact", "ledger", "output"],
                &["allow-new"],
            )?;
            let extraction = load_verified_extraction(
                &options.path("extraction-index")?,
                &options.path("artifact")?,
            )?;
            let previous = load_identity_ledger(&options.path("ledger")?)?;
            let ledger =
                evolve_identity_ledger(&extraction.document, &previous, options.flag("allow-new"))?;
            let output = options.path("output")?;
            write_identity_ledger(&output, &ledger)?;
            print_json(&serde_json::json!({
                "allow_new_identities": options.flag("allow-new"),
                "entry_count": ledger.entries.len(),
                "output": output,
                "revision": ledger.revision,
            }))?;
        }
        "build" => {
            let options = Options::parse(
                rest,
                &[
                    "extraction-index",
                    "artifact",
                    "ledger",
                    "output",
                    "released-at",
                    "base-batch-id",
                    "chunk-size",
                ],
                &[],
            )?;
            let chunk_size = options
                .optional("chunk-size")
                .map(|value| {
                    value
                        .parse::<usize>()
                        .context("--chunk-size must be an integer")
                })
                .transpose()?
                .unwrap_or(MAX_MINERAL_INGESTION_CHUNK_ITEMS);
            let build = ImaBundleBuildOptions {
                extraction_index_path: options.path("extraction-index")?,
                artifact_path: options.path("artifact")?,
                ledger_path: options.path("ledger")?,
                output: options.path("output")?,
                released_at: options.required("released-at")?.to_string(),
                base_batch_id: options.optional("base-batch-id").map(str::to_string),
                chunk_size,
            };
            let index = build_release_bundle(&build)?;
            print_json(&index)?;
        }
        "verify" => {
            let options = Options::parse(rest, &["bundle"], &[])?;
            let index = verify_release_bundle(&options.path("bundle")?)?;
            print_json(&index)?;
        }
        "stage" => {
            let options = Options::parse(rest, &["bundle", "server", "token-env"], &[])?;
            let token_env = options.optional("token-env").unwrap_or(DEFAULT_TOKEN_ENV);
            if token_env.is_empty()
                || token_env.contains('=')
                || token_env.chars().any(char::is_control)
            {
                bail!("--token-env must name one environment variable");
            }
            let token = env::var(token_env).with_context(|| {
                format!("staging token environment variable {token_env} is unset")
            })?;
            let bundle = options.path("bundle")?;
            let outcome =
                stage_release_bundle(&bundle, options.required("server")?, &token).await?;
            print_json(&serde_json::json!({
                "activation": "not_attempted_operator_review_required",
                "staging": outcome,
            }))?;
        }
        _ => bail!("unknown command '{command}'; run ima-release --help"),
    }
    Ok(())
}

#[derive(Debug)]
struct Options {
    values: BTreeMap<String, String>,
    flags: HashSet<String>,
}

impl Options {
    fn parse(arguments: Vec<String>, value_names: &[&str], flag_names: &[&str]) -> Result<Self> {
        if arguments
            .iter()
            .any(|argument| argument == "--help" || argument == "-h")
        {
            print_help();
            std::process::exit(0);
        }
        let allowed_values = value_names.iter().copied().collect::<HashSet<_>>();
        let allowed_flags = flag_names.iter().copied().collect::<HashSet<_>>();
        let mut values = BTreeMap::new();
        let mut flags = HashSet::new();
        let mut index = 0;
        while index < arguments.len() {
            let argument = &arguments[index];
            let Some(name) = argument.strip_prefix("--") else {
                bail!("unexpected positional argument '{argument}'");
            };
            if allowed_flags.contains(name) {
                if !flags.insert(name.to_string()) {
                    bail!("duplicate option '--{name}'");
                }
                index += 1;
                continue;
            }
            if !allowed_values.contains(name) {
                bail!("unknown option '--{name}'");
            }
            let value = arguments
                .get(index + 1)
                .filter(|value| !value.starts_with("--"))
                .with_context(|| format!("option '--{name}' requires a value"))?;
            if values.insert(name.to_string(), value.clone()).is_some() {
                bail!("duplicate option '--{name}'");
            }
            index += 2;
        }
        Ok(Self { values, flags })
    }

    fn required(&self, name: &str) -> Result<&str> {
        self.optional(name)
            .with_context(|| format!("missing required option '--{name}'"))
    }

    fn optional(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    fn path(&self, name: &str) -> Result<PathBuf> {
        Ok(PathBuf::from(self.required(name)?))
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.contains(name)
    }
}

fn print_json(value: &impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_help() {
    println!(
        r#"Deterministic IMA-CNMNC release bundle builder

Usage:
  ima-release ledger-init --extraction-index FILE --artifact PDF \
    [--overrides FILE] --output FILE
  ima-release ledger-evolve --extraction-index FILE --artifact PDF \
    --ledger FILE --output FILE [--allow-new]
  ima-release build --extraction-index FILE --artifact PDF \
    --ledger FILE --output DIR --released-at DATE \
    [--base-batch-id BATCH_ID] [--chunk-size 500]
  ima-release verify --bundle DIR
  ima-release stage --bundle DIR --server URL [--token-env NAME]

Safety:
  The initial official release requires an explicit Phenakite override mapping
  to mineral.silicates.0x5b6b8000 so it adopts the existing public route.
  ledger-evolve rejects new identities unless --allow-new is explicit.
  stage creates/reuses a quarantine batch, uploads chunks, and finalizes it.
  stage never approves or activates a release; operator review remains required.
  The token is read from INGESTION_API_TOKEN by default and is never accepted
  as a command-line value.
"#
    );
}
