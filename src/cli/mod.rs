use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::{Value, to_value};

use crate::app::{Application, root_exists};
use crate::config::ResolvedConfig;
use crate::domain::{
    ArtifactMetadata, EdgeDraft, EdgeKind, EnvironmentManifest, GraphTraversalRequest, RecordDraft,
    RecordKind, RecordSnapshot, RunEventDraft, RunEventKind, RunKind, TraversalDirection,
    VerifierJobRequest,
};
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
    /// Register or retrieve an immutable pinned Lean environment manifest.
    Environment(EnvironmentOptions),
    /// Ingest, retrieve, or verify a canonical content-addressed artifact.
    Artifact(ArtifactOptions),
    /// Enqueue or inspect verifier work without directly mutating mathematical status.
    Verify(VerifyOptions),
    /// Lease and execute at most one contained verifier job.
    Worker(WorkerOptions),
    /// Serve the Model Context Protocol over newline-delimited stdio.
    Serve,
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
    /// Create or retrieve exact version-bound graph edges.
    Edge(EdgeOptions),
    /// Traverse the version-bound graph with explicit typed bounds.
    Graph(GraphOptions),
    /// Start, inspect, append to, and verify non-authoritative research runs.
    Research(ResearchOptions),
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
struct EnvironmentOptions {
    #[command(subcommand)]
    action: EnvironmentAction,
}

#[derive(Debug, Subcommand)]
enum EnvironmentAction {
    Register(EnvironmentRegisterOptions),
    Get(EnvironmentGetOptions),
    List(EnvironmentListOptions),
}

#[derive(Debug, Args)]
struct EnvironmentRegisterOptions {
    #[arg(long)]
    manifest_json: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct EnvironmentGetOptions {
    #[arg(long)]
    environment_hash: String,
}

#[derive(Debug, Args)]
struct EnvironmentListOptions {
    #[arg(long, default_value_t = 20)]
    limit: usize,
}

#[derive(Debug, Args)]
struct ArtifactOptions {
    #[command(subcommand)]
    action: ArtifactAction,
}

#[derive(Debug, Subcommand)]
enum ArtifactAction {
    Ingest(ArtifactIngestOptions),
    Get(ArtifactGetOptions),
    List(ArtifactListOptions),
    Verify(ArtifactGetOptions),
}

#[derive(Debug, Args)]
struct ArtifactIngestOptions {
    #[arg(long)]
    input_file: PathBuf,

    #[arg(long)]
    metadata_json: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct ArtifactGetOptions {
    #[arg(long)]
    artifact_hash: String,
}

#[derive(Debug, Args)]
struct ArtifactListOptions {
    #[arg(long, default_value_t = 20)]
    limit: usize,
}

#[derive(Debug, Args)]
struct VerifyOptions {
    #[command(subcommand)]
    action: VerifyAction,
}

#[derive(Debug, Subcommand)]
enum VerifyAction {
    Check(VerifyCheckOptions),
    Status(VerifyStatusOptions),
    List(VerifyListOptions),
    PromoteDiagnostic(VerifyPromoteDiagnosticOptions),
    Evidence(VerifyEvidenceOptions),
    EvidenceList(VerifyListOptions),
    Audit(VerifyAuditOptions),
    AuditStatus(VerifyStatusOptions),
    AuditList(VerifyListOptions),
    PromoteAudit(VerifyPromoteDiagnosticOptions),
}

#[derive(Debug, Args)]
struct VerifyCheckOptions {
    #[arg(long)]
    environment_hash: String,

    #[arg(long)]
    module_artifact_hash: String,

    #[arg(long)]
    declaration_name: String,

    #[arg(long, default_value_t = 0)]
    priority: i32,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct VerifyStatusOptions {
    #[arg(long)]
    job_id: String,
}

#[derive(Debug, Args)]
struct VerifyListOptions {
    #[arg(long, default_value_t = 20)]
    limit: usize,
}

#[derive(Debug, Args)]
struct VerifyPromoteDiagnosticOptions {
    #[arg(long)]
    formalization_object_id: String,

    #[arg(long)]
    formalization_version_hash: String,

    #[arg(long)]
    job_id: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct VerifyEvidenceOptions {
    #[arg(long)]
    evidence_id: String,
}

#[derive(Debug, Args)]
struct VerifyAuditOptions {
    #[arg(long)]
    formalization_object_id: String,

    #[arg(long)]
    formalization_version_hash: String,

    #[arg(long)]
    diagnostic_evidence_id: String,

    #[arg(long, default_value_t = 0)]
    priority: i32,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum WorkerJobKind {
    Elaboration,
    Audit,
}

#[derive(Debug, Args)]
struct WorkerOptions {
    #[arg(long)]
    worker_id: String,

