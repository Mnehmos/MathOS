use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::str::FromStr;

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::{Value, to_value};

use crate::app::{Application, PedagogyPathMode, root_exists};
use crate::config::ResolvedConfig;
use crate::domain::schemas::{
    ExactVersionReference, LEARNING_UNIT_SCHEMA_VERSION, LearningUnitReviewState,
    LearningUnitTrainingStatus,
};
use crate::domain::{
    ArtifactMetadata, CounterexampleRepairRequest, EdgeDraft, EdgeKind, EnvironmentManifest,
    GraphTraversalRequest, PublicationOutcome, RecordDraft, RecordKind, RecordSnapshot,
    RunEventDraft, RunEventKind, RunKind, TraversalDirection, VerifierJobRequest,
};
use crate::error::AppError;

const MAX_PUBLICATION_CANDIDATE_DOCUMENT_BYTES: usize = 1_048_576;

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
    /// Propose, validate, review, link, or traverse canonical learning units.
    Pedagogy(PedagogyOptions),
    /// Build or independently verify a portable receipt-bound release directory.
    Release(ReleaseOptions),
    /// Search the current canonical record heads through SQLite FTS5.
    Search(SearchOptions),
    /// Create or retrieve exact version-bound graph edges.
    Edge(EdgeOptions),
    /// Traverse the version-bound graph with explicit typed bounds.
    Graph(GraphOptions),
    /// Start, inspect, append to, and verify non-authoritative research runs.
    Research(ResearchOptions),
    /// Package an exact refutation witness and atomically create an immutable repaired claim.
    Counterexample(CounterexampleOptions),
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
    ReviewFidelity(ReviewFidelityOptions),
    FidelityStatus(FidelityStatusOptions),
    /// Derive the research status of one exact claim version.
    ClaimStatus(ClaimStatusOptions),
    PreparePublication(VerifyPreparePublicationOptions),
    ValidatePublicationCandidate(VerifyValidatePublicationCandidateOptions),
    StagePublicationCandidate(VerifyStagePublicationCandidateOptions),
    IngestPublication(VerifyIngestPublicationOptions),
    PromotePublicationAuthority(VerifyPromotePublicationAuthorityOptions),
    /// Stage one exact protected Comparator run, plan, release, policy, and attestation closure.
    StageComparatorAuthority(VerifyStageComparatorAuthorityOptions),
    /// Replay a staged Comparator closure and create a non-authoritative attestation receipt.
    IngestComparatorAuthority(VerifyIngestComparatorAuthorityOptions),
    /// Promote one fully replayed Comparator receipt through the closed authority gate.
    PromoteComparatorAuthority(VerifyPromoteComparatorAuthorityOptions),
    /// Replay live current/stale status for immutable Comparator authority evidence.
    ComparatorAuthorityStatus(VerifyComparatorAuthorityStatusOptions),
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
struct ReviewFidelityOptions {
    #[arg(
        long,
        help = "Closed fidelity_review_request/1 or fidelity_review_request/2 JSON object"
    )]
    request_json: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct FidelityStatusOptions {
    #[arg(long)]
    formalization_object_id: String,

    #[arg(long)]
    formalization_version_hash: String,
}

#[derive(Debug, Args)]
struct ClaimStatusOptions {
    #[arg(long)]
    claim_object_id: String,

    #[arg(long)]
    claim_version_hash: String,
}

#[derive(Debug, Args)]
struct CounterexampleOptions {
    #[command(subcommand)]
    action: CounterexampleAction,
}

#[derive(Debug, Subcommand)]
enum CounterexampleAction {
    /// Build a canonical counterexample package and atomically register its new claim and repair edge.
    Repair(CounterexampleRepairOptions),
    /// Revalidate and retrieve a registered package with its new claim and controlled repair edge.
    Get(CounterexampleGetOptions),
}

