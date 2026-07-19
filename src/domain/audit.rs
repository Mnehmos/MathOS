use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canonical::value_hash;
use crate::domain::TrustProfile;
use crate::domain::schemas::ExactVersionReference;
use crate::domain::verifier::VerifierJobState;
use crate::error::AppError;

pub const AUDIT_POLICY_SCHEMA_VERSION: &str = "audit_policy/1";
pub const AUDIT_REQUEST_SCHEMA_VERSION: &str = "audit_request/1";
pub const AUDIT_REPORT_SCHEMA_VERSION: &str = "audit_report/1";
const MAX_AXIOMS: usize = 256;
const MAX_TOKENS: usize = 64;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LeanAuditPolicy {
    pub schema_version: String,
    pub policy_name: String,
    pub allowed_axioms: Vec<String>,
    pub forbidden_source_tokens: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LeanAuditRequest {
    pub schema_version: String,
    pub subject: ExactVersionReference,
    pub diagnostic_evidence_id: String,
    pub diagnostic_evidence_hash: String,
    pub environment_hash: String,
    pub module_artifact_hash: String,
    pub declaration_name: String,
    pub policy_hash: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeanAuditClassification {
    Passed,
    Rejected,
    Inconclusive,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LeanAuditReport {
    pub schema_version: String,
    pub job_id: String,
    pub request_hash: String,
    pub subject: ExactVersionReference,
    pub diagnostic_evidence_hash: String,
    pub environment_hash: String,
    pub module_artifact_hash: String,
    pub declaration_name: String,
    pub policy_hash: String,
    pub classification: LeanAuditClassification,
    pub source_forbidden_token: Option<String>,
    pub observed_axioms: Vec<String>,
    pub unexpected_axioms: Vec<String>,
    pub stdout_artifact_hash: Option<String>,
    pub stderr_artifact_hash: Option<String>,
    pub observed_toolchain_version: Option<String>,
    pub trust_profile: TrustProfile,
    pub dependency_closure_complete: bool,
    pub memory_limit_enforced: bool,
    pub network_isolation_enforced: bool,
    pub authoritative: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LeanAuditJobSnapshot {
    pub job_id: String,
    pub request: LeanAuditRequest,
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

impl LeanAuditPolicy {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != AUDIT_POLICY_SCHEMA_VERSION
            || self.policy_name.trim().is_empty()
            || self.policy_name.len() > 128
            || !is_sorted_unique_bounded(&self.allowed_axioms, MAX_AXIOMS)
            || !is_sorted_unique_bounded(&self.forbidden_source_tokens, MAX_TOKENS)
            || self.allowed_axioms.iter().any(|name| !is_lean_name(name))
            || self
                .forbidden_source_tokens
                .iter()
                .any(|token| !is_lean_name(token))
        {
            return Err(audit_error(
                "MCL_AUDIT_POLICY_INVALID",
                "Lean audit policy does not satisfy the closed canonical contract",
                "Use the committed sorted and bounded audit policy.",
            ));
        }
        Ok(())
    }

    pub fn policy_hash(&self) -> Result<String, AppError> {
        self.validate()?;
        value_hash(&serde_json::to_value(self).map_err(|error| {
            audit_error(
                "MCL_AUDIT_POLICY_INVALID",
                error.to_string(),
                "Report this deterministic audit policy serialization defect.",
            )
        })?)
    }
}

impl LeanAuditRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != AUDIT_REQUEST_SCHEMA_VERSION
            || uuid::Uuid::parse_str(&self.subject.object_id).is_err()
            || uuid::Uuid::parse_str(&self.diagnostic_evidence_id).is_err()
            || !is_hash(&self.subject.version_hash)
            || !is_hash(&self.diagnostic_evidence_hash)
            || !is_hash(&self.environment_hash)
            || !is_hash(&self.module_artifact_hash)
            || !is_hash(&self.policy_hash)
            || !is_lean_name(&self.declaration_name)
            || self.declaration_name.len() > 256
        {
            return Err(audit_error(
                "MCL_AUDIT_REQUEST_INVALID",
                "Lean audit request does not satisfy the closed exact-reference contract",
                "Resolve an exact formalization, diagnostic evidence record, and committed audit policy.",
            ));
        }
        Ok(())
    }

    pub fn request_hash(&self) -> Result<String, AppError> {
        self.validate()?;
        value_hash(&serde_json::to_value(self).map_err(|error| {
            audit_error(
                "MCL_AUDIT_REQUEST_INVALID",
                error.to_string(),
                "Report this deterministic audit request serialization defect.",
            )
        })?)
    }
}

impl LeanAuditReport {
    pub fn validate(&self) -> Result<(), AppError> {
        let passed_shape = self.classification != LeanAuditClassification::Passed
            || (self.source_forbidden_token.is_none()
                && self.unexpected_axioms.is_empty()
                && self.dependency_closure_complete);
        let rejected_shape = self.classification != LeanAuditClassification::Rejected
            || self.source_forbidden_token.is_some()
            || !self.unexpected_axioms.is_empty();
        if self.schema_version != AUDIT_REPORT_SCHEMA_VERSION
            || uuid::Uuid::parse_str(&self.job_id).is_err()
            || uuid::Uuid::parse_str(&self.subject.object_id).is_err()
            || !is_hash(&self.request_hash)
            || !is_hash(&self.subject.version_hash)
            || !is_hash(&self.diagnostic_evidence_hash)
            || !is_hash(&self.environment_hash)
            || !is_hash(&self.module_artifact_hash)
            || !is_hash(&self.policy_hash)
            || !is_lean_name(&self.declaration_name)
            || self.declaration_name.len() > 256
            || !is_sorted_unique_bounded(&self.observed_axioms, MAX_AXIOMS)
            || !is_sorted_unique_bounded(&self.unexpected_axioms, MAX_AXIOMS)
            || self.observed_axioms.iter().any(|name| !is_lean_name(name))
            || self
                .unexpected_axioms
                .iter()
                .any(|name| !is_lean_name(name))
            || self
                .unexpected_axioms
                .iter()
                .any(|name| self.observed_axioms.binary_search(name).is_err())
            || self
                .source_forbidden_token
                .as_deref()
                .is_some_and(|token| !is_lean_name(token) || token.len() > 64)
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
            || self.authoritative
            || !passed_shape
            || !rejected_shape
        {
            return Err(audit_error(
                "MCL_AUDIT_REPORT_INVALID",
                "Lean audit report does not satisfy the closed non-authoritative contract",
                "Quarantine the audit result and rerun the exact audit job.",
            ));
        }
        Ok(())
    }

    pub fn validate_against_policy(&self, policy: &LeanAuditPolicy) -> Result<(), AppError> {
        self.validate()?;
        policy.validate()?;
        let expected_policy_hash = policy.policy_hash()?;
        let expected_unexpected = self
            .observed_axioms
            .iter()
            .filter(|axiom| policy.allowed_axioms.binary_search(axiom).is_err())
            .cloned()
            .collect::<Vec<_>>();
        let source_token_allowed = self
            .source_forbidden_token
            .as_ref()
            .is_none_or(|token| policy.forbidden_source_tokens.binary_search(token).is_ok());
        let classification_consistent = match self.classification {
            LeanAuditClassification::Passed => {
                self.source_forbidden_token.is_none()
                    && expected_unexpected.is_empty()
                    && self.dependency_closure_complete
            }
            LeanAuditClassification::Rejected => {
                self.source_forbidden_token.is_some() || !expected_unexpected.is_empty()
            }
            LeanAuditClassification::Inconclusive | LeanAuditClassification::Failed => true,
        };
        if self.policy_hash != expected_policy_hash
            || self.unexpected_axioms != expected_unexpected
            || !source_token_allowed
            || !classification_consistent
        {
            return Err(audit_error(
                "MCL_AUDIT_POLICY_RESULT_INVALID",
                "Lean audit report does not reproduce the committed policy decision",
                "Quarantine the audit result and recompute it from the exact observed axioms and source scan.",
            ));
        }
        Ok(())
    }
}

pub fn audit_policy_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/audit/policy/1",
        "title": "MathOS Lean Audit Policy v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "policy_name", "allowed_axioms", "forbidden_source_tokens"],
        "properties": {
            "schema_version": {"const": AUDIT_POLICY_SCHEMA_VERSION},
            "policy_name": {"type": "string", "minLength": 1, "maxLength": 128},
            "allowed_axioms": {"type": "array", "maxItems": MAX_AXIOMS, "items": {"type": "string", "minLength": 1, "maxLength": 256}},
            "forbidden_source_tokens": {"type": "array", "maxItems": MAX_TOKENS, "items": {"type": "string", "minLength": 1, "maxLength": 64}}
        }
    })
}