    #[arg(long, default_value_t = 3_660)]
    lease_seconds: u64,

    #[arg(long, value_enum, default_value_t = WorkerJobKind::Elaboration)]
    job_kind: WorkerJobKind,
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

#[derive(Debug, Args)]
struct EdgeOptions {
    #[command(subcommand)]
    action: EdgeAction,
}

#[derive(Debug, Subcommand)]
enum EdgeAction {
    Create(EdgeCreateOptions),
    Get(EdgeGetOptions),
}

#[derive(Debug, Args)]
struct EdgeCreateOptions {
    #[arg(long)]
    kind: String,

    #[arg(long)]
    source_object_id: String,

    #[arg(long)]
    source_version_hash: String,

    #[arg(long)]
    target_object_id: String,

    #[arg(long)]
    target_version_hash: String,

    #[arg(long, default_value = "{}")]
    payload_json: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct EdgeGetOptions {
    #[arg(long)]
    edge_id: String,
}

#[derive(Debug, Args)]
struct GraphOptions {
    #[arg(long)]
    root_object_id: String,

    #[arg(long)]
    root_version_hash: String,

    #[arg(long, default_value = "both")]
    direction: String,

    #[arg(long = "edge-kind")]
    edge_kinds: Vec<String>,

    #[arg(long, default_value_t = 1)]
    max_depth: u32,

    #[arg(long, default_value_t = 100)]
    limit: usize,
}

#[derive(Debug, Args)]
struct ResearchOptions {
    #[command(subcommand)]
    action: ResearchAction,
}

#[derive(Debug, Subcommand)]
enum ResearchAction {
    Start(RunStartOptions),
    Get(RunGetOptions),
    Events(RunGetOptions),
    Submit(RunSubmitOptions),
    Verify(RunGetOptions),
}

#[derive(Debug, Args)]
struct RunStartOptions {
    #[arg(long)]
    kind: String,

    #[arg(long, default_value = "{}")]
    budget_json: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct RunGetOptions {
    #[arg(long)]
    run_id: String,
}

#[derive(Debug, Args)]
struct RunSubmitOptions {
    #[arg(long)]
    run_id: String,

    #[arg(long)]
    expected_head: String,

    #[arg(long)]
    kind: String,

    #[arg(long, default_value = "{}")]
    payload_json: String,

