use std::future::ready;
use std::str::FromStr;

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Implementation, InitializeRequestParams, InitializeResult, ProtocolVersion,
        ServerCapabilities, ServerInfo,
    },
    schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json, to_value};

use crate::app::{Application, PedagogyPathMode};
use crate::config::ResolvedConfig;
use crate::domain::schemas::{
    ExactVersionReference, LEARNING_UNIT_SCHEMA_VERSION, LearningUnitReviewState,
    LearningUnitTrainingStatus,
};
use crate::domain::{
    CounterexampleRepairRequest, EdgeDraft, EdgeKind, GraphTraversalRequest, PublicationOutcome,
    RecordDraft, RecordKind, RunEventDraft, RunEventKind, RunKind, TraversalDirection,
    VersionedFidelityReviewRequest,
};
use crate::error::AppError;

const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::V_2025_11_25;

#[derive(Clone, Debug)]
pub struct MathOsMcp {
    config: ResolvedConfig,
    #[expect(dead_code, reason = "tool_handler macro accesses this router field")]
    tool_router: ToolRouter<Self>,
}

impl MathOsMcp {
    pub fn new(config: ResolvedConfig) -> Self {
        Self {
            config,
            tool_router: Self::tool_router(),
        }
    }

    fn application(&self) -> Result<Application, AppError> {
        Application::open(&self.config)
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SystemRequest {
    action: SystemAction,
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SystemAction {
    Describe,
    Health,
    Capabilities,
    Policy,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QueryRequest {
    action: QueryAction,
    #[serde(default)]
    object_id: Option<String>,
    #[serde(default)]
    version_hash: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    root_object_id: Option<String>,
    #[serde(default)]
    root_version_hash: Option<String>,
    #[serde(default)]
    direction: McpTraversalDirection,
    #[serde(default)]
    edge_kinds: Vec<String>,
    #[serde(default)]
    max_depth: Option<u32>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryAction {
    Get,
    Search,
    Graph,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordMutationRequest {
    action: RecordMutationAction,
    #[serde(default)]
    object_id: Option<String>,
    #[serde(default)]
    expected_head: Option<String>,
    #[serde(default)]
    payload: Option<Value>,
    #[serde(default)]
    searchable_text: Option<String>,
    #[serde(default)]
    actor: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordMutationAction {
    Propose,
    Version,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PedagogyRequest {
    action: PedagogyAction,
    #[serde(default)]
    object_id: Option<String>,
    #[serde(default)]
    version_hash: Option<String>,
    #[serde(default)]
    expected_head: Option<String>,
    #[serde(default)]
    payload: Option<Value>,
    #[serde(default)]
    searchable_text: Option<String>,
    #[serde(default)]
    decision: Option<String>,
    #[serde(default)]
    training_status: Option<String>,
    #[serde(default)]
    notes: Option<Vec<String>>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    source_object_id: Option<String>,
    #[serde(default)]
    source_version_hash: Option<String>,
    #[serde(default)]
    target_object_id: Option<String>,
    #[serde(default)]
    target_version_hash: Option<String>,
    #[serde(default)]
    root_object_id: Option<String>,
    #[serde(default)]
    root_version_hash: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    include_soft: Option<Option<bool>>,
    #[serde(default)]
    max_depth: Option<u32>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    actor: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    dry_run: Option<Option<bool>>,
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PedagogyAction {
    Propose,
    Version,
    Get,
    Validate,
    Review,
    Link,
    Path,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResearchRequest {
    action: ResearchAction,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    expected_head: Option<String>,
    #[serde(default)]
    run_kind: Option<String>,
    #[serde(default)]
    event_kind: Option<String>,
    #[serde(default)]
    budget: Option<Value>,
    #[serde(default)]
    payload: Option<Value>,
    #[serde(default)]
    actor: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResearchAction {
    Start,
    Observe,
    Submit,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CounterexampleRequest {
    action: CounterexampleAction,
    #[serde(default)]
    request: Option<Value>,
    #[serde(default)]
    artifact_hash: Option<String>,
    #[serde(default)]
    actor: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    dry_run: Option<Option<bool>>,
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CounterexampleAction {
    Repair,
    Get,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifyRequest {
    action: VerifyAction,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    request: Option<Option<Value>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    claim_object_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    claim_version_hash: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    formalization_object_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    formalization_version_hash: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    outcome: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    diagnostic_evidence_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    proof_closure_evidence_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    axiom_audit_evidence_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    source_commit_sha: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    source_tree_sha: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    report_artifact_hash: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    attestation_bundle_artifact_hash: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    publication_receipt_hash: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    actor: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    idempotency_key: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_optional")]
    dry_run: Option<Option<bool>>,
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerifyAction {
    ReviewFidelity,
    FidelityStatus,
    ClaimStatus,
    PreparePublication,
    IngestPublication,
    PromotePublicationAuthority,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpTraversalDirection {
    Outgoing,
    Incoming,
    #[default]
    Both,
}

impl From<McpTraversalDirection> for TraversalDirection {
    fn from(value: McpTraversalDirection) -> Self {
        match value {
            McpTraversalDirection::Outgoing => Self::Outgoing,
            McpTraversalDirection::Incoming => Self::Incoming,
            McpTraversalDirection::Both => Self::Both,
        }
    }
}

fn default_search_limit() -> usize {
    20
}

fn default_graph_depth() -> u32 {
    1
}

fn default_graph_limit() -> usize {
    100
}

fn default_pedagogy_path_depth() -> u32 {
    8
}

#[tool_router]
impl MathOsMcp {
    #[tool(
        description = "Inspect MathOS identity, local health, implemented capabilities, or trust policy. Closed actions: describe, health, capabilities, policy."
    )]
    fn system(&self, Parameters(request): Parameters<SystemRequest>) -> CallToolResult {
        result_to_tool(match request.action {
            SystemAction::Describe => Ok(json!({
                "product": "MathOS",
                "component": "Mathematical Claim Engine",
                "company": "MnehmosAI",
                "version": env!("CARGO_PKG_VERSION"),
                "protocol_version": PROTOCOL_VERSION.as_str(),
                "mission": "AI proposes. Systems validate. Commits are controlled. The trace remembers."
            })),
            SystemAction::Health => Ok(to_value(Application::health(&self.config))
                .expect("diagnostic report is serializable")),
            SystemAction::Capabilities => Ok(json!({
                "tools": ["system", "query", "source", "claim", "formalization", "pedagogy", "research", "counterexample", "verify"],
                "system_actions": ["describe", "health", "capabilities", "policy"],
                "query_actions": ["get", "search", "graph"],
                "source_actions": ["propose", "version"],
                "claim_actions": ["propose", "version"],
                "formalization_actions": ["propose", "version"],
                "pedagogy_actions": ["propose", "version", "get", "validate", "review", "link", "path"],
                "research_actions": ["start", "observe", "submit"],
                "counterexample_actions": ["repair", "get"],
                "verify_actions": ["review_fidelity", "fidelity_status", "claim_status", "prepare_publication", "ingest_publication", "promote_publication_authority"],
                "mutations": true,
                "authoritative_verification": true,
                "claim_research_status": "derived_read_only",
                "transport": "stdio",
                "protocol_version": PROTOCOL_VERSION.as_str()
            })),
            SystemAction::Policy => Ok(json!({
                "model_inference": "external",
                "proof_authority": "verifier_evidence_only",
                "direct_status_mutation": false,
                "raw_shell": false,
                "raw_sql": false,
                "network_transport": false,
                "stdout": "protocol_only"
            })),
        })
    }

    #[tool(
        description = "Read canonical MathOS state through exact lookup, FTS5 search, or bounded typed graph traversal. Closed actions: get, search, graph."
    )]
    fn query(&self, Parameters(request): Parameters<QueryRequest>) -> CallToolResult {
        result_to_tool(self.execute_query(request))
    }

    #[tool(
        description = "Propose or version a source through immutable canonical records. Mutations require actor and idempotency_key; version also requires object_id and expected_head."
    )]
    fn source(&self, Parameters(request): Parameters<RecordMutationRequest>) -> CallToolResult {
        result_to_tool(self.execute_record_mutation(RecordKind::Source, request))
    }

    #[tool(
        description = "Propose or version a truth-valued claim through immutable canonical records. Mutations require actor and idempotency_key; version also requires object_id and expected_head."
    )]
    fn claim(&self, Parameters(request): Parameters<RecordMutationRequest>) -> CallToolResult {
        result_to_tool(self.execute_record_mutation(RecordKind::Claim, request))
    }

    #[tool(
        description = "Propose or version one exact formal interpretation of a claim. This never marks the claim proved, disproved, or faithful."
    )]
    fn formalization(
        &self,
        Parameters(request): Parameters<RecordMutationRequest>,
    ) -> CallToolResult {
        result_to_tool(self.execute_record_mutation(RecordKind::Formalization, request))
    }

    #[tool(
        description = "Propose, version, get, validate, review, link, or traverse canonical learning units through one closed pedagogy service. Review never grants mathematical authority."
    )]
    fn pedagogy(&self, Parameters(request): Parameters<PedagogyRequest>) -> CallToolResult {
        result_to_tool(self.execute_pedagogy(request))
    }

    #[tool(
        description = "Start, observe, or submit a typed non-authoritative research run. Run events preserve proposals and diagnostics but never decide mathematical truth."
    )]
    fn research(&self, Parameters(request): Parameters<ResearchRequest>) -> CallToolResult {
        result_to_tool(self.execute_research(request))
    }

    #[tool(
        description = "Build or retrieve a canonical counterexample package through the controlled atomic repair path. Closed actions: repair, get. repair requires one counterexample_repair_request/1 object, actor, and idempotency_key; get requires artifact_hash."
    )]
    fn counterexample(
        &self,
        Parameters(request): Parameters<CounterexampleRequest>,
    ) -> CallToolResult {
        result_to_tool(self.execute_counterexample(request))
    }

