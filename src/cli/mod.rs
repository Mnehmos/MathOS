use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use serde_json::{Value, to_value};

use crate::app::{Application, root_exists};
use crate::config::ResolvedConfig;
use crate::error::AppError;

#[derive(Debug)]
pub struct CliOutcome {
    pub value: Value,
    pub success: bool,
}

#[derive(Debug, Parser)]
#[command(name = "mcl", version, about = "Local-first Mathematical Claim Engine")]
pub struct Cli {
    #[arg(long, global = true, default_value = ".")]
    pub root: PathBuf,

    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize the real SQLite database and content-addressed artifact store.
    Init(MutationOptions),
    /// Check persisted storage without creating a missing instance.
    Health,
    /// Run storage checks and report Lean toolchain readiness.
    Doctor,
}

#[derive(Debug, Args)]
struct MutationOptions {
    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    actor: String,

    #[arg(long)]
    idempotency_key: String,
}

impl Cli {
    pub fn execute(self) -> Result<CliOutcome, AppError> {
        if !root_exists(&self.root)
            && matches!(&self.command, Command::Init(options) if options.dry_run)
        {
            return Err(AppError::new(
                "MCL_DRY_RUN_ROOT_MISSING",
                format!(
                    "dry-run root does not exist at {}; refusing to create it",
                    self.root.display()
                ),
                false,
                "Create the intended root directory, then repeat the dry run.",
            ));
        }
        if !root_exists(&self.root) && !matches!(self.command, Command::Init(_)) {
            return Err(AppError::new(
                "MCL_INSTANCE_NOT_INITIALIZED",
                format!("instance root does not exist at {}", self.root.display()),
                false,
                "Run `mcl init` with the intended root.",
            ));
        }
        let config = ResolvedConfig::load(&self.root, self.config.as_deref())?;
        match self.command {
            Command::Init(options) => Ok(CliOutcome {
                value: Application::initialize(
                    &config,
                    &options.actor,
                    &options.idempotency_key,
                    options.dry_run,
                )?,
                success: true,
            }),
            Command::Health => {
                let report = Application::health(&config);
                let success = report.healthy;
                Ok(CliOutcome {
                    value: to_value(report).expect("diagnostic report is serializable"),
                    success,
                })
            }
            Command::Doctor => {
                let report = Application::doctor(&config);
                let success = report.healthy;
                Ok(CliOutcome {
                    value: to_value(report).expect("diagnostic report is serializable"),
                    success,
                })
            }
        }
    }
}