    #[command(flatten)]
    mutation: MutationOptions,
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
            Command::Environment(options) => execute_environment(&config, options),
            Command::Artifact(options) => execute_artifact(&config, options),
            Command::Verify(options) => execute_verify(&config, options),
            Command::Worker(options) => {
                let mut application = Application::open(&config)?;
                let value = match options.job_kind {
                    WorkerJobKind::Elaboration => application
                        .work_one_verifier_job(&options.worker_id, options.lease_seconds)?
                        .map(|outcome| {
                            to_value(outcome).expect("verifier work outcome is serializable")
                        })
                        .unwrap_or_else(|| {
                            serde_json::json!({
                                "worked": false,
                                "job_kind": "elaboration",
                                "message": "No queued verifier job was available."
                            })
                        }),
                    WorkerJobKind::Audit => application
                        .work_one_audit_job(&options.worker_id, options.lease_seconds)?
                        .map(|outcome| {
                            to_value(outcome).expect("audit work outcome is serializable")
                        })
                        .unwrap_or_else(|| {
                            serde_json::json!({
                                "worked": false,
                                "job_kind": "audit",
                                "message": "No queued audit job was available."
                            })
                        }),
                };
                Ok(CliOutcome {
                    value,
                    success: true,
                })
            }
            Command::Serve => {
                crate::mcp::serve_stdio(config)?;
                Ok(CliOutcome {
                    value: Value::Null,
                    success: true,
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
            Command::Edge(options) => execute_edge(&config, options),
            Command::Graph(options) => execute_graph(&config, options),
            Command::Research(options) => execute_research(&config, options),
        }
    }
}

fn execute_verify(config: &ResolvedConfig, options: VerifyOptions) -> Result<CliOutcome, AppError> {
    let mut application = Application::open(config)?;
    let value = match options.action {
        VerifyAction::Check(options) => {
            let request = VerifierJobRequest {
                schema_version: "verifier_request/1".to_owned(),
                environment_hash: options.environment_hash,
                module_artifact_hash: options.module_artifact_hash,
                declaration_name: options.declaration_name,
            };
            to_value(application.enqueue_verifier_job(
                &request,
                options.priority,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("verifier enqueue outcome is serializable")
        }
        VerifyAction::Status(options) => to_value(application.get_verifier_job(&options.job_id)?)
            .expect("verifier job is serializable"),
        VerifyAction::List(options) => to_value(application.list_verifier_jobs(options.limit)?)
            .expect("verifier job list is serializable"),
        VerifyAction::PromoteDiagnostic(options) => {
            to_value(application.promote_verifier_diagnostic(
                &options.formalization_object_id,
                &options.formalization_version_hash,
                &options.job_id,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("evidence promotion outcome is serializable")
        }
        VerifyAction::Evidence(options) => {
            to_value(application.get_evidence(&options.evidence_id)?)
                .expect("evidence is serializable")
        }
        VerifyAction::EvidenceList(options) => to_value(application.list_evidence(options.limit)?)
            .expect("evidence list is serializable"),
        VerifyAction::Audit(options) => to_value(application.enqueue_audit_job(
            &crate::domain::schemas::ExactVersionReference {
                object_id: options.formalization_object_id,
                version_hash: options.formalization_version_hash,
            },
            &options.diagnostic_evidence_id,
            options.priority,
            &options.mutation.actor,
            &options.mutation.idempotency_key,
            options.mutation.dry_run,
        )?)
        .expect("audit enqueue outcome is serializable"),
        VerifyAction::AuditStatus(options) => to_value(application.get_audit_job(&options.job_id)?)
            .expect("audit job is serializable"),
        VerifyAction::AuditList(options) => to_value(application.list_audit_jobs(options.limit)?)
            .expect("audit job list is serializable"),
        VerifyAction::PromoteAudit(options) => to_value(application.promote_audit_evidence(
            &options.formalization_object_id,
            &options.formalization_version_hash,
            &options.job_id,
            &options.mutation.actor,
            &options.mutation.idempotency_key,
            options.mutation.dry_run,
        )?)
        .expect("audit evidence promotion is serializable"),
    };
    Ok(CliOutcome {
        value,
        success: true,
    })
}

fn execute_artifact(
    config: &ResolvedConfig,
    options: ArtifactOptions,
) -> Result<CliOutcome, AppError> {
    let mut application = Application::open(config)?;
    let value = match options.action {
        ArtifactAction::Ingest(options) => {
            let metadata: ArtifactMetadata = serde_json::from_str(&options.metadata_json)
                .map_err(|error| {
                    AppError::new(
                        "MCL_ARTIFACT_JSON_INVALID",
                        format!("artifact metadata JSON is invalid: {error}"),
                        false,
                        "Supply one complete object matching `schemas/artifact/artifact-metadata-1.schema.json`.",
                    )
                })?;
            let input = if options.input_file.is_absolute() {
                options.input_file
            } else {
                config.root.join(options.input_file)
            };
            if fs::symlink_metadata(&input)
                .map_err(|error| AppError::io("inspect artifact input", error))?
                .file_type()
                .is_symlink()
            {
                return Err(AppError::new(
                    "MCL_ARTIFACT_INPUT_UNSAFE",
                    "artifact input may not be a symbolic link",
                    false,
                    "Use a regular file contained by the instance root.",
                ));
            }
            let input = input
                .canonicalize()
                .map_err(|error| AppError::io("canonicalize artifact input", error))?;
            if !input.starts_with(&config.root) || !input.is_file() {
                return Err(AppError::new(
                    "MCL_ARTIFACT_INPUT_UNSAFE",
                    format!(
                        "artifact input {} is not a regular file contained by the instance root",
                        input.display()
                    ),
                    false,
                    "Copy the intended file under the instance root and retry.",
                ));
            }
            let input_size = fs::metadata(&input)
                .map_err(|error| AppError::io("inspect artifact input size", error))?
                .len();
            metadata.validate(input_size)?;
            let bytes =
                fs::read(&input).map_err(|error| AppError::io("read artifact input", error))?;
            to_value(application.ingest_artifact(
                &bytes,
                &metadata,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("artifact ingest outcome is serializable")
        }
        ArtifactAction::Get(options) => to_value(application.get_artifact(&options.artifact_hash)?)
            .expect("artifact snapshot is serializable"),
        ArtifactAction::List(options) => to_value(application.list_artifacts(options.limit)?)
            .expect("artifact list is serializable"),
        ArtifactAction::Verify(options) => {
            to_value(application.verify_artifact(&options.artifact_hash)?)
                .expect("artifact verification report is serializable")
        }
    };
    Ok(CliOutcome {
        value,
        success: true,
    })
}

fn execute_environment(
    config: &ResolvedConfig,
    options: EnvironmentOptions,
) -> Result<CliOutcome, AppError> {
    let mut application = Application::open(config)?;
    let value = match options.action {
        EnvironmentAction::Register(options) => {
            let manifest: EnvironmentManifest = serde_json::from_str(&options.manifest_json)
                .map_err(|error| {
                    AppError::new(
                        "MCL_ENVIRONMENT_JSON_INVALID",
                        format!("environment manifest JSON is invalid: {error}"),
                        false,
                        "Supply one complete manifest matching `schemas/environment/environment-1.schema.json`.",
                    )
                })?;
            to_value(application.register_environment(
                &manifest,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("environment registration outcome is serializable")
        }
        EnvironmentAction::Get(options) => {
            to_value(application.get_environment(&options.environment_hash)?)
                .expect("environment snapshot is serializable")
        }
        EnvironmentAction::List(options) => to_value(application.list_environments(options.limit)?)
            .expect("environment list is serializable"),
    };
    Ok(CliOutcome {
        value,
        success: true,
    })
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

fn execute_edge(config: &ResolvedConfig, options: EdgeOptions) -> Result<CliOutcome, AppError> {
    let mut application = Application::open(config)?;
    let value = match options.action {
        EdgeAction::Create(options) => {
            let draft = EdgeDraft {
                kind: EdgeKind::from_str(&options.kind)?,
                source_object_id: options.source_object_id,
                source_version_hash: options.source_version_hash,
                target_object_id: options.target_object_id,
                target_version_hash: options.target_version_hash,
                payload: parse_json(&options.payload_json, "edge payload")?,
            };
            to_value(application.create_edge(
                &draft,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("edge mutation outcome is serializable")
        }
        EdgeAction::Get(options) => to_value(application.get_edge(&options.edge_id)?)
            .expect("edge snapshot is serializable"),
    };
    Ok(CliOutcome {
        value,
        success: true,
    })
}

fn execute_graph(config: &ResolvedConfig, options: GraphOptions) -> Result<CliOutcome, AppError> {
    let application = Application::open(config)?;
    let direction = match options.direction.as_str() {
        "outgoing" => TraversalDirection::Outgoing,
        "incoming" => TraversalDirection::Incoming,
        "both" => TraversalDirection::Both,
        value => {
            return Err(AppError::new(
                "MCL_GRAPH_DIRECTION_UNKNOWN",
                format!("unknown graph direction `{value}`"),
                false,
                "Use `outgoing`, `incoming`, or `both`.",
            ));
        }
    };
    let edge_kinds = options
        .edge_kinds
        .iter()
        .map(|kind| EdgeKind::from_str(kind))
        .collect::<Result<Vec<_>, _>>()?;
    let request = GraphTraversalRequest {
        root_object_id: options.root_object_id,
        root_version_hash: options.root_version_hash,
        direction,
        edge_kinds,
        max_depth: options.max_depth,
        limit: options.limit,
    };
    Ok(CliOutcome {
        value: to_value(application.traverse_graph(&request)?)
            .expect("graph traversal is serializable"),
        success: true,
    })
}

fn execute_research(
    config: &ResolvedConfig,
    options: ResearchOptions,
) -> Result<CliOutcome, AppError> {
    let mut application = Application::open(config)?;
    let value = match options.action {
        ResearchAction::Start(options) => {
            let kind = RunKind::from_str(&options.kind)?;
            let budget = parse_json(&options.budget_json, "run budget")?;
            to_value(application.create_run(
                kind,
                &budget,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("run mutation outcome is serializable")
        }
        ResearchAction::Get(options) => {
            to_value(application.get_run(&options.run_id)?).expect("run snapshot is serializable")
        }
        ResearchAction::Events(options) => to_value(application.list_run_events(&options.run_id)?)
            .expect("run events are serializable"),
        ResearchAction::Submit(options) => {
            let draft = RunEventDraft {
                kind: RunEventKind::from_str(&options.kind)?,
                payload: parse_json(&options.payload_json, "run event payload")?,
            };
            to_value(application.append_run_event(
                &options.run_id,
                &options.expected_head,
                &draft,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("run event mutation outcome is serializable")
        }
        ResearchAction::Verify(options) => to_value(application.verify_run_chain(&options.run_id)?)
            .expect("run chain report is serializable"),
    };
    Ok(CliOutcome {
        value,
        success: true,
    })
}

fn parse_json(input: &str, label: &str) -> Result<Value, AppError> {
    serde_json::from_str(input).map_err(|error| {
        AppError::new(
            "MCL_CLI_JSON_INVALID",
            format!("{label} JSON is invalid: {error}"),
            false,
            "Supply one complete JSON value through the corresponding JSON option.",
        )
    })
}