    #[tool(
        description = "Create role-separated statement-fidelity evidence from a closed fidelity_review_request/1 or fidelity_review_request/2 object, read its derived status, derive one exact claim version's research status, prepare and ingest non-authoritative publication records, or promote one fully replayed receipt to exact formalization proof/refutation authority. Closed actions: review_fidelity, fidelity_status, claim_status, prepare_publication, ingest_publication, promote_publication_authority. claim_status accepts exact claim identity only and never stores or accepts a verdict."
    )]
    fn verify(&self, Parameters(request): Parameters<VerifyRequest>) -> CallToolResult {
        result_to_tool(self.execute_verify(request))
    }

    fn execute_query(&self, request: QueryRequest) -> Result<Value, AppError> {
        let application = self.application()?;
        match request.action {
            QueryAction::Get => {
                let object_id = required(request.object_id, "object_id", "get")?;
                to_value(application.get_record(&object_id, request.version_hash.as_deref())?)
                    .map_err(serialization_error)
            }
            QueryAction::Search => {
                let query = required(request.query, "query", "search")?;
                let limit = request.limit.unwrap_or_else(default_search_limit);
                validate_limit(limit, default_search_limit(), "search")?;
                to_value(application.search_records(&query, limit)?).map_err(serialization_error)
            }
            QueryAction::Graph => {
                let root_object_id = required(request.root_object_id, "root_object_id", "graph")?;
                let root_version_hash =
                    required(request.root_version_hash, "root_version_hash", "graph")?;
                let limit = request.limit.unwrap_or_else(default_graph_limit);
                let max_depth = request.max_depth.unwrap_or_else(default_graph_depth);
                validate_limit(limit, default_graph_limit(), "graph")?;
                if max_depth == 0 || max_depth > 32 {
                    return Err(AppError::new(
                        "MCL_GRAPH_DEPTH_INVALID",
                        format!("graph max_depth must be between 1 and 32; received {max_depth}"),
                        false,
                        "Choose an explicit bounded depth from 1 through 32.",
                    ));
                }
                let edge_kinds = request
                    .edge_kinds
                    .iter()
                    .map(|kind| EdgeKind::from_str(kind))
                    .collect::<Result<Vec<_>, _>>()?;
                let request = GraphTraversalRequest {
                    root_object_id,
                    root_version_hash,
                    direction: request.direction.into(),
                    edge_kinds,
                    max_depth,
                    limit,
                };
                to_value(application.traverse_graph(&request)?).map_err(serialization_error)
            }
        }
    }

    fn execute_record_mutation(
        &self,
        kind: RecordKind,
        request: RecordMutationRequest,
    ) -> Result<Value, AppError> {
        let action_name = record_action_name(request.action);
        let payload = request
            .payload
            .ok_or_else(|| missing_field("payload", action_name, kind.as_str()))?;
        let searchable_text = required(request.searchable_text, "searchable_text", kind.as_str())?;
        let actor = required(request.actor, "actor", kind.as_str())?;
        let idempotency_key = required(request.idempotency_key, "idempotency_key", kind.as_str())?;
        let draft = RecordDraft {
            kind,
            schema_version: record_schema_version(kind).to_owned(),
            payload,
            searchable_text,
        };
        let mut application = self.application()?;
        match request.action {
            RecordMutationAction::Propose => {
                reject_present(request.object_id, "object_id", "propose")?;
                reject_present(request.expected_head, "expected_head", "propose")?;
                to_value(application.create_record(
                    &draft,
                    &actor,
                    &idempotency_key,
                    request.dry_run,
                )?)
                .map_err(serialization_error)
            }
            RecordMutationAction::Version => {
                let object_id = required(request.object_id, "object_id", "version")?;
                let expected_head = required(request.expected_head, "expected_head", "version")?;
                to_value(application.version_record(
                    &object_id,
                    &expected_head,
                    &draft,
                    &actor,
                    &idempotency_key,
                    request.dry_run,
                )?)
                .map_err(serialization_error)
            }
        }
    }

    fn execute_pedagogy(&self, request: PedagogyRequest) -> Result<Value, AppError> {
        let mut application = self.application()?;
        match request.action {
            PedagogyAction::Propose => {
                reject_pedagogy_fields(
                    &request,
                    &[
                        "payload",
                        "searchable_text",
                        "actor",
                        "idempotency_key",
                        "dry_run",
                    ],
                    "propose",
                )?;
                let payload = required_value(request.payload, "payload", "propose")?;
                let searchable_text =
                    required(request.searchable_text, "searchable_text", "propose")?;
                let actor = required(request.actor, "actor", "propose")?;
                let idempotency_key =
                    required(request.idempotency_key, "idempotency_key", "propose")?;
                to_value(application.create_record(
                    &RecordDraft {
                        kind: RecordKind::LearningUnit,
                        schema_version: LEARNING_UNIT_SCHEMA_VERSION.to_owned(),
                        payload,
                        searchable_text,
                    },
                    &actor,
                    &idempotency_key,
                    mutation_dry_run(request.dry_run, "propose")?,
                )?)
                .map_err(serialization_error)
            }
            PedagogyAction::Version => {
                reject_pedagogy_fields(
                    &request,
                    &[
                        "object_id",
                        "expected_head",
                        "payload",
                        "searchable_text",
                        "actor",
                        "idempotency_key",
                        "dry_run",
                    ],
                    "version",
                )?;
                let object_id = required(request.object_id, "object_id", "version")?;
                let expected_head = required(request.expected_head, "expected_head", "version")?;
                let payload = required_value(request.payload, "payload", "version")?;
                let searchable_text =
                    required(request.searchable_text, "searchable_text", "version")?;
                let actor = required(request.actor, "actor", "version")?;
                let idempotency_key =
                    required(request.idempotency_key, "idempotency_key", "version")?;
                to_value(application.version_record(
                    &object_id,
                    &expected_head,
                    &RecordDraft {
                        kind: RecordKind::LearningUnit,
                        schema_version: LEARNING_UNIT_SCHEMA_VERSION.to_owned(),
                        payload,
                        searchable_text,
                    },
                    &actor,
                    &idempotency_key,
                    mutation_dry_run(request.dry_run, "version")?,
                )?)
                .map_err(serialization_error)
            }
            PedagogyAction::Get => {
                reject_pedagogy_fields(&request, &["object_id", "version_hash"], "get")?;
                let object_id = required(request.object_id, "object_id", "get")?;
                let record = application.get_record(&object_id, request.version_hash.as_deref())?;
                if record.kind != RecordKind::LearningUnit {
                    return Err(AppError::new(
                        "MCL_RECORD_KIND_MISMATCH",
                        format!(
                            "pedagogy get resolved to `{}`, not `learning_unit`",
                            record.kind
                        ),
                        false,
                        "Use pedagogy get only with a canonical learning-unit object.",
                    ));
                }
                to_value(record).map_err(serialization_error)
            }
            PedagogyAction::Validate => {
                reject_pedagogy_fields(&request, &["object_id", "version_hash"], "validate")?;
                let object_id = required(request.object_id, "object_id", "validate")?;
                let version_hash = required(request.version_hash, "version_hash", "validate")?;
                to_value(application.validate_learning_unit(&object_id, &version_hash)?)
                    .map_err(serialization_error)
            }
            PedagogyAction::Review => {
                reject_pedagogy_fields(
                    &request,
                    &[
                        "object_id",
                        "expected_head",
                        "decision",
                        "training_status",
                        "notes",
                        "actor",
                        "idempotency_key",
                        "dry_run",
                    ],
                    "review",
                )?;
                let object_id = required(request.object_id, "object_id", "review")?;
                let expected_head = required(request.expected_head, "expected_head", "review")?;
                let decision = required(request.decision, "decision", "review")?;
                let training_status =
                    required(request.training_status, "training_status", "review")?;
                let notes = required_value(request.notes, "notes", "review")?;
                let actor = required(request.actor, "actor", "review")?;
                let idempotency_key =
                    required(request.idempotency_key, "idempotency_key", "review")?;
                to_value(application.review_learning_unit(
                    &object_id,
                    &expected_head,
                    parse_pedagogy_review_state(&decision)?,
                    parse_pedagogy_training_status(&training_status)?,
                    notes,
                    &actor,
                    &idempotency_key,
                    mutation_dry_run(request.dry_run, "review")?,
                )?)
                .map_err(serialization_error)
            }
            PedagogyAction::Link => {
                reject_pedagogy_fields(
                    &request,
                    &[
                        "kind",
                        "source_object_id",
                        "source_version_hash",
                        "target_object_id",
                        "target_version_hash",
                        "payload",
                        "actor",
                        "idempotency_key",
                        "dry_run",
                    ],
                    "link",
                )?;
                let kind = required(request.kind, "kind", "link")?;
                let source_object_id =
                    required(request.source_object_id, "source_object_id", "link")?;
                let source_version_hash =
                    required(request.source_version_hash, "source_version_hash", "link")?;
                let target_object_id =
                    required(request.target_object_id, "target_object_id", "link")?;
                let target_version_hash =
                    required(request.target_version_hash, "target_version_hash", "link")?;
                let payload = required_value(request.payload, "payload", "link")?;
                let actor = required(request.actor, "actor", "link")?;
                let idempotency_key = required(request.idempotency_key, "idempotency_key", "link")?;
                to_value(application.create_pedagogy_link(
                    &EdgeDraft {
                        kind: EdgeKind::from_str(&kind)?,
                        source_object_id,
                        source_version_hash,
                        target_object_id,
                        target_version_hash,
                        payload,
                    },
                    &actor,
                    &idempotency_key,
                    mutation_dry_run(request.dry_run, "link")?,
                )?)
                .map_err(serialization_error)
            }
            PedagogyAction::Path => {
                reject_pedagogy_fields(
                    &request,
                    &[
                        "root_object_id",
                        "root_version_hash",
                        "mode",
                        "include_soft",
                        "max_depth",
                        "limit",
                    ],
                    "path",
                )?;
                let root_object_id = required(request.root_object_id, "root_object_id", "path")?;
                let root_version_hash =
                    required(request.root_version_hash, "root_version_hash", "path")?;
                let mode = required(request.mode, "mode", "path")?;
                let include_soft =
                    optional_present_value(request.include_soft, false, "include_soft", "path")?;
                to_value(
                    application.pedagogy_path(
                        &ExactVersionReference {
                            object_id: root_object_id,
                            version_hash: root_version_hash,
                        },
                        parse_pedagogy_path_mode(&mode)?,
                        include_soft,
                        request
                            .max_depth
                            .unwrap_or_else(default_pedagogy_path_depth),
                        request.limit.unwrap_or_else(default_graph_limit),
                    )?,
                )
                .map_err(serialization_error)
            }
        }
    }

    fn execute_research(&self, request: ResearchRequest) -> Result<Value, AppError> {
        let mut application = self.application()?;
        match request.action {
            ResearchAction::Start => {
                let kind = RunKind::from_str(&required(request.run_kind, "run_kind", "start")?)?;
                let actor = required(request.actor, "actor", "start")?;
                let idempotency_key =
                    required(request.idempotency_key, "idempotency_key", "start")?;
                reject_present(request.run_id, "run_id", "start")?;
                reject_present(request.expected_head, "expected_head", "start")?;
                reject_present(request.event_kind, "event_kind", "start")?;
                to_value(application.create_run(
                    kind,
                    &request.budget.unwrap_or_else(|| json!({})),
                    &actor,
                    &idempotency_key,
                    request.dry_run,
                )?)
                .map_err(serialization_error)
            }
            ResearchAction::Observe => {
                let run_id = required(request.run_id, "run_id", "observe")?;
                if request.dry_run {
                    return Err(AppError::new(
                        "MCL_MCP_FIELD_FORBIDDEN",
                        "research action `observe` does not accept `dry_run`",
                        false,
                        "Remove `dry_run`; observe is already read-only.",
                    ));
                }
                let run = application.get_run(&run_id)?;
                let events = application.list_run_events(&run_id)?;
                Ok(json!({"run": run, "events": events}))
            }
            ResearchAction::Submit => {
                let run_id = required(request.run_id, "run_id", "submit")?;
                let expected_head = required(request.expected_head, "expected_head", "submit")?;
                let event_kind =
                    RunEventKind::from_str(&required(request.event_kind, "event_kind", "submit")?)?;
                let actor = required(request.actor, "actor", "submit")?;
                let idempotency_key =
                    required(request.idempotency_key, "idempotency_key", "submit")?;
                let draft = RunEventDraft {
                    kind: event_kind,
                    payload: request.payload.unwrap_or_else(|| json!({})),
                };
                to_value(application.append_run_event(
                    &run_id,
                    &expected_head,
                    &draft,
                    &actor,
                    &idempotency_key,
                    request.dry_run,
                )?)
                .map_err(serialization_error)
            }
        }
    }

    fn execute_counterexample(&self, request: CounterexampleRequest) -> Result<Value, AppError> {
        let mut application = self.application()?;
        match request.action {
            CounterexampleAction::Repair => {
                reject_present(request.artifact_hash, "artifact_hash", "repair")?;
                let repair_value = request
                    .request
                    .ok_or_else(|| missing_field("request", "repair", "counterexample"))?;
                let repair_request: CounterexampleRepairRequest =
                    serde_json::from_value(repair_value).map_err(|error| {
                        AppError::new(
                            "MCL_COUNTEREXAMPLE_JSON_INVALID",
                            error.to_string(),
                            false,
                            "Supply one closed counterexample_repair_request/1 object.",
                        )
                    })?;
                let actor = required(request.actor, "actor", "counterexample repair")?;
                let idempotency_key = required(
                    request.idempotency_key,
                    "idempotency_key",
                    "counterexample repair",
                )?;
                to_value(application.repair_disproved_claim(
                    &repair_request,
                    &actor,
                    &idempotency_key,
                    mutation_dry_run(request.dry_run, "counterexample repair")?,
                )?)
                .map_err(serialization_error)
            }
            CounterexampleAction::Get => {
                reject_present(request.request, "request", "counterexample get")?;
                reject_present(request.actor, "actor", "counterexample get")?;
                reject_present(
                    request.idempotency_key,
                    "idempotency_key",
                    "counterexample get",
                )?;
                reject_present(request.dry_run, "dry_run", "counterexample get")?;
                let artifact_hash =
                    required(request.artifact_hash, "artifact_hash", "counterexample get")?;
                to_value(application.get_counterexample_package(&artifact_hash)?)
                    .map_err(serialization_error)
            }
        }
    }

    fn execute_verify(&self, request: VerifyRequest) -> Result<Value, AppError> {
        let mut application = self.application()?;
        match request.action {
            VerifyAction::ReviewFidelity => {
                reject_present(
                    request.claim_object_id,
                    "claim_object_id",
                    "review_fidelity",
                )?;
                reject_present(
                    request.claim_version_hash,
                    "claim_version_hash",
                    "review_fidelity",
                )?;
                let payload = request
                    .request
                    .flatten()
                    .ok_or_else(|| missing_field("request", "review_fidelity", "verify"))?;
                let review: VersionedFidelityReviewRequest =
                    serde_json::from_value(payload).map_err(|error| {
                        AppError::new(
                            "MCL_FIDELITY_JSON_INVALID",
                            error.to_string(),
                            false,
                            "Supply one closed fidelity_review_request/1 or fidelity_review_request/2 object.",
                        )
                    })?;
                let actor = required(request.actor.flatten(), "actor", "review_fidelity")?;
                let idempotency_key = required(
                    request.idempotency_key.flatten(),
                    "idempotency_key",
                    "review_fidelity",
                )?;
                reject_present(
                    request.formalization_object_id,
                    "formalization_object_id",
                    "review_fidelity",
                )?;
                reject_present(
                    request.formalization_version_hash,
                    "formalization_version_hash",
                    "review_fidelity",
                )?;
                reject_present(request.outcome, "outcome", "review_fidelity")?;
                reject_present(
                    request.diagnostic_evidence_id,
                    "diagnostic_evidence_id",
                    "review_fidelity",
                )?;
                reject_present(
                    request.proof_closure_evidence_id,
                    "proof_closure_evidence_id",
                    "review_fidelity",
                )?;
                reject_present(
                    request.axiom_audit_evidence_id,
                    "axiom_audit_evidence_id",
                    "review_fidelity",
                )?;
                reject_present(
                    request.source_commit_sha,
                    "source_commit_sha",
                    "review_fidelity",
                )?;
                reject_present(
                    request.source_tree_sha,
                    "source_tree_sha",
                    "review_fidelity",
                )?;
                reject_present(
                    request.report_artifact_hash,
                    "report_artifact_hash",
                    "review_fidelity",
                )?;
                reject_present(
                    request.attestation_bundle_artifact_hash,
                    "attestation_bundle_artifact_hash",
                    "review_fidelity",
                )?;
                reject_present(
                    request.publication_receipt_hash,
                    "publication_receipt_hash",
                    "review_fidelity",
                )?;
                to_value(application.review_fidelity(
                    &review,
                    &actor,
                    &idempotency_key,
                    mutation_dry_run(request.dry_run, "review_fidelity")?,
                )?)
                .map_err(serialization_error)
            }
            VerifyAction::FidelityStatus => {
                reject_present(request.dry_run, "dry_run", "fidelity_status")?;
                reject_present(
                    request.claim_object_id,
                    "claim_object_id",
                    "fidelity_status",
                )?;
                reject_present(
                    request.claim_version_hash,
                    "claim_version_hash",
                    "fidelity_status",
                )?;
                reject_present(request.request, "request", "fidelity_status")?;
                reject_present(request.actor, "actor", "fidelity_status")?;
                reject_present(
                    request.idempotency_key,
                    "idempotency_key",
                    "fidelity_status",
                )?;
                reject_present(request.outcome, "outcome", "fidelity_status")?;
                reject_present(
                    request.diagnostic_evidence_id,
                    "diagnostic_evidence_id",
                    "fidelity_status",
                )?;
                reject_present(
                    request.proof_closure_evidence_id,
                    "proof_closure_evidence_id",
                    "fidelity_status",
                )?;
                reject_present(
                    request.axiom_audit_evidence_id,
                    "axiom_audit_evidence_id",
                    "fidelity_status",
                )?;
                reject_present(
                    request.source_commit_sha,
                    "source_commit_sha",
                    "fidelity_status",
                )?;
                reject_present(
                    request.source_tree_sha,
                    "source_tree_sha",
                    "fidelity_status",
                )?;
                reject_present(
                    request.report_artifact_hash,
                    "report_artifact_hash",
                    "fidelity_status",
                )?;
                reject_present(
                    request.attestation_bundle_artifact_hash,
                    "attestation_bundle_artifact_hash",
                    "fidelity_status",
                )?;
                reject_present(
                    request.publication_receipt_hash,
                    "publication_receipt_hash",
                    "fidelity_status",
                )?;
                let formalization = crate::domain::schemas::ExactVersionReference {
                    object_id: required(
                        request.formalization_object_id.flatten(),
                        "formalization_object_id",
                        "fidelity_status",
                    )?,
                    version_hash: required(
                        request.formalization_version_hash.flatten(),
                        "formalization_version_hash",
                        "fidelity_status",
                    )?,
                };
                to_value(application.fidelity_status(&formalization)?).map_err(serialization_error)
            }
            VerifyAction::ClaimStatus => {
                reject_present(request.request, "request", "claim_status")?;
                reject_present(
                    request.formalization_object_id,
                    "formalization_object_id",
                    "claim_status",
                )?;
                reject_present(
                    request.formalization_version_hash,
                    "formalization_version_hash",
                    "claim_status",
                )?;
                reject_present(request.outcome, "outcome", "claim_status")?;
                reject_present(
                    request.diagnostic_evidence_id,
                    "diagnostic_evidence_id",
                    "claim_status",
                )?;
                reject_present(
                    request.proof_closure_evidence_id,
                    "proof_closure_evidence_id",
                    "claim_status",
                )?;
                reject_present(
                    request.axiom_audit_evidence_id,
                    "axiom_audit_evidence_id",
                    "claim_status",
                )?;
                reject_present(
                    request.source_commit_sha,
                    "source_commit_sha",
                    "claim_status",
                )?;
                reject_present(request.source_tree_sha, "source_tree_sha", "claim_status")?;
                reject_present(
                    request.report_artifact_hash,
                    "report_artifact_hash",
                    "claim_status",
                )?;
                reject_present(
                    request.attestation_bundle_artifact_hash,
                    "attestation_bundle_artifact_hash",
                    "claim_status",
                )?;
                reject_present(
                    request.publication_receipt_hash,
                    "publication_receipt_hash",
                    "claim_status",
                )?;
                reject_present(request.actor, "actor", "claim_status")?;
                reject_present(request.idempotency_key, "idempotency_key", "claim_status")?;
                reject_present(request.dry_run, "dry_run", "claim_status")?;
                let claim = crate::domain::schemas::ExactVersionReference {
                    object_id: required(
                        request.claim_object_id.flatten(),
                        "claim_object_id",
                        "claim_status",
                    )?,
                    version_hash: required(
                        request.claim_version_hash.flatten(),
                        "claim_version_hash",
                        "claim_status",
                    )?,
                };
                to_value(application.claim_research_status(&claim)?).map_err(serialization_error)
            }
            VerifyAction::PreparePublication => {
                reject_present(
                    request.claim_object_id,
                    "claim_object_id",
                    "prepare_publication",
                )?;
                reject_present(
                    request.claim_version_hash,
                    "claim_version_hash",
                    "prepare_publication",
                )?;
                reject_present(request.request, "request", "prepare_publication")?;
                reject_present(
                    request.report_artifact_hash,
                    "report_artifact_hash",
                    "prepare_publication",
                )?;
                reject_present(
                    request.attestation_bundle_artifact_hash,
                    "attestation_bundle_artifact_hash",
                    "prepare_publication",
                )?;
                reject_present(
                    request.publication_receipt_hash,
                    "publication_receipt_hash",
                    "prepare_publication",
                )?;
                let formalization = crate::domain::schemas::ExactVersionReference {
                    object_id: required(
                        request.formalization_object_id.flatten(),
                        "formalization_object_id",
                        "prepare_publication",
                    )?,
                    version_hash: required(
                        request.formalization_version_hash.flatten(),
                        "formalization_version_hash",
                        "prepare_publication",
                    )?,
                };
                let outcome = PublicationOutcome::from_str(&required(
                    request.outcome.flatten(),
                    "outcome",
                    "prepare_publication",
                )?)?;
                let diagnostic_evidence_id = required(
                    request.diagnostic_evidence_id.flatten(),
                    "diagnostic_evidence_id",
                    "prepare_publication",
                )?;
                let proof_closure_evidence_id = required(
                    request.proof_closure_evidence_id.flatten(),
                    "proof_closure_evidence_id",
                    "prepare_publication",
                )?;
                let axiom_audit_evidence_id = required(
                    request.axiom_audit_evidence_id.flatten(),
                    "axiom_audit_evidence_id",
                    "prepare_publication",
                )?;
                let source_commit_sha = required(
                    request.source_commit_sha.flatten(),
                    "source_commit_sha",
                    "prepare_publication",
                )?;
                let source_tree_sha = required(
                    request.source_tree_sha.flatten(),
                    "source_tree_sha",
                    "prepare_publication",
                )?;
                let actor = required(request.actor.flatten(), "actor", "prepare_publication")?;
                let idempotency_key = required(
                    request.idempotency_key.flatten(),
                    "idempotency_key",
                    "prepare_publication",
                )?;
                to_value(application.prepare_publication_request(
                    &formalization,
                    outcome,
                    &diagnostic_evidence_id,
                    &proof_closure_evidence_id,
                    &axiom_audit_evidence_id,
                    &source_commit_sha,
                    &source_tree_sha,
                    &actor,
                    &idempotency_key,
                    mutation_dry_run(request.dry_run, "prepare_publication")?,
                )?)
                .map_err(serialization_error)
            }
            VerifyAction::IngestPublication => {
                reject_present(
                    request.claim_object_id,
                    "claim_object_id",
                    "ingest_publication",
                )?;
                reject_present(
                    request.claim_version_hash,
                    "claim_version_hash",
                    "ingest_publication",
                )?;
                reject_present(request.request, "request", "ingest_publication")?;
                reject_present(
                    request.formalization_object_id,
                    "formalization_object_id",
                    "ingest_publication",
                )?;
                reject_present(
                    request.formalization_version_hash,
                    "formalization_version_hash",
                    "ingest_publication",
                )?;
                reject_present(request.outcome, "outcome", "ingest_publication")?;
                reject_present(
                    request.diagnostic_evidence_id,
                    "diagnostic_evidence_id",
                    "ingest_publication",
                )?;
                reject_present(
                    request.proof_closure_evidence_id,
                    "proof_closure_evidence_id",
                    "ingest_publication",
                )?;
                reject_present(
                    request.axiom_audit_evidence_id,
                    "axiom_audit_evidence_id",
                    "ingest_publication",
                )?;
                reject_present(
                    request.source_commit_sha,
                    "source_commit_sha",
                    "ingest_publication",
                )?;
                reject_present(
                    request.source_tree_sha,
                    "source_tree_sha",
                    "ingest_publication",
                )?;
                reject_present(
                    request.publication_receipt_hash,
                    "publication_receipt_hash",
                    "ingest_publication",
                )?;
                let report_artifact_hash = required(
                    request.report_artifact_hash.flatten(),
                    "report_artifact_hash",
                    "ingest_publication",
                )?;
                let attestation_bundle_artifact_hash = required(
                    request.attestation_bundle_artifact_hash.flatten(),
                    "attestation_bundle_artifact_hash",
                    "ingest_publication",
                )?;
                let actor = required(request.actor.flatten(), "actor", "ingest_publication")?;
                let idempotency_key = required(
                    request.idempotency_key.flatten(),
                    "idempotency_key",
                    "ingest_publication",
                )?;
                to_value(application.ingest_publication(
                    &report_artifact_hash,
                    &attestation_bundle_artifact_hash,
                    &actor,
                    &idempotency_key,
                    mutation_dry_run(request.dry_run, "ingest_publication")?,
                )?)
                .map_err(serialization_error)
            }
            VerifyAction::PromotePublicationAuthority => {
                reject_present(
                    request.claim_object_id,
                    "claim_object_id",
                    "promote_publication_authority",
                )?;
                reject_present(
                    request.claim_version_hash,
                    "claim_version_hash",
                    "promote_publication_authority",
                )?;
                reject_present(request.request, "request", "promote_publication_authority")?;
                reject_present(
                    request.formalization_object_id,
                    "formalization_object_id",
                    "promote_publication_authority",
                )?;
                reject_present(
                    request.formalization_version_hash,
                    "formalization_version_hash",
                    "promote_publication_authority",
                )?;
                reject_present(request.outcome, "outcome", "promote_publication_authority")?;
                reject_present(
                    request.diagnostic_evidence_id,
                    "diagnostic_evidence_id",
                    "promote_publication_authority",
                )?;
                reject_present(
                    request.proof_closure_evidence_id,
                    "proof_closure_evidence_id",
                    "promote_publication_authority",
                )?;
                reject_present(
                    request.axiom_audit_evidence_id,
                    "axiom_audit_evidence_id",
                    "promote_publication_authority",
                )?;
                reject_present(
                    request.source_commit_sha,
                    "source_commit_sha",
                    "promote_publication_authority",
                )?;
                reject_present(
                    request.source_tree_sha,
                    "source_tree_sha",
                    "promote_publication_authority",
                )?;
                reject_present(
                    request.report_artifact_hash,
                    "report_artifact_hash",
                    "promote_publication_authority",
                )?;
                reject_present(
                    request.attestation_bundle_artifact_hash,
                    "attestation_bundle_artifact_hash",
                    "promote_publication_authority",
                )?;
                let publication_receipt_hash = required(
                    request.publication_receipt_hash.flatten(),
                    "publication_receipt_hash",
                    "promote_publication_authority",
                )?;
                let actor = required(
                    request.actor.flatten(),
                    "actor",
                    "promote_publication_authority",
                )?;
                let idempotency_key = required(
                    request.idempotency_key.flatten(),
                    "idempotency_key",
                    "promote_publication_authority",
                )?;
                to_value(application.promote_publication_authority(
                    &publication_receipt_hash,
                    &actor,
                    &idempotency_key,
                    mutation_dry_run(request.dry_run, "promote_publication_authority")?,
                )?)
                .map_err(serialization_error)
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for MathOsMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(PROTOCOL_VERSION)
            .with_server_info(
                Implementation::new("mathos", env!("CARGO_PKG_VERSION"))
                    .with_title("MathOS Mathematical Claim Engine"),
            )
            .with_instructions(
                "Use typed MathOS actions. Model output is a proposal and never proof authority.",
            )
    }

    fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<InitializeResult, McpError>> + Send + '_ {
        if request.protocol_version != PROTOCOL_VERSION {
            return ready(Err(McpError::invalid_request(
                format!(
                    "unsupported MCP protocol version {}; MathOS requires {}",
                    request.protocol_version, PROTOCOL_VERSION
                ),
                Some(json!({
                    "requested": request.protocol_version.as_str(),
                    "supported": PROTOCOL_VERSION.as_str()
                })),
            )));
        }
        context.peer.set_peer_info(request);
        ready(Ok(self.get_info()))
    }
}