#[derive(Debug, Args)]
struct CounterexampleRepairOptions {
    #[arg(long, help = "Closed counterexample_repair_request/1 JSON object")]
    request_json: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct CounterexampleGetOptions {
    #[arg(long)]
    artifact_hash: String,
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

#[derive(Debug, Args)]
struct VerifyPreparePublicationOptions {
    #[arg(long)]
    formalization_object_id: String,

    #[arg(long)]
    formalization_version_hash: String,

    #[arg(long)]
    outcome: String,

    #[arg(long)]
    diagnostic_evidence_id: String,

    #[arg(long)]
    proof_closure_evidence_id: String,

    #[arg(long)]
    axiom_audit_evidence_id: String,

    #[arg(long)]
    source_commit_sha: String,

    #[arg(long)]
    source_tree_sha: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct VerifyValidatePublicationCandidateOptions {
    #[arg(long)]
    report_file: PathBuf,

    #[arg(long)]
    retained_closure_file: PathBuf,

    #[arg(long)]
    retained_root: PathBuf,
}

#[derive(Debug, Args)]
struct VerifyStagePublicationCandidateOptions {
    #[arg(long)]
    report_file: PathBuf,

    #[arg(long)]
    retained_closure_file: PathBuf,

    #[arg(long)]
    retained_root: PathBuf,

    #[arg(long)]
    attestation_bundle_file: PathBuf,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct VerifyIngestPublicationOptions {
    #[arg(long)]
    report_artifact_hash: String,

    #[arg(long)]
    attestation_bundle_artifact_hash: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct VerifyPromotePublicationAuthorityOptions {
    #[arg(long)]
    publication_receipt_hash: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct VerifyStageComparatorAuthorityOptions {
    #[arg(long)]
    run_dir: PathBuf,

    #[arg(long)]
    expected_report_hash: String,

    #[arg(long)]
    expected_package_verification_hash: String,

    #[arg(long)]
    plan_file: PathBuf,

    #[arg(long)]
    release_dir: PathBuf,

    #[arg(long)]
    expected_release_manifest_hash: String,

    #[arg(long)]
    attestation_bundle_file: PathBuf,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct VerifyIngestComparatorAuthorityOptions {
    #[arg(long)]
    report_artifact_hash: String,

    #[arg(long)]
    attestation_bundle_artifact_hash: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct VerifyPromoteComparatorAuthorityOptions {
    #[arg(long)]
    comparator_receipt_hash: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct VerifyComparatorAuthorityStatusOptions {
    #[arg(long)]
    evidence_id: String,
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
struct PedagogyOptions {
    #[command(subcommand)]
    action: PedagogyAction,
}

#[derive(Debug, Subcommand)]
enum PedagogyAction {
    /// Validate and propose a draft canonical learning unit.
    Propose(RecordCreateOptions),
    /// Append a revised draft using compare-and-swap.
    Version(RecordVersionOptions),
    /// Retrieve the current head or one exact immutable version.
    Get(RecordGetOptions),
    /// Revalidate exact references, content policy, and prerequisite links.
    Validate(PedagogyValidateOptions),
    /// Record an actor-bound reviewed or rejected immutable version.
    Review(PedagogyReviewOptions),
    /// Create one typed exact-version pedagogy edge.
    Link(EdgeCreateOptions),
    /// Build a deterministic bounded prerequisite or recommended path.
    Path(PedagogyPathOptions),
}

#[derive(Debug, Args)]
struct PedagogyValidateOptions {
    #[arg(long)]
    object_id: String,

    #[arg(long)]
    version_hash: String,
}

#[derive(Debug, Args)]
struct PedagogyReviewOptions {
    #[arg(long)]
    object_id: String,

    #[arg(long)]
    expected_head: String,

    #[arg(long)]
    decision: String,

    #[arg(long)]
    training_status: String,

    #[arg(long)]
    notes_json: String,

    #[command(flatten)]
    mutation: MutationOptions,
}

#[derive(Debug, Args)]
struct PedagogyPathOptions {
    #[arg(long)]
    root_object_id: String,

    #[arg(long)]
    root_version_hash: String,

    #[arg(long, default_value = "prerequisites")]
    mode: String,

    #[arg(long)]
    include_soft: bool,

    #[arg(long, default_value_t = 8)]
    max_depth: u32,

    #[arg(long, default_value_t = 100)]
    limit: usize,
}

#[derive(Debug, Args)]
struct ReleaseOptions {
    #[command(subcommand)]
    action: ReleaseAction,
}

#[derive(Debug, Subcommand)]
enum ReleaseAction {
    /// Build one new deterministic directory from an authoritative receipt and reviewed path.
    Build(ReleaseBuildOptions),
    /// Verify and replay a copied bundle without opening the MathOS database.
    Verify(ReleaseVerifyOptions),
    /// Project a frozen release into canonical MathCorpus and MCIP artifacts offline.
    Export(ReleaseExportOptions),
    /// Verify a copied MathCorpus/MCIP export by deterministic offline reprojection.
    VerifyExport(ReleaseVerifyExportOptions),
    /// Project a leakage-declared frozen-release cohort into RL/evaluation tasks offline.
    ExportRl(ReleaseExportRlOptions),
    /// Verify an RL/evaluation export by deterministic offline cohort reprojection.
    VerifyRlExport(ReleaseVerifyRlExportOptions),
    /// Project one frozen release into an exact five-file Comparator-ready package.
    ExportComparator(ReleaseExportComparatorOptions),
    /// Verify and deterministically reproject a Comparator-ready package offline.
    VerifyComparatorPackage(ReleaseVerifyComparatorPackageOptions),
    /// Verify a protected official Comparator execution bundle without opening the database.
    VerifyComparatorRun(ReleaseVerifyComparatorRunOptions),
}

#[derive(Debug, Args)]
struct ReleaseBuildOptions {
    #[arg(long)]
    publication_receipt_hash: String,

    #[arg(long)]
    pedagogy_root_object_id: String,

    #[arg(long)]
    pedagogy_root_version_hash: String,

    #[arg(long, default_value = "prerequisites")]
    mode: String,

    #[arg(long)]
    include_soft: bool,

    #[arg(long, default_value_t = 8)]
    max_depth: u32,

    #[arg(long, default_value_t = 100)]
    limit: usize,

    #[arg(long, default_value = "private")]
    profile: String,

    #[arg(long)]
    output_dir: PathBuf,

    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ReleaseVerifyOptions {
    #[arg(long)]
    bundle_dir: PathBuf,

    #[arg(long)]
    expected_manifest_hash: String,
}

#[derive(Debug, Args)]
struct ReleaseExportOptions {
    #[arg(long)]
    bundle_dir: PathBuf,

    #[arg(long)]
    expected_manifest_hash: String,

    #[arg(long)]
    packet_id: String,

    #[arg(long)]
    domain: String,

    #[arg(long)]
    level: String,

    #[arg(long)]
    difficulty_bin: String,

    #[arg(long)]
    output_dir: PathBuf,

    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ReleaseVerifyExportOptions {
    #[arg(long)]
    export_dir: PathBuf,

    #[arg(long)]
    expected_manifest_hash: String,

    #[arg(long)]
    source_bundle_dir: PathBuf,
}

#[derive(Debug, Args)]
struct ReleaseExportRlOptions {
    #[arg(long)]
    plan: PathBuf,

    #[arg(long)]
    source_root: PathBuf,

    #[arg(long)]
    output_dir: PathBuf,

    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ReleaseVerifyRlExportOptions {
    #[arg(long)]
    export_dir: PathBuf,

    #[arg(long)]
    expected_manifest_hash: String,

    #[arg(long)]
    plan: PathBuf,

    #[arg(long)]
    source_root: PathBuf,
}

#[derive(Debug, Args)]
struct ReleaseExportComparatorOptions {
    #[arg(long)]
    plan: PathBuf,

    #[arg(long)]
    bundle_dir: PathBuf,

    #[arg(long)]
    expected_release_manifest_hash: String,

    #[arg(long)]
    output_dir: PathBuf,

    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ReleaseVerifyComparatorPackageOptions {
    #[arg(long)]
    package_dir: PathBuf,

    #[arg(long)]
    expected_verification_hash: String,

    #[arg(long)]
    plan: PathBuf,

    #[arg(long)]
    bundle_dir: PathBuf,

    #[arg(long)]
    expected_release_manifest_hash: String,
}

#[derive(Debug, Args)]
struct ReleaseVerifyComparatorRunOptions {
    #[arg(long)]
    run_dir: PathBuf,

    #[arg(long)]
    expected_report_hash: String,

    #[arg(long)]
    expected_package_verification_hash: String,
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
        if let Command::Release(ReleaseOptions { action }) = &self.command {
            let value = match action {
                ReleaseAction::Verify(options) => Some(
                    to_value(Application::verify_release(
                        &options.bundle_dir,
                        &options.expected_manifest_hash,
                    )?)
                    .expect("release verification report is serializable"),
                ),
                ReleaseAction::Export(options) => Some(
                    to_value(crate::corpus_export::export_release(
                        crate::corpus_export::CorpusExportRequest {
                            bundle_dir: &options.bundle_dir,
                            expected_manifest_hash: &options.expected_manifest_hash,
                            packet_id: &options.packet_id,
                            domain: crate::domain::MathCorpusDomain::from_str(&options.domain)?,
                            level: crate::domain::MathCorpusLevel::from_str(&options.level)?,
                            difficulty_bin: crate::domain::MathCorpusDifficultyBin::from_str(
                                &options.difficulty_bin,
                            )?,
                            output_dir: &options.output_dir,
                            dry_run: options.dry_run,
                        },
                    )?)
                    .expect("corpus export outcome is serializable"),
                ),
                ReleaseAction::VerifyExport(options) => Some(
                    to_value(crate::corpus_export::verify_export(
                        &options.export_dir,
                        &options.expected_manifest_hash,
                        &options.source_bundle_dir,
                    )?)
                    .expect("corpus export verification report is serializable"),
                ),
                ReleaseAction::ExportRl(options) => Some(
                    to_value(crate::rl_export::export_rl(
                        crate::rl_export::RlExportRequest {
                            plan_path: &options.plan,
                            source_root: &options.source_root,
                            output_dir: &options.output_dir,
                            dry_run: options.dry_run,
                        },
                    )?)
                    .expect("RL export outcome is serializable"),
                ),
                ReleaseAction::VerifyRlExport(options) => Some(
                    to_value(crate::rl_export::verify_rl_export(
                        &options.export_dir,
                        &options.expected_manifest_hash,
                        &options.plan,
                        &options.source_root,
                    )?)
                    .expect("RL export verification report is serializable"),
                ),
                ReleaseAction::ExportComparator(options) => Some(
                    to_value(crate::comparator_export::export_comparator(
                        crate::comparator_export::ComparatorExportRequest {
                            plan_path: &options.plan,
                            bundle_dir: &options.bundle_dir,
                            expected_release_manifest_hash: &options.expected_release_manifest_hash,
                            output_dir: &options.output_dir,
                            dry_run: options.dry_run,
                        },
                    )?)
                    .expect("Comparator export outcome is serializable"),
                ),
                ReleaseAction::VerifyComparatorPackage(options) => Some(
                    to_value(crate::comparator_export::verify_comparator_package(
                        &options.package_dir,
                        &options.expected_verification_hash,
                        &options.plan,
                        &options.bundle_dir,
                        &options.expected_release_manifest_hash,
                    )?)
                    .expect("Comparator package verification report is serializable"),
                ),
                ReleaseAction::VerifyComparatorRun(options) => Some(
                    to_value(crate::comparator_run::verify_comparator_run(
                        crate::comparator_run::ComparatorRunVerificationRequest {
                            run_dir: &options.run_dir,
                            expected_report_hash: &options.expected_report_hash,
                            expected_package_verification_hash: &options
                                .expected_package_verification_hash,
                        },
                    )?)
                    .expect("Comparator run verification outcome is serializable"),
                ),
                ReleaseAction::Build(_) => None,
            };
            if let Some(value) = value {
                return Ok(CliOutcome {
                    value,
                    success: true,
                });
            }
        }
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
            Command::Pedagogy(options) => execute_pedagogy(&config, options),
            Command::Release(options) => execute_release(&config, options),
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
            Command::Counterexample(options) => execute_counterexample(&config, options),
        }
    }
}

fn execute_counterexample(
    config: &ResolvedConfig,
    options: CounterexampleOptions,
) -> Result<CliOutcome, AppError> {
    let mut application = Application::open(config)?;
    let value = match options.action {
        CounterexampleAction::Repair(options) => {
            let request: CounterexampleRepairRequest = serde_json::from_str(&options.request_json)
                .map_err(|error| {
                    AppError::new(
                        "MCL_COUNTEREXAMPLE_JSON_INVALID",
                        error.to_string(),
                        false,
                        "Supply one closed counterexample_repair_request/1 JSON object.",
                    )
                })?;
            to_value(application.repair_disproved_claim(
                &request,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("counterexample repair outcome is serializable")
        }
        CounterexampleAction::Get(options) => {
            to_value(application.get_counterexample_package(&options.artifact_hash)?)
                .expect("counterexample package snapshot is serializable")
        }
    };
    Ok(CliOutcome {
        value,
        success: true,
    })
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
        VerifyAction::ReviewFidelity(options) => {
            let request: crate::domain::VersionedFidelityReviewRequest =
                serde_json::from_str(&options.request_json).map_err(|error| {
                    AppError::new(
                        "MCL_FIDELITY_JSON_INVALID",
                        error.to_string(),
                        false,
                        "Supply one closed fidelity_review_request/1 or fidelity_review_request/2 JSON object.",
                    )
                })?;
            to_value(application.review_fidelity(
                &request,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("fidelity review outcome is serializable")
        }
        VerifyAction::FidelityStatus(options) => to_value(application.fidelity_status(
            &crate::domain::schemas::ExactVersionReference {
                object_id: options.formalization_object_id,
                version_hash: options.formalization_version_hash,
            },
        )?)
        .expect("fidelity status is serializable"),
        VerifyAction::ClaimStatus(options) => to_value(application.claim_research_status(
            &crate::domain::schemas::ExactVersionReference {
                object_id: options.claim_object_id,
                version_hash: options.claim_version_hash,
            },
        )?)
        .expect("claim research status is serializable"),
        VerifyAction::PreparePublication(options) => {
            let subject = crate::domain::schemas::ExactVersionReference {
                object_id: options.formalization_object_id,
                version_hash: options.formalization_version_hash,
            };
            let outcome = PublicationOutcome::from_str(&options.outcome)?;
            to_value(application.prepare_publication_request(
                &subject,
                outcome,
                &options.diagnostic_evidence_id,
                &options.proof_closure_evidence_id,
                &options.axiom_audit_evidence_id,
                &options.source_commit_sha,
                &options.source_tree_sha,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("publication request preparation is serializable")
        }
        VerifyAction::ValidatePublicationCandidate(options) => {
            let report_bytes = read_publication_candidate_file(
                config,
                &options.report_file,
                "publication report",
            )?;
            let retained_closure_bytes = read_publication_candidate_file(
                config,
                &options.retained_closure_file,
                "publication retained closure",
            )?;
            let retained_root = resolve_publication_retained_root(config, &options.retained_root)?;
            to_value(application.validate_publication_candidate(
                &report_bytes,
                &retained_closure_bytes,
                &retained_root,
            )?)
            .expect("publication candidate validation outcome is serializable")
        }
        VerifyAction::StagePublicationCandidate(options) => {
            let report_bytes = read_publication_candidate_file(
                config,
                &options.report_file,
                "publication report",
            )?;
            let retained_closure_bytes = read_publication_candidate_file(
                config,
                &options.retained_closure_file,
                "publication retained closure",
            )?;
            let attestation_bundle_bytes = read_publication_candidate_file(
                config,
                &options.attestation_bundle_file,
                "publication attestation bundle",
            )?;
            let retained_root = resolve_publication_retained_root(config, &options.retained_root)?;
            to_value(application.stage_publication_candidate(
                &report_bytes,
                &retained_closure_bytes,
                &retained_root,
                &attestation_bundle_bytes,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("publication stage outcome is serializable")
        }
        VerifyAction::IngestPublication(options) => to_value(application.ingest_publication(
            &options.report_artifact_hash,
            &options.attestation_bundle_artifact_hash,
            &options.mutation.actor,
            &options.mutation.idempotency_key,
            options.mutation.dry_run,
        )?)
        .expect("publication ingestion outcome is serializable"),
        VerifyAction::PromotePublicationAuthority(options) => {
            to_value(application.promote_publication_authority(
                &options.publication_receipt_hash,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("publication authority promotion outcome is serializable")
        }
        VerifyAction::StageComparatorAuthority(options) => {
            let run_dir = resolve_comparator_authority_input(
                config,
                &options.run_dir,
                true,
                "Comparator run directory",
            )?;
            let plan_file = resolve_comparator_authority_input(
                config,
                &options.plan_file,
                false,
                "Comparator plan",
            )?;
            let release_dir = resolve_comparator_authority_input(
                config,
                &options.release_dir,
                true,
                "Comparator source release",
            )?;
            let bundle_file = resolve_comparator_authority_input(
                config,
                &options.attestation_bundle_file,
                false,
                "Comparator attestation bundle",
            )?;
            let bundle_bytes = fs::read(&bundle_file)
                .map_err(|error| AppError::io("read Comparator attestation bundle", error))?;
            to_value(application.stage_comparator_authority(
                &run_dir,
                &options.expected_report_hash,
                &options.expected_package_verification_hash,
                &plan_file,
                &release_dir,
                &options.expected_release_manifest_hash,
                &bundle_bytes,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("Comparator authority stage outcome is serializable")
        }
        VerifyAction::IngestComparatorAuthority(options) => {
            to_value(application.ingest_comparator_authority(
                &options.report_artifact_hash,
                &options.attestation_bundle_artifact_hash,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("Comparator authority ingestion outcome is serializable")
        }
        VerifyAction::PromoteComparatorAuthority(options) => {
            to_value(application.promote_comparator_authority(
                &options.comparator_receipt_hash,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("Comparator authority promotion outcome is serializable")
        }
        VerifyAction::ComparatorAuthorityStatus(options) => {
            to_value(application.comparator_authority_status(&options.evidence_id)?)
                .expect("Comparator authority status is serializable")
        }
    };
    Ok(CliOutcome {
        value,
        success: true,
    })
}

fn resolve_publication_retained_root(
    config: &ResolvedConfig,
    requested: &PathBuf,
) -> Result<PathBuf, AppError> {
    let root = if requested.is_absolute() {
        requested.clone()
    } else {
        config.root.join(requested)
    };
    let metadata = fs::symlink_metadata(&root)
        .map_err(|error| AppError::io("inspect publication retained root", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::new(
            "MCL_PUBLICATION_RETAINED_ROOT_UNSAFE",
            "publication retained root must be a real directory, not a file or symbolic link",
            false,
            "Use the workflow output directory contained by the instance root.",
        ));
    }
    let root = root
        .canonicalize()
        .map_err(|error| AppError::io("canonicalize publication retained root", error))?;
    if !root.starts_with(&config.root) {
        return Err(AppError::new(
            "MCL_PUBLICATION_RETAINED_ROOT_UNSAFE",
            format!(
                "publication retained root {} escapes the instance root",
                root.display()
            ),
            false,
            "Place retained workflow output under the initialized instance root.",
        ));
    }
    Ok(root)
}

fn resolve_comparator_authority_input(
    config: &ResolvedConfig,
    requested: &PathBuf,
    directory: bool,
    label: &str,
) -> Result<PathBuf, AppError> {
    let path = if requested.is_absolute() {
        requested.clone()
    } else {
        config.root.join(requested)
    };
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| AppError::io("inspect Comparator authority input", error))?;
    if metadata.file_type().is_symlink()
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err(AppError::new(
            "MCL_COMPARATOR_AUTHORITY_PATH_UNSAFE",
            format!(
                "{label} must be a real {}",
                if directory { "directory" } else { "file" }
            ),
            false,
            "Place exact protected inputs under the initialized instance root without links.",
        ));
    }
    let path = path
        .canonicalize()
        .map_err(|error| AppError::io("canonicalize Comparator authority input", error))?;
    if !path.starts_with(&config.root) {
        return Err(AppError::new(
            "MCL_COMPARATOR_AUTHORITY_PATH_UNSAFE",
            format!("{label} {} escapes the instance root", path.display()),
            false,
            "Place exact protected inputs under the initialized instance root.",
        ));
    }
    Ok(path)
}

fn read_publication_candidate_file(
    config: &ResolvedConfig,
    requested: &PathBuf,
    label: &str,
) -> Result<Vec<u8>, AppError> {
    let input = if requested.is_absolute() {
        requested.clone()
    } else {
        config.root.join(requested)
    };
    if fs::symlink_metadata(&input)
        .map_err(|error| AppError::io(&format!("inspect {label} input"), error))?
        .file_type()
        .is_symlink()
    {
        return Err(AppError::new(
            "MCL_PUBLICATION_CANDIDATE_INPUT_UNSAFE",
            format!("{label} input may not be a symbolic link"),
            false,
            "Use a regular file contained by the instance root.",
        ));
    }
    let input = input
        .canonicalize()
        .map_err(|error| AppError::io(&format!("canonicalize {label} input"), error))?;
    if !input.starts_with(&config.root) || !input.is_file() {
        return Err(AppError::new(
            "MCL_PUBLICATION_CANDIDATE_INPUT_UNSAFE",
            format!(
                "{label} input {} is not a regular file contained by the instance root",
                input.display()
            ),
            false,
            "Copy the exact canonical file under the instance root and retry.",
        ));
    }
    let file = fs::File::open(&input)
        .map_err(|error| AppError::io(&format!("open {label} input"), error))?;
    if !file
        .metadata()
        .map_err(|error| AppError::io(&format!("inspect {label} input size"), error))?
        .is_file()
    {
        return Err(AppError::new(
            "MCL_PUBLICATION_CANDIDATE_INPUT_UNSAFE",
            format!("{label} input is not a regular file"),
            false,
            "Use a regular file contained by the instance root.",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_PUBLICATION_CANDIDATE_DOCUMENT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::io(&format!("read {label} input"), error))?;
    if bytes.len() > MAX_PUBLICATION_CANDIDATE_DOCUMENT_BYTES {
        return Err(AppError::new(
            "MCL_PUBLICATION_CANDIDATE_INPUT_TOO_LARGE",
            format!(
                "{label} input exceeds the {} byte limit",
                MAX_PUBLICATION_CANDIDATE_DOCUMENT_BYTES
            ),
            false,
            "Reduce the document to the closed publication contract and retry.",
        ));
    }
    Ok(bytes)
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
            RecordKind::LearningUnit => LEARNING_UNIT_SCHEMA_VERSION,
        }
        .to_owned(),
        payload,
        searchable_text,
    })
}

fn execute_pedagogy(
    config: &ResolvedConfig,
    options: PedagogyOptions,
) -> Result<CliOutcome, AppError> {
    let mut application = Application::open(config)?;
    let value = match options.action {
        PedagogyAction::Propose(options) => {
            let draft = record_draft(
                RecordKind::LearningUnit,
                &options.payload_json,
                options.searchable_text,
            )?;
            to_value(application.create_record(
                &draft,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("learning-unit mutation outcome is serializable")
        }
        PedagogyAction::Version(options) => {
            let draft = record_draft(
                RecordKind::LearningUnit,
                &options.payload_json,
                options.searchable_text,
            )?;
            to_value(application.version_record(
                &options.object_id,
                &options.expected_head,
                &draft,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("learning-unit mutation outcome is serializable")
        }
        PedagogyAction::Get(options) => {
            let record =
                application.get_record(&options.object_id, options.version_hash.as_deref())?;
            require_record_kind(&record, RecordKind::LearningUnit)?;
            to_value(record).expect("learning unit is serializable")
        }
        PedagogyAction::Validate(options) => {
            to_value(application.validate_learning_unit(&options.object_id, &options.version_hash)?)
                .expect("learning-unit validation is serializable")
        }
        PedagogyAction::Review(options) => {
            let notes: Vec<String> =
                serde_json::from_value(parse_json(&options.notes_json, "pedagogy review notes")?)
                    .map_err(|error| {
                    AppError::new(
                        "MCL_CLI_JSON_INVALID",
                        format!("pedagogy review notes must be a JSON string array: {error}"),
                        false,
                        "Supply review notes through `--notes-json '[\"rationale\"]'`.",
                    )
                })?;
            to_value(application.review_learning_unit(
                &options.object_id,
                &options.expected_head,
                parse_learning_unit_review_state(&options.decision)?,
                parse_learning_unit_training_status(&options.training_status)?,
                notes,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("learning-unit review outcome is serializable")
        }
        PedagogyAction::Link(options) => {
            let draft = EdgeDraft {
                kind: EdgeKind::from_str(&options.kind)?,
                source_object_id: options.source_object_id,
                source_version_hash: options.source_version_hash,
                target_object_id: options.target_object_id,
                target_version_hash: options.target_version_hash,
                payload: parse_json(&options.payload_json, "pedagogy link payload")?,
            };
            to_value(application.create_pedagogy_link(
                &draft,
                &options.mutation.actor,
                &options.mutation.idempotency_key,
                options.mutation.dry_run,
            )?)
            .expect("pedagogy link outcome is serializable")
        }
        PedagogyAction::Path(options) => to_value(application.pedagogy_path(
            &ExactVersionReference {
                object_id: options.root_object_id,
                version_hash: options.root_version_hash,
            },
            parse_pedagogy_path_mode(&options.mode)?,
            options.include_soft,
            options.max_depth,
            options.limit,
        )?)
        .expect("pedagogy path is serializable"),
    };
    Ok(CliOutcome {
        value,
        success: true,
    })
}

fn execute_release(
    config: &ResolvedConfig,
    options: ReleaseOptions,
) -> Result<CliOutcome, AppError> {
    let value = match options.action {
        ReleaseAction::Build(options) => {
            let mut application = Application::open(config)?;
            to_value(application.build_release(
                &options.publication_receipt_hash,
                &ExactVersionReference {
                    object_id: options.pedagogy_root_object_id,
                    version_hash: options.pedagogy_root_version_hash,
                },
                parse_pedagogy_path_mode(&options.mode)?,
                options.include_soft,
                options.max_depth,
                options.limit,
                parse_release_profile(&options.profile)?,
                &options.output_dir,
                options.dry_run,
            )?)
            .expect("release build outcome is serializable")
        }
        ReleaseAction::Verify(options) => to_value(Application::verify_release(
            &options.bundle_dir,
            &options.expected_manifest_hash,
        )?)
        .expect("release verification report is serializable"),
        ReleaseAction::Export(options) => to_value(crate::corpus_export::export_release(
            crate::corpus_export::CorpusExportRequest {
                bundle_dir: &options.bundle_dir,
                expected_manifest_hash: &options.expected_manifest_hash,
                packet_id: &options.packet_id,
                domain: crate::domain::MathCorpusDomain::from_str(&options.domain)?,
                level: crate::domain::MathCorpusLevel::from_str(&options.level)?,
                difficulty_bin: crate::domain::MathCorpusDifficultyBin::from_str(
                    &options.difficulty_bin,
                )?,
                output_dir: &options.output_dir,
                dry_run: options.dry_run,
            },
        )?)
        .expect("corpus export outcome is serializable"),
        ReleaseAction::VerifyExport(options) => to_value(crate::corpus_export::verify_export(
            &options.export_dir,
            &options.expected_manifest_hash,
            &options.source_bundle_dir,
        )?)
        .expect("corpus export verification report is serializable"),
        ReleaseAction::ExportRl(options) => to_value(crate::rl_export::export_rl(
            crate::rl_export::RlExportRequest {
                plan_path: &options.plan,
                source_root: &options.source_root,
                output_dir: &options.output_dir,
                dry_run: options.dry_run,
            },
        )?)
        .expect("RL export outcome is serializable"),
        ReleaseAction::VerifyRlExport(options) => to_value(crate::rl_export::verify_rl_export(
            &options.export_dir,
            &options.expected_manifest_hash,
            &options.plan,
            &options.source_root,
        )?)
        .expect("RL export verification report is serializable"),
        ReleaseAction::ExportComparator(options) => {
            to_value(crate::comparator_export::export_comparator(
                crate::comparator_export::ComparatorExportRequest {
                    plan_path: &options.plan,
                    bundle_dir: &options.bundle_dir,
                    expected_release_manifest_hash: &options.expected_release_manifest_hash,
                    output_dir: &options.output_dir,
                    dry_run: options.dry_run,
                },
            )?)
            .expect("Comparator export outcome is serializable")
        }
        ReleaseAction::VerifyComparatorPackage(options) => {
            to_value(crate::comparator_export::verify_comparator_package(
                &options.package_dir,
                &options.expected_verification_hash,
                &options.plan,
                &options.bundle_dir,
                &options.expected_release_manifest_hash,
            )?)
            .expect("Comparator package verification report is serializable")
        }
        ReleaseAction::VerifyComparatorRun(options) => {
            to_value(crate::comparator_run::verify_comparator_run(
                crate::comparator_run::ComparatorRunVerificationRequest {
                    run_dir: &options.run_dir,
                    expected_report_hash: &options.expected_report_hash,
                    expected_package_verification_hash: &options.expected_package_verification_hash,
                },
            )?)
            .expect("Comparator run verification outcome is serializable")
        }
    };
    Ok(CliOutcome {
        value,
        success: true,
    })
}

fn parse_release_profile(value: &str) -> Result<crate::domain::ReleaseProfile, AppError> {
    match value {
        "private" => Ok(crate::domain::ReleaseProfile::Private),
        "public" => Ok(crate::domain::ReleaseProfile::Public),
        _ => Err(AppError::new(
            "MCL_RELEASE_PROFILE_INVALID",
            format!("unknown release profile `{value}`"),
            false,
            "Use `private` or `public`.",
        )),
    }
}

fn parse_learning_unit_review_state(value: &str) -> Result<LearningUnitReviewState, AppError> {
    match value {
        "reviewed" => Ok(LearningUnitReviewState::Reviewed),
        "rejected" => Ok(LearningUnitReviewState::Rejected),
        _ => Err(AppError::new(
            "MCL_PEDAGOGY_REVIEW_DECISION_INVALID",
            format!("unknown pedagogy review decision `{value}`"),
            false,
            "Use `reviewed` or `rejected`.",
        )),
    }
}

fn parse_learning_unit_training_status(
    value: &str,
) -> Result<LearningUnitTrainingStatus, AppError> {
    match value {
        "ineligible" => Ok(LearningUnitTrainingStatus::Ineligible),
        "quarantined" => Ok(LearningUnitTrainingStatus::Quarantined),
        "eligible_private" => Ok(LearningUnitTrainingStatus::EligiblePrivate),
        "eligible_public" => Ok(LearningUnitTrainingStatus::EligiblePublic),
        "held_out_evaluation" => Ok(LearningUnitTrainingStatus::HeldOutEvaluation),
        _ => Err(AppError::new(
            "MCL_PEDAGOGY_TRAINING_STATUS_INVALID",
            format!("unknown learning-unit training status `{value}`"),
            false,
            "Use a status declared by learning_unit/1.",
        )),
    }
}

fn parse_pedagogy_path_mode(value: &str) -> Result<PedagogyPathMode, AppError> {
    match value {
        "prerequisites" => Ok(PedagogyPathMode::Prerequisites),
        "recommended" => Ok(PedagogyPathMode::Recommended),
        _ => Err(AppError::new(
            "MCL_PEDAGOGY_PATH_MODE_INVALID",
            format!("unknown pedagogy path mode `{value}`"),
            false,
            "Use `prerequisites` or `recommended`.",
        )),
    }
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
