use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use serde_json::{Value, to_value};

use crate::app::{Application, root_exists};
use crate::config::ResolvedConfig;
use crate::domain::{RecordDraft, RecordKind, RecordSnapshot};
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
    /// Create, version, or retrieve a source through the canonical application path.
    Source(EntityOptions),
    /// Create, version, or retrieve a mathematical concept.
    Concept(EntityOptions),
    /// Create, version, or retrieve a truth-valued claim.
    Claim(EntityOptions),
    /// Create, version, or retrieve one exact formal interpretation.
    Formalization(EntityOptions),
    /// Search the current canonical record heads through SQLite FTS5.
    Search(SearchOptions),
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

#[derive(Debug, Args)]
struct EntityOptions {
    #[command(subcommand)]
    action: EntityAction,
}

#[derive(Debug, Subcommand)]
enum EntityAction {
    /// Validate and create a new stable canonical object.
    Create(RecordCreateOptions),
    /// Validate and append an immutable version using compare-and-swap.
    Version(RecordVersionOptions),
    /// Retrieve the current head or one exact immutable version.
    Get(RecordGetOptions),
}

#[derive(Debug, Args)]
struct RecordCreateOptions {
    #[arg(long)]
    payload_json: String,

    #[arg(long)]
    searchable_text: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct RecordVersionOptions {
    #[arg(long)]
    object_id: String,

    #[arg(long)]
    expected_head: String,

    #[arg(long)]
    payload_json: String,

    #[arg(long)]
    searchable_text: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct RecordGetOptions {
    #[arg(long)]
    object_id: String,

    #[arg(long)]
    version_hash: Option<String>,
}

#[derive(Debug, Args)]
struct SearchOptions {
    #[arg(long)]
    query: String,

    #[arg(long, default_value_t = 20)]
    limit: usize,
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
            Command::Source(options) => execute_entity(&config, RecordKind::Source, options),
            Command::Concept(options) => execute_entity(&config, RecordKind::Concept, options),
            Command::Claim(options) => execute_entity(&config, RecordKind::Claim, options),
            Command::Formalization(options) => {
                execute_entity(&config, RecordKind::Formalization, options)
            }
            Command::Search(options) => {
                let application = Application::open(&config)?;
                Ok(CliOutcome {
                    value: to_value(application.search_records(&options.query, options.limit)?)
                        .expect("record search result is serializable"),
                    success: true,
                })
            }
        }
    }
}

fn execute_entity(
    config: &ResolvedConfig,
    kind: RecordKind,
    options: EntityOptions,
) -> Result<CliOutcome, AppError> {
    let mut application = Application::open(config)?;
    let value = match options.action {
        EntityAction::Create(options) => {
            let draft = record_draft(kind, &options.payload_json, options.searchable_text)?;
            to_value(application.create_record(
                &draft,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("record mutation outcome is serializable")
        }
        EntityAction::Version(options) => {
            let draft = record_draft(kind, &options.payload_json, options.searchable_text)?;
            to_value(application.version_record(
                &options.object_id,
                &options.expected_head,
                &draft,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("record mutation outcome is serializable")
        }
        EntityAction::Get(options) => {
            let record =
                application.get_record(&options.object_id, options.version_hash.as_deref())?;
            require_record_kind(&record, kind)?;
            to_value(record).expect("record snapshot is serializable")
        }
    };
    Ok(CliOutcome {
        value,
        success: true,
    })
}

fn record_draft(
    kind: RecordKind,
    payload_json: &str,
    searchable_text: String,
) -> Result<RecordDraft, AppError> {
    let payload = serde_json::from_str(payload_json).map_err(|error| {
        AppError::new(
            "MCL_CLI_JSON_INVALID",
            format!("payload JSON is invalid: {error}"),
            false,
            "Supply one complete JSON object through `--payload-json`.",
        )
    })?;
    Ok(RecordDraft {
        kind,
        schema_version: match kind {
            RecordKind::Source => "source/1",
            RecordKind::Concept => "concept/1",
            RecordKind::Claim => "claim/1",
            RecordKind::Formalization => "formalization/1",
            RecordKind::LearningUnit => "learning_unit/1",
        }
        .to_owned(),
        payload,
        searchable_text,
    })
}

fn require_record_kind(record: &RecordSnapshot, expected: RecordKind) -> Result<(), AppError> {
    if record.kind != expected {
        return Err(AppError::new(
            "MCL_RECORD_KIND_MISMATCH",
            format!(
                "requested `{expected}` object, but {} is `{}`",
                record.object_id, record.kind
            ),
            false,
            "Use the CLI family matching the canonical record kind.",
        ));
    }
    Ok(())
}