pub fn serve_stdio(config: ResolvedConfig) -> Result<(), AppError> {
    Application::open(&config)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| AppError::io("create MCP runtime", error))?;
    runtime.block_on(async move {
        let service = MathOsMcp::new(config)
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|error| mcp_runtime_error("start MCP stdio server", error))?;
        service
            .waiting()
            .await
            .map_err(|error| mcp_runtime_error("run MCP stdio server", error))?;
        Ok(())
    })
}

fn result_to_tool(result: Result<Value, AppError>) -> CallToolResult {
    match result {
        Ok(value) => CallToolResult::structured(value),
        Err(error) => CallToolResult::structured_error(
            to_value(error).expect("application error is serializable"),
        ),
    }
}

fn validate_limit(limit: usize, default: usize, operation: &str) -> Result<(), AppError> {
    if limit == 0 || limit > 1_000 {
        return Err(AppError::new(
            "MCL_QUERY_LIMIT_INVALID",
            format!("{operation} limit must be between 1 and 1000; received {limit}"),
            false,
            format!("Use a bounded limit such as {default}."),
        ));
    }
    Ok(())
}

fn deserialize_present_optional<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

fn required(value: Option<String>, field: &str, action: &str) -> Result<String, AppError> {
    value.filter(|item| !item.trim().is_empty()).ok_or_else(|| {
        AppError::new(
            "MCL_MCP_FIELD_REQUIRED",
            format!("action `{action}` requires a nonempty `{field}`"),
            false,
            format!("Supply `{field}` for the `{action}` action."),
        )
    })
}

