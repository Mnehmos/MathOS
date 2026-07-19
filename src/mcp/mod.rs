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

use crate::app::Application;
use crate::config::ResolvedConfig;
use crate::domain::{EdgeKind, GraphTraversalRequest, TraversalDirection};
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
                "tools": ["system", "query"],
                "system_actions": ["describe", "health", "capabilities", "policy"],
                "query_actions": ["get", "search", "graph"],
                "mutations": false,
                "authoritative_verification": false,
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

fn required(value: Option<String>, field: &str, action: &str) -> Result<String, AppError> {
    value.filter(|item| !item.trim().is_empty()).ok_or_else(|| {
        AppError::new(
            "MCL_MCP_FIELD_REQUIRED",
            format!("query action `{action}` requires a nonempty `{field}`"),
            false,
            format!("Supply `{field}` for the `{action}` action."),
        )
    })
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
