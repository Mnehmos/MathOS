use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::str::FromStr;

use crate::domain::TrustProfile;
use crate::error::AppError;

pub const VERIFIER_REQUEST_SCHEMA_VERSION: &str = "verifier_request/1";
pub const VERIFIER_EXECUTION_REPORT_SCHEMA_VERSION: &str = "verifier_execution_report/1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierJobRequest {
    pub schema_version: String,
    pub environment_hash: String,
    pub module_artifact_hash: String,
    pub declaration_name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifierJobState {
    Queued,
    Leased,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Blocked,
}

impl VerifierJobState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Leased => "leased",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Blocked => "blocked",
        }
    }
}

impl FromStr for VerifierJobState {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "leased" => Ok(Self::Leased),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "blocked" => Ok(Self::Blocked),
            _ => Err(AppError::new(
                "MCL_VERIFIER_JOB_INTEGRITY_FAILED",
                format!("stored verifier job state `{value}` is invalid"),
                false,
                "Quarantine the database and restore a verified backup.",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VerifierJobSnapshot {
    pub job_id: String,
    pub request: VerifierJobRequest,
    pub canonical_input_hash: String,
    pub state: VerifierJobState,
    pub priority: i32,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<i64>,
    pub attempt_count: u32,
    pub progress: Value,
    pub result_artifact_hash: Option<String>,
    pub last_error: Option<Value>,
    pub actor: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifierExecutionClassification {
    Elaborated,
    Rejected,
    TimedOut,
    OutputLimitExceeded,
    ToolchainMismatch,
    LaunchFailed,
    UnsafeSource,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierExecutionReport {
    pub schema_version: String,
    pub job_id: String,
    pub environment_hash: String,
    pub module_artifact_hash: String,
    pub declaration_name: String,
    pub classification: VerifierExecutionClassification,
    pub exit_code: Option<i32>,
    pub stdout_artifact_hash: Option<String>,
    pub stderr_artifact_hash: Option<String>,
    pub duration_milliseconds: u64,
    pub observed_toolchain_version: Option<String>,
    pub forbidden_source_token: Option<String>,
    pub trust_profile: TrustProfile,
    pub memory_limit_enforced: bool,
    pub network_isolation_enforced: bool,
    pub authoritative: bool,
}

impl VerifierJobRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != VERIFIER_REQUEST_SCHEMA_VERSION {
            return Err(verifier_request_error(
                format!(
                    "verifier request schema must be `{VERIFIER_REQUEST_SCHEMA_VERSION}`, received `{}`",
                    self.schema_version
                ),
                "Use the committed verifier request schema.",
            ));
        }
        validate_hash(&self.environment_hash, "environment")?;
        validate_hash(&self.module_artifact_hash, "module artifact")?;
        if self.declaration_name.is_empty()
            || self.declaration_name.len() > 256
            || !self.declaration_name.split('.').all(|segment| {
                !segment.is_empty()
                    && segment
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            })
        {
            return Err(verifier_request_error(
                format!("unsafe Lean declaration name `{}`", self.declaration_name),
                "Use a dotted Lean declaration name without paths, whitespace, or shell characters.",
            ));
        }
        Ok(())
    }
}

impl VerifierExecutionReport {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != VERIFIER_EXECUTION_REPORT_SCHEMA_VERSION
            || uuid::Uuid::parse_str(&self.job_id).is_err()
            || !is_hash(&self.environment_hash)
            || !is_hash(&self.module_artifact_hash)
            || self
                .stdout_artifact_hash
                .as_deref()
                .is_some_and(|hash| !is_hash(hash))
            || self
                .stderr_artifact_hash
                .as_deref()
                .is_some_and(|hash| !is_hash(hash))
            || self
                .observed_toolchain_version
                .as_deref()
                .is_some_and(|version| version.is_empty() || version.len() > 256)
            || self
                .forbidden_source_token
                .as_deref()
                .is_some_and(|token| token.is_empty() || token.len() > 64)
            || self.authoritative
            || self.declaration_name.is_empty()
            || self.declaration_name.len() > 256
            || !self.declaration_name.split('.').all(|segment| {
                !segment.is_empty()
                    && segment
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            })
        {
            return Err(AppError::new(
                "MCL_VERIFIER_REPORT_INVALID",
                "verifier execution report does not satisfy the closed canonical contract",
                false,
                "Quarantine the verifier result and rerun the exact job.",
            ));
        }
        Ok(())
    }
}

pub fn verifier_request_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/verifier/request/1",
        "title": "MathOS Lean Verifier Job Request v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "environment_hash", "module_artifact_hash", "declaration_name"],
        "properties": {
            "schema_version": {"const": VERIFIER_REQUEST_SCHEMA_VERSION},
            "environment_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "module_artifact_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "declaration_name": {"type": "string", "minLength": 1, "maxLength": 256, "pattern": "^[A-Za-z0-9_]+(\\.[A-Za-z0-9_]+)*$"}
        }
    })
}