fn required_value<T>(value: Option<T>, field: &str, action: &str) -> Result<T, AppError> {
    value.ok_or_else(|| missing_field(field, action, "pedagogy"))
}

fn mutation_dry_run(value: Option<Option<bool>>, action: &str) -> Result<bool, AppError> {
    match value {
        None => Ok(false),
        Some(Some(dry_run)) => Ok(dry_run),
        Some(None) => Err(AppError::new(
            "MCL_MCP_FIELD_INVALID",
            format!("action `{action}` requires `dry_run` to be a Boolean when present"),
            false,
            "Set `dry_run` to true or false, or omit it.",
        )),
    }
}

fn missing_field(field: &str, action: &str, family: &str) -> AppError {
    AppError::new(
        "MCL_MCP_FIELD_REQUIRED",
        format!("{family} action `{action}` requires `{field}`"),
        false,
        format!("Supply `{field}` for the `{action}` action."),
    )
}

fn reject_present<T>(value: Option<T>, field: &str, action: &str) -> Result<(), AppError> {
    if value.is_some() {
        return Err(AppError::new(
            "MCL_MCP_FIELD_FORBIDDEN",
            format!("action `{action}` does not accept `{field}`"),
            false,
            format!("Remove `{field}` from the `{action}` action."),
        ));
    }
    Ok(())
}