pub fn audit_request_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/audit/request/1",
        "title": "MathOS Lean Audit Request v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "subject", "diagnostic_evidence_id", "diagnostic_evidence_hash", "environment_hash", "module_artifact_hash", "declaration_name", "policy_hash"],
        "properties": {
            "schema_version": {"const": AUDIT_REQUEST_SCHEMA_VERSION},
            "subject": {"type": "object", "additionalProperties": false, "required": ["object_id", "version_hash"], "properties": {"object_id": {"type": "string", "format": "uuid"}, "version_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}}},
            "diagnostic_evidence_id": {"type": "string", "format": "uuid"},
            "diagnostic_evidence_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "environment_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "module_artifact_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "declaration_name": {"type": "string", "minLength": 1, "maxLength": 256},
            "policy_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
        }
    })
}

pub fn audit_report_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/audit/report/1",
        "title": "MathOS Lean Audit Report v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "job_id", "request_hash", "subject", "diagnostic_evidence_hash", "environment_hash", "module_artifact_hash", "declaration_name", "policy_hash", "classification", "source_forbidden_token", "observed_axioms", "unexpected_axioms", "stdout_artifact_hash", "stderr_artifact_hash", "observed_toolchain_version", "trust_profile", "dependency_closure_complete", "memory_limit_enforced", "network_isolation_enforced", "authoritative"],
        "properties": {
            "schema_version": {"const": AUDIT_REPORT_SCHEMA_VERSION},
            "job_id": {"type": "string", "format": "uuid"},
            "request_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "subject": {"type": "object", "additionalProperties": false, "required": ["object_id", "version_hash"], "properties": {"object_id": {"type": "string", "format": "uuid"}, "version_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}}},
            "diagnostic_evidence_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "environment_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "module_artifact_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "declaration_name": {"type": "string", "minLength": 1, "maxLength": 256},
            "policy_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "classification": {"enum": ["passed", "rejected", "inconclusive", "failed"]},
            "source_forbidden_token": {"type": ["string", "null"], "minLength": 1, "maxLength": 64},
            "observed_axioms": {"type": "array", "maxItems": MAX_AXIOMS, "items": {"type": "string", "minLength": 1, "maxLength": 256}},
            "unexpected_axioms": {"type": "array", "maxItems": MAX_AXIOMS, "items": {"type": "string", "minLength": 1, "maxLength": 256}},
            "stdout_artifact_hash": {"type": ["string", "null"], "pattern": "^[0-9a-f]{64}$"},
            "stderr_artifact_hash": {"type": ["string", "null"], "pattern": "^[0-9a-f]{64}$"},
            "observed_toolchain_version": {"type": ["string", "null"], "minLength": 1, "maxLength": 256},
            "trust_profile": {"enum": ["local", "publication"]},
            "dependency_closure_complete": {"type": "boolean"},
            "memory_limit_enforced": {"type": "boolean"},
            "network_isolation_enforced": {"type": "boolean"},
            "authoritative": {"const": false}
        }
    })
}