pub fn verifier_execution_report_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/verifier/execution-report/1",
        "title": "MathOS Lean Verifier Execution Report v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "job_id", "environment_hash", "module_artifact_hash", "declaration_name", "classification", "exit_code", "stdout_artifact_hash", "stderr_artifact_hash", "duration_milliseconds", "observed_toolchain_version", "forbidden_source_token", "trust_profile", "memory_limit_enforced", "network_isolation_enforced", "authoritative"],
        "properties": {
            "schema_version": {"const": VERIFIER_EXECUTION_REPORT_SCHEMA_VERSION},
            "job_id": {"type": "string", "format": "uuid"},
            "environment_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "module_artifact_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "declaration_name": {"type": "string", "minLength": 1, "maxLength": 256},
            "classification": {"enum": ["elaborated", "rejected", "timed_out", "output_limit_exceeded", "toolchain_mismatch", "launch_failed", "unsafe_source"]},
            "exit_code": {"type": ["integer", "null"]},
            "stdout_artifact_hash": {"type": ["string", "null"], "pattern": "^[0-9a-f]{64}$"},
            "stderr_artifact_hash": {"type": ["string", "null"], "pattern": "^[0-9a-f]{64}$"},
            "duration_milliseconds": {"type": "integer", "minimum": 0},
            "observed_toolchain_version": {"type": ["string", "null"], "minLength": 1, "maxLength": 256},
            "forbidden_source_token": {"type": ["string", "null"], "minLength": 1, "maxLength": 64},
            "trust_profile": {"enum": ["local", "publication"]},
            "memory_limit_enforced": {"type": "boolean"},
            "network_isolation_enforced": {"type": "boolean"},
            "authoritative": {"const": false}
        }
    })
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn validate_hash(hash: &str, label: &str) -> Result<(), AppError> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(verifier_request_error(
            format!("verifier {label} must be a lowercase SHA-256 identity"),
            "Use an exact hash returned by canonical lookup.",
        ));
    }
    Ok(())
}

fn verifier_request_error(message: impl Into<String>, action: impl Into<String>) -> AppError {
    AppError::new("MCL_VERIFIER_REQUEST_INVALID", message, false, action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> VerifierJobRequest {
        VerifierJobRequest {
            schema_version: VERIFIER_REQUEST_SCHEMA_VERSION.to_owned(),
            environment_hash: "a".repeat(64),
            module_artifact_hash: "b".repeat(64),
            declaration_name: "MathOS.Pilot.truth".to_owned(),
        }
    }

    #[test]
    fn verifier_request_is_closed_and_contains_no_command_surface() {
        request().validate().expect("valid request");
        let mut unknown = serde_json::to_value(request()).expect("request JSON");
        unknown["command"] = json!("sh -c anything");
        assert!(serde_json::from_value::<VerifierJobRequest>(unknown).is_err());

        let mut unsafe_name = request();
        unsafe_name.declaration_name = "Truth; rm".to_owned();
        assert_eq!(
            unsafe_name
                .validate()
                .expect_err("shell-shaped name rejected")
                .code,
            "MCL_VERIFIER_REQUEST_INVALID"
        );
    }

    #[test]
    fn committed_schema_matches_the_closed_rust_contract() {
        let committed: Value = serde_json::from_str(include_str!(
            "../../schemas/verifier/verifier-request-1.schema.json"
        ))
        .expect("committed verifier request schema");
        assert_eq!(committed, verifier_request_schema());

        let report: Value = serde_json::from_str(include_str!(
            "../../schemas/verifier/verifier-execution-report-1.schema.json"
        ))
        .expect("committed verifier execution report schema");
        assert_eq!(report, verifier_execution_report_schema());
    }

    #[test]
    fn execution_reports_reject_wrong_schema_and_noncanonical_hashes() {
        let mut report = VerifierExecutionReport {
            schema_version: VERIFIER_EXECUTION_REPORT_SCHEMA_VERSION.to_owned(),
            job_id: uuid::Uuid::now_v7().to_string(),
            environment_hash: "a".repeat(64),
            module_artifact_hash: "b".repeat(64),
            declaration_name: "MathOS.Pilot.truth".to_owned(),
            classification: VerifierExecutionClassification::Elaborated,
            exit_code: Some(0),
            stdout_artifact_hash: None,
            stderr_artifact_hash: None,
            duration_milliseconds: 1,
            observed_toolchain_version: Some("Lean 4.32.0".to_owned()),
            forbidden_source_token: None,
            trust_profile: TrustProfile::Local,
            memory_limit_enforced: false,
            network_isolation_enforced: false,
            authoritative: false,
        };
        report.validate().expect("closed report validates");

        report.schema_version = "verifier_execution_report/2".to_owned();
        assert_eq!(
            report
                .validate()
                .expect_err("unknown report schema rejected")
                .code,
            "MCL_VERIFIER_REPORT_INVALID"
        );
        report.schema_version = VERIFIER_EXECUTION_REPORT_SCHEMA_VERSION.to_owned();
        report.environment_hash = "A".repeat(64);
        assert_eq!(
            report
                .validate()
                .expect_err("noncanonical hash rejected")
                .code,
            "MCL_VERIFIER_REPORT_INVALID"
        );
    }
}