fn reject_pedagogy_fields(
    request: &PedagogyRequest,
    allowed: &[&str],
    action: &str,
) -> Result<(), AppError> {
    let present = [
        ("object_id", request.object_id.is_some()),
        ("version_hash", request.version_hash.is_some()),
        ("expected_head", request.expected_head.is_some()),
        ("payload", request.payload.is_some()),
        ("searchable_text", request.searchable_text.is_some()),
        ("decision", request.decision.is_some()),
        ("training_status", request.training_status.is_some()),
        ("notes", request.notes.is_some()),
        ("kind", request.kind.is_some()),
        ("source_object_id", request.source_object_id.is_some()),
        ("source_version_hash", request.source_version_hash.is_some()),
        ("target_object_id", request.target_object_id.is_some()),
        ("target_version_hash", request.target_version_hash.is_some()),
        ("root_object_id", request.root_object_id.is_some()),
        ("root_version_hash", request.root_version_hash.is_some()),
        ("mode", request.mode.is_some()),
        ("include_soft", request.include_soft.is_some()),
        ("max_depth", request.max_depth.is_some()),
        ("limit", request.limit.is_some()),
        ("actor", request.actor.is_some()),
        ("idempotency_key", request.idempotency_key.is_some()),
        ("dry_run", request.dry_run.is_some()),
    ];
    for (field, is_present) in present {
        if is_present && !allowed.contains(&field) {
            return Err(AppError::new(
                "MCL_MCP_FIELD_FORBIDDEN",
                format!("action `{action}` does not accept `{field}`"),
                false,
                format!("Remove `{field}` from the `{action}` action."),
            ));
        }
    }
    Ok(())
}