pub fn committed_audit_policy() -> Result<LeanAuditPolicy, AppError> {
    let policy: LeanAuditPolicy = serde_json::from_str(include_str!(
        "../../policies/lean-local-audit-1.json"
    ))
    .map_err(|error| {
        audit_error(
            "MCL_AUDIT_POLICY_INVALID",
            error.to_string(),
            "Repair the committed audit policy before building or running audits.",
        )
    })?;
    policy.validate()?;
    Ok(policy)
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_lean_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\''))
        })
}

fn is_sorted_unique_bounded(values: &[String], limit: usize) -> bool {
    values.len() <= limit && !values.windows(2).any(|pair| pair[0] >= pair[1])
}

fn audit_error(
    code: &'static str,
    message: impl Into<String>,
    action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject() -> ExactVersionReference {
        ExactVersionReference {
            object_id: uuid::Uuid::now_v7().to_string(),
            version_hash: "a".repeat(64),
        }
    }

    fn report() -> LeanAuditReport {
        let policy = committed_audit_policy().expect("policy");
        LeanAuditReport {
            schema_version: AUDIT_REPORT_SCHEMA_VERSION.to_owned(),
            job_id: uuid::Uuid::now_v7().to_string(),
            request_hash: "b".repeat(64),
            subject: subject(),
            diagnostic_evidence_hash: "c".repeat(64),
            environment_hash: "d".repeat(64),
            module_artifact_hash: "e".repeat(64),
            declaration_name: "MathOS.truth".to_owned(),
            policy_hash: policy.policy_hash().expect("policy hash"),
            classification: LeanAuditClassification::Passed,
            source_forbidden_token: None,
            observed_axioms: vec!["Classical.choice".to_owned()],
            unexpected_axioms: Vec::new(),
            stdout_artifact_hash: None,
            stderr_artifact_hash: None,
            observed_toolchain_version: Some("Lean 4.32.0".to_owned()),
            trust_profile: TrustProfile::Local,
            dependency_closure_complete: true,
            memory_limit_enforced: false,
            network_isolation_enforced: false,
            authoritative: false,
        }
    }

    #[test]
    fn committed_policy_is_narrow_sorted_and_content_identified() {
        let policy = committed_audit_policy().expect("committed policy validates");
        assert_eq!(
            policy.allowed_axioms,
            ["Classical.choice", "Quot.sound", "propext"]
        );
        assert!(!policy.allowed_axioms.iter().any(|axiom| {
            matches!(
                axiom.as_str(),
                "sorryAx" | "Lean.trustCompiler" | "Lean.ofReduceBool"
            )
        }));
        assert_eq!(
            policy.forbidden_source_tokens,
            crate::verifier::FORBIDDEN_SOURCE_TOKENS
        );
        assert_eq!(policy.policy_hash().expect("policy hash").len(), 64);
    }

    #[test]
    fn committed_schemas_match_closed_rust_contracts() {
        let policy: Value = serde_json::from_str(include_str!(
            "../../schemas/audit/audit-policy-1.schema.json"
        ))
        .expect("policy schema");
        let request: Value = serde_json::from_str(include_str!(
            "../../schemas/audit/audit-request-1.schema.json"
        ))
        .expect("request schema");
        let report: Value = serde_json::from_str(include_str!(
            "../../schemas/audit/audit-report-1.schema.json"
        ))
        .expect("report schema");
        assert_eq!(policy, audit_policy_schema());
        assert_eq!(request, audit_request_schema());
        assert_eq!(report, audit_report_schema());
    }

    #[test]
    fn audit_report_cannot_claim_authority_or_hide_unexpected_axioms() {
        let policy = committed_audit_policy().expect("policy");
        report()
            .validate_against_policy(&policy)
            .expect("diagnostic audit report validates");
        let mut authority = report();
        authority.authoritative = true;
        assert_eq!(
            authority
                .validate()
                .expect_err("audit cannot grant itself authority")
                .code,
            "MCL_AUDIT_REPORT_INVALID"
        );

        let mut hidden = report();
        hidden.classification = LeanAuditClassification::Rejected;
        hidden.unexpected_axioms = vec!["fabricated".to_owned()];
        assert_eq!(
            hidden
                .validate()
                .expect_err("unexpected axiom must be observed")
                .code,
            "MCL_AUDIT_REPORT_INVALID"
        );

        let mut hidden_observation = report();
        hidden_observation
            .observed_axioms
            .push("sorryAx".to_owned());
        hidden_observation.observed_axioms.sort();
        assert_eq!(
            hidden_observation
                .validate_against_policy(&policy)
                .expect_err("observed unexpected axiom cannot be omitted from policy result")
                .code,
            "MCL_AUDIT_POLICY_RESULT_INVALID"
        );

        let mut explicit_rejection = report();
        explicit_rejection.classification = LeanAuditClassification::Rejected;
        explicit_rejection
            .observed_axioms
            .push("sorryAx".to_owned());
        explicit_rejection.observed_axioms.sort();
        explicit_rejection.unexpected_axioms = vec!["sorryAx".to_owned()];
        explicit_rejection
            .validate_against_policy(&policy)
            .expect("unexpected axiom remains explicit and rejected");

        let mut incomplete = report();
        incomplete.dependency_closure_complete = false;
        assert_eq!(
            incomplete
                .validate()
                .expect_err("incomplete closure cannot pass")
                .code,
            "MCL_AUDIT_REPORT_INVALID"
        );
    }
}