fn optional_present_value(
    value: Option<Option<bool>>,
    default: bool,
    field: &str,
    action: &str,
) -> Result<bool, AppError> {
    match value {
        None => Ok(default),
        Some(Some(value)) => Ok(value),
        Some(None) => Err(AppError::new(
            "MCL_MCP_FIELD_REQUIRED",
            format!("action `{action}` requires `{field}` to be a boolean when present"),
            false,
            format!("Supply a boolean `{field}` or omit it."),
        )),
    }
}

fn record_action_name(action: RecordMutationAction) -> &'static str {
    match action {
        RecordMutationAction::Propose => "propose",
        RecordMutationAction::Version => "version",
    }
}

fn record_schema_version(kind: RecordKind) -> &'static str {
    match kind {
        RecordKind::Source => "source/1",
        RecordKind::Concept => "concept/1",
        RecordKind::Claim => "claim/1",
        RecordKind::Formalization => "formalization/1",
        RecordKind::LearningUnit => LEARNING_UNIT_SCHEMA_VERSION,
    }
}

fn parse_pedagogy_review_state(value: &str) -> Result<LearningUnitReviewState, AppError> {
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

fn parse_pedagogy_training_status(value: &str) -> Result<LearningUnitTrainingStatus, AppError> {
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

fn serialization_error(error: serde_json::Error) -> AppError {
    AppError::new(
        "MCL_MCP_SERIALIZATION_FAILED",
        error.to_string(),
        false,
        "Report this deterministic MCP serialization defect.",
    )
}

fn mcp_runtime_error(context: &str, error: impl std::fmt::Display) -> AppError {
    AppError::new(
        "MCL_MCP_RUNTIME_FAILED",
        format!("{context}: {error}"),
        true,
        "Inspect stderr diagnostics, confirm stdin/stdout pipes remain open, and retry.",
    )
}
