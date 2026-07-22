use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::canonical::{canonical_json, value_hash};
use crate::domain::{ArtifactRestriction, ReleaseProfile};
use crate::error::AppError;

pub const RL_EXPORT_PLAN_SCHEMA_VERSION: &str = "rl_export_plan/1";
pub const RL_TASK_SCHEMA_VERSION: &str = "rl_task/1";
pub const RL_LEAKAGE_REPORT_SCHEMA_VERSION: &str = "rl_leakage_report/1";
pub const RL_EXPORT_MANIFEST_SCHEMA_VERSION: &str = "rl_export_manifest/1";
pub const MAX_RL_RELEASES: usize = 64;
pub const MAX_RL_MEMBERS: usize = 8_192;
pub const MAX_RL_MEMBER_BYTES: u64 = 256 * 1_048_576;
pub const MAX_RL_TOTAL_BYTES: u64 = 2 * 1_073_741_824;

const MAX_LABELS: usize = 128;
const MAX_LABEL_BYTES: usize = 256;
const MAX_TASK_BYTES: usize = 4 * 1_048_576;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RlSplit {
    Train,
    Validation,
    PublicTest,
    HeldOutEvaluation,
}

impl RlSplit {
    pub const ALL: [Self; 4] = [
        Self::Train,
        Self::Validation,
        Self::PublicTest,
        Self::HeldOutEvaluation,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Train => "train",
            Self::Validation => "validation",
            Self::PublicTest => "public_test",
            Self::HeldOutEvaluation => "held_out_evaluation",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RlTaskFamily {
    Formalization,
    FidelitySelection,
    Counterexample,
    StatementRepair,
    DeclarationRetrieval,
    Decomposition,
    ProofGeneration,
    ProofRepair,
    Generalization,
    Explanation,
    CurriculumOrdering,
    FrontierSelection,
}

impl RlTaskFamily {
    pub const ALL: [Self; 12] = [
        Self::Formalization,
        Self::FidelitySelection,
        Self::Counterexample,
        Self::StatementRepair,
        Self::DeclarationRetrieval,
        Self::Decomposition,
        Self::ProofGeneration,
        Self::ProofRepair,
        Self::Generalization,
        Self::Explanation,
        Self::CurriculumOrdering,
        Self::FrontierSelection,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Formalization => "formalization",
            Self::FidelitySelection => "fidelity_selection",
            Self::Counterexample => "counterexample",
            Self::StatementRepair => "statement_repair",
            Self::DeclarationRetrieval => "declaration_retrieval",
            Self::Decomposition => "decomposition",
            Self::ProofGeneration => "proof_generation",
            Self::ProofRepair => "proof_repair",
            Self::Generalization => "generalization",
            Self::Explanation => "explanation",
            Self::CurriculumOrdering => "curriculum_ordering",
            Self::FrontierSelection => "frontier_selection",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlLeakageLabels {
    pub theorem_dependency_components: Vec<String>,
    pub equivalent_formalizations: Vec<String>,
    pub shared_sources: Vec<String>,
    pub certificate_families: Vec<String>,
    pub proof_variants: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlPlanRelease {
    pub release_id: String,
    pub expected_manifest_hash: String,
    pub split: RlSplit,
    pub published_on: String,
    pub benchmark_identity: String,
    pub leakage_labels: RlLeakageLabels,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlExportPlan {
    pub schema_version: String,
    pub publication_cutoff: String,
    pub releases: Vec<RlPlanRelease>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlTaskEvidenceReference {
    pub path: String,
    pub content_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlTaskTrust {
    pub source_release_id: String,
    pub source_release_manifest_hash: String,
    pub publication_receipt_hash: String,
    pub authority_evidence_ids: Vec<String>,
    pub fidelity_evidence_ids: Vec<String>,
    pub kernel_verified: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlTaskPolicy {
    pub restriction: ArtifactRestriction,
    pub license_expressions: Vec<String>,
    pub license_complete: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlTask {
    pub schema_version: String,
    pub task_id: String,
    pub family: RlTaskFamily,
    pub split: RlSplit,
    pub leakage_component_id: String,
    pub input: Value,
    pub target: Value,
    pub evidence: Vec<RlTaskEvidenceReference>,
    pub trust: RlTaskTrust,
    pub policy: RlTaskPolicy,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlLeakageComponent {
    pub component_id: String,
    pub split: RlSplit,
    pub release_ids: Vec<String>,
    pub leakage_keys: Vec<String>,
    pub task_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlTaskFamilySummary {
    pub family: RlTaskFamily,
    pub emitted_task_count: u64,
    pub skip_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlLeakageReport {
    pub schema_version: String,
    pub plan_hash: String,
    pub components: Vec<RlLeakageComponent>,
    pub task_families: Vec<RlTaskFamilySummary>,
    pub cross_split_overlap_count: u64,
    pub temporal_policy_verified: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RlExportMemberKind {
    Plan,
    LeakageReport,
    Task,
    Artifact,
    SourceReleaseManifest,
    SourceMember,
    Schema,
}

impl RlExportMemberKind {
    pub const fn directory(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::LeakageReport => "leakage",
            Self::Task => "tasks",
            Self::Artifact => "artifacts",
            Self::SourceReleaseManifest => "source-releases",
            Self::SourceMember => "source-members",
            Self::Schema => "schemas",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlExportMember {
    pub path: String,
    pub kind: RlExportMemberKind,
    pub content_hash: String,
    pub byte_size: u64,
    pub license_expression: Option<String>,
    pub restriction: ArtifactRestriction,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlExportSourceBinding {
    pub release_id: String,
    pub release_manifest_hash: String,
    pub release_profile: ReleaseProfile,
    pub split: RlSplit,
    pub published_on: String,
    pub leakage_component_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlExportManifest {
    pub schema_version: String,
    pub plan_hash: String,
    pub publication_cutoff: String,
    pub source_releases: Vec<RlExportSourceBinding>,
    pub leakage_report_sha256: String,
    pub task_count: u64,
    pub component_count: u64,
    pub members: Vec<RlExportMember>,
}

impl RlExportPlan {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != RL_EXPORT_PLAN_SCHEMA_VERSION
            || !valid_date(&self.publication_cutoff)
            || self.releases.is_empty()
            || self.releases.len() > MAX_RL_RELEASES
        {
            return Err(rl_error(
                "MCL_RL_PLAN_INVALID",
                "RL export plan has an unsupported schema, invalid cutoff, or release count",
            ));
        }
        let mut previous = None;
        let mut hashes = BTreeSet::new();
        for release in &self.releases {
            release.validate(&self.publication_cutoff)?;
            if previous.is_some_and(|id: &str| id >= release.release_id.as_str())
                || !hashes.insert(release.expected_manifest_hash.as_str())
            {
                return Err(rl_error(
                    "MCL_RL_PLAN_INVALID",
                    "RL plan releases must be sorted by unique ID and bind unique release hashes",
                ));
            }
            previous = Some(release.release_id.as_str());
        }
        Ok(())
    }

    pub fn plan_hash(&self) -> Result<String, AppError> {
        self.validate()?;
        value_hash(&serde_json::to_value(self).map_err(serialization_error)?)
    }
}

impl RlPlanRelease {
    fn validate(&self, cutoff: &str) -> Result<(), AppError> {
        if !valid_release_id(&self.release_id)
            || !is_hash(&self.expected_manifest_hash)
            || !valid_date(&self.published_on)
            || !valid_label(&self.benchmark_identity)
        {
            return Err(rl_error(
                "MCL_RL_PLAN_RELEASE_INVALID",
                "RL plan release identity, hash, date, or benchmark is invalid",
            ));
        }
        let temporal_ok = match self.split {
            RlSplit::Train => self.published_on.as_str() <= cutoff,
            RlSplit::Validation | RlSplit::PublicTest | RlSplit::HeldOutEvaluation => {
                self.published_on.as_str() > cutoff
            }
        };
        if !temporal_ok {
            return Err(rl_error(
                "MCL_RL_TEMPORAL_LEAKAGE",
                "RL split assignment violates the declared publication cutoff",
            ));
        }
        self.leakage_labels.validate()
    }
}

impl RlLeakageLabels {
    fn validate(&self) -> Result<(), AppError> {
        for labels in [
            &self.theorem_dependency_components,
            &self.equivalent_formalizations,
            &self.shared_sources,
            &self.certificate_families,
            &self.proof_variants,
        ] {
            if labels.is_empty()
                || labels.len() > MAX_LABELS
                || labels.iter().any(|label| !valid_label(label))
                || labels.windows(2).any(|pair| pair[0] >= pair[1])
            {
                return Err(rl_error(
                    "MCL_RL_LEAKAGE_LABELS_INVALID",
                    "every leakage dimension must be explicitly populated, sorted, unique, and bounded",
                ));
            }
        }
        Ok(())
    }
}

impl RlTask {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != RL_TASK_SCHEMA_VERSION
            || !is_prefixed_hash(&self.task_id, "rl_task_")
            || !is_prefixed_hash(&self.leakage_component_id, "rl_component_")
            || !self.input.is_object()
            || !self.target.is_object()
            || contains_private_reasoning(&self.input)
            || contains_private_reasoning(&self.target)
            || self.evidence.is_empty()
            || self.evidence.len() > 512
        {
            return Err(rl_error(
                "MCL_RL_TASK_INVALID",
                "RL task identity, payload, evidence, or reasoning policy is invalid",
            ));
        }
        let bytes = canonical_json(&serde_json::to_value(self).map_err(serialization_error)?)?;
        if bytes.len() > MAX_TASK_BYTES {
            return Err(rl_error(
                "MCL_RL_TASK_INVALID",
                "RL task exceeds its closed canonical byte bound",
            ));
        }
        let mut previous = None;
        for evidence in &self.evidence {
            if !is_safe_relative_path(&evidence.path)
                || !is_hash(&evidence.content_hash)
                || previous.is_some_and(|path: &str| path >= evidence.path.as_str())
            {
                return Err(rl_error(
                    "MCL_RL_TASK_EVIDENCE_INVALID",
                    "RL task evidence references must be exact, safe, sorted, and unique",
                ));
            }
            previous = Some(evidence.path.as_str());
        }
        self.trust.validate()?;
        self.policy.validate()?;
        if self.expected_task_id()? != self.task_id {
            return Err(rl_error(
                "MCL_RL_TASK_HASH_MISMATCH",
                "RL task ID differs from its canonical task body",
            ));
        }
        Ok(())
    }

    pub fn expected_task_id(&self) -> Result<String, AppError> {
        let mut value = serde_json::to_value(self).map_err(serialization_error)?;
        value
            .as_object_mut()
            .expect("RL task serializes as object")
            .remove("task_id");
        Ok(format!("rl_task_{}", value_hash(&value)?))
    }
}

impl RlTaskTrust {
    fn validate(&self) -> Result<(), AppError> {
        if !valid_release_id(&self.source_release_id)
            || !is_hash(&self.source_release_manifest_hash)
            || !is_hash(&self.publication_receipt_hash)
            || !self.kernel_verified
            || !valid_sorted_uuids(&self.authority_evidence_ids)
            || !valid_sorted_uuids(&self.fidelity_evidence_ids)
        {
            return Err(rl_error(
                "MCL_RL_TASK_TRUST_INVALID",
                "RL task lacks exact replayed authority or fidelity bindings",
            ));
        }
        Ok(())
    }
}

impl RlTaskPolicy {
    fn validate(&self) -> Result<(), AppError> {
        if self
            .license_expressions
            .iter()
            .any(|license| !valid_label(license))
            || self
                .license_expressions
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || (self.restriction == ArtifactRestriction::Public
                && (!self.license_complete || self.license_expressions.is_empty()))
        {
            return Err(rl_error(
                "MCL_RL_TASK_POLICY_INVALID",
                "public RL tasks require a complete sorted nonempty license set",
            ));
        }
        Ok(())
    }
}

impl RlLeakageReport {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != RL_LEAKAGE_REPORT_SCHEMA_VERSION
            || !is_hash(&self.plan_hash)
            || self.components.is_empty()
            || self.components.len() > MAX_RL_RELEASES
            || self.cross_split_overlap_count != 0
            || !self.temporal_policy_verified
            || self.task_families.len() != RlTaskFamily::ALL.len()
        {
            return Err(rl_error(
                "MCL_RL_LEAKAGE_REPORT_INVALID",
                "RL leakage report is incomplete or reports unresolved leakage",
            ));
        }
        let mut previous = None;
        let mut task_ids = BTreeSet::new();
        for component in &self.components {
            component.validate()?;
            if previous.is_some_and(|id: &str| id >= component.component_id.as_str())
                || component
                    .task_ids
                    .iter()
                    .any(|task_id| !task_ids.insert(task_id.as_str()))
            {
                return Err(rl_error(
                    "MCL_RL_LEAKAGE_REPORT_INVALID",
                    "RL leakage components or task assignments are duplicated or unsorted",
                ));
            }
            previous = Some(component.component_id.as_str());
        }
        for (summary, family) in self.task_families.iter().zip(RlTaskFamily::ALL) {
            if summary.family != family
                || (summary.emitted_task_count == 0) != summary.skip_reason.is_some()
                || summary
                    .skip_reason
                    .as_deref()
                    .is_some_and(|reason| !valid_label(reason))
            {
                return Err(rl_error(
                    "MCL_RL_LEAKAGE_REPORT_INVALID",
                    "RL task-family audit is incomplete or inconsistent",
                ));
            }
        }
        Ok(())
    }
}

impl RlLeakageComponent {
    fn validate(&self) -> Result<(), AppError> {
        if !is_prefixed_hash(&self.component_id, "rl_component_")
            || self.release_ids.is_empty()
            || self.leakage_keys.is_empty()
            || self.release_ids.iter().any(|id| !valid_release_id(id))
            || self.release_ids.windows(2).any(|pair| pair[0] >= pair[1])
            || self.leakage_keys.iter().any(|key| !valid_label(key))
            || self.leakage_keys.windows(2).any(|pair| pair[0] >= pair[1])
            || self
                .task_ids
                .iter()
                .any(|id| !is_prefixed_hash(id, "rl_task_"))
            || self.task_ids.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(rl_error(
                "MCL_RL_LEAKAGE_COMPONENT_INVALID",
                "RL leakage component identity, releases, keys, or tasks are invalid",
            ));
        }
        Ok(())
    }
}

impl RlExportManifest {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != RL_EXPORT_MANIFEST_SCHEMA_VERSION
            || !is_hash(&self.plan_hash)
            || !valid_date(&self.publication_cutoff)
            || !is_hash(&self.leakage_report_sha256)
            || self.source_releases.is_empty()
            || self.source_releases.len() > MAX_RL_RELEASES
            || self.component_count == 0
            || self.component_count > self.source_releases.len() as u64
            || self.members.is_empty()
            || self.members.len() > MAX_RL_MEMBERS
        {
            return Err(rl_error(
                "MCL_RL_EXPORT_MANIFEST_INVALID",
                "RL export manifest has invalid identities, counts, or schema",
            ));
        }
        let mut previous_source = None;
        let mut source_hashes = BTreeSet::new();
        for source in &self.source_releases {
            source.validate()?;
            if previous_source.is_some_and(|id: &str| id >= source.release_id.as_str())
                || !source_hashes.insert(source.release_manifest_hash.as_str())
            {
                return Err(rl_error(
                    "MCL_RL_EXPORT_MANIFEST_INVALID",
                    "RL export source bindings are duplicated or unsorted",
                ));
            }
            previous_source = Some(source.release_id.as_str());
        }
        let mut previous_member = None;
        let mut paths = BTreeSet::new();
        let mut total = 0_u64;
        let mut observed_tasks = 0_u64;
        for member in &self.members {
            member.validate()?;
            if previous_member.is_some_and(|path: &str| path >= member.path.as_str()) {
                return Err(rl_error(
                    "MCL_RL_EXPORT_MANIFEST_INVALID",
                    "RL export members must be strictly path-sorted",
                ));
            }
            previous_member = Some(member.path.as_str());
            paths.insert(member.path.as_str());
            total = total.checked_add(member.byte_size).ok_or_else(|| {
                rl_error(
                    "MCL_RL_EXPORT_MANIFEST_INVALID",
                    "RL export member sizes overflow their closed bound",
                )
            })?;
            observed_tasks += u64::from(member.kind == RlExportMemberKind::Task);
        }
        if total > MAX_RL_TOTAL_BYTES
            || observed_tasks != self.task_count
            || !paths.contains("plan/plan.json")
            || !paths.contains("leakage/report.json")
            || !paths.contains("schemas/rl-export-plan-1.schema.json")
            || !paths.contains("schemas/rl-task-1.schema.json")
            || !paths.contains("schemas/rl-leakage-report-1.schema.json")
            || !paths.contains("schemas/rl-export-manifest-1.schema.json")
        {
            return Err(rl_error(
                "MCL_RL_EXPORT_MANIFEST_INVALID",
                "RL export inventory, task count, or total bytes are invalid",
            ));
        }
        for source in &self.source_releases {
            let path = format!("source-releases/{}/manifest.json", source.release_id);
            if !paths.contains(path.as_str()) {
                return Err(rl_error(
                    "MCL_RL_EXPORT_MANIFEST_INVALID",
                    "RL export omits a bound source release manifest",
                ));
            }
        }
        Ok(())
    }
}

impl RlExportSourceBinding {
    fn validate(&self) -> Result<(), AppError> {
        if !valid_release_id(&self.release_id)
            || !is_hash(&self.release_manifest_hash)
            || !valid_date(&self.published_on)
            || !is_prefixed_hash(&self.leakage_component_id, "rl_component_")
            || (self.release_profile == ReleaseProfile::Private
                && self.split != RlSplit::HeldOutEvaluation)
        {
            return Err(rl_error(
                "MCL_RL_EXPORT_SOURCE_INVALID",
                "RL source binding violates identity or private split policy",
            ));
        }
        Ok(())
    }
}

impl RlExportMember {
    fn validate(&self) -> Result<(), AppError> {
        if !is_safe_relative_path(&self.path)
            || !self
                .path
                .starts_with(&format!("{}/", self.kind.directory()))
            || !is_hash(&self.content_hash)
            || self.byte_size > MAX_RL_MEMBER_BYTES
            || self
                .license_expression
                .as_deref()
                .is_some_and(|license| !valid_label(license))
            || (self.kind == RlExportMemberKind::Artifact
                && self.path != format!("artifacts/{}", self.content_hash))
        {
            return Err(rl_error(
                "MCL_RL_EXPORT_MEMBER_INVALID",
                "RL export member path, hash, size, license, or artifact identity is invalid",
            ));
        }
        Ok(())
    }
}

fn valid_sorted_uuids(values: &[String]) -> bool {
    !values.is_empty()
        && values.len() <= 256
        && values
            .iter()
            .all(|value| uuid::Uuid::parse_str(value).is_ok())
        && values.windows(2).all(|pair| pair[0] < pair[1])
}

fn contains_private_reasoning(value: &Value) -> bool {
    match value {
        Value::Array(values) => values.iter().any(contains_private_reasoning),
        Value::Object(values) => values.iter().any(|(key, value)| {
            matches!(
                key.as_str(),
                "chain_of_thought" | "private_chain_of_thought" | "reasoning_trace"
            ) || contains_private_reasoning(value)
        }),
        _ => false,
    }
}

fn valid_release_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' | b'0'..=b'9' => true,
            b'-' | b'_' => index > 0,
            _ => false,
        })
}

fn valid_label(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_LABEL_BYTES
        && !value.as_bytes().contains(&0)
        && !value.contains(['\r', '\n'])
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let Ok(year) = value[0..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u32>() else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year > 0 && (1..=days).contains(&day)
}

fn is_safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.contains('\\')
        && value
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn is_prefixed_hash(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(is_hash)
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn serialization_error(error: serde_json::Error) -> AppError {
    AppError::new(
        "MCL_RL_SERIALIZATION_FAILED",
        error.to_string(),
        false,
        "Report this deterministic RL export serialization defect.",
    )
}

fn rl_error(code: &'static str, message: impl Into<String>) -> AppError {
    AppError::new(
        code,
        message,
        false,
        "Restore the exact frozen releases and use the committed leakage-aware RL export contract.",
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn labels() -> RlLeakageLabels {
        RlLeakageLabels {
            theorem_dependency_components: vec!["prime-parity".to_owned()],
            equivalent_formalizations: vec!["prime-parity".to_owned()],
            shared_sources: vec!["pilot-a-source".to_owned()],
            certificate_families: vec!["pilot-a-refutation".to_owned()],
            proof_variants: vec!["pilot-a-repair".to_owned()],
        }
    }

    #[test]
    fn plan_requires_component_metadata_and_temporal_isolation() {
        let mut plan = RlExportPlan {
            schema_version: RL_EXPORT_PLAN_SCHEMA_VERSION.to_owned(),
            publication_cutoff: "2026-07-20".to_owned(),
            releases: vec![RlPlanRelease {
                release_id: "pilot-a".to_owned(),
                expected_manifest_hash: "a".repeat(64),
                split: RlSplit::HeldOutEvaluation,
                published_on: "2026-07-21".to_owned(),
                benchmark_identity: "pilot-a".to_owned(),
                leakage_labels: labels(),
            }],
        };
        plan.validate().expect("valid held-out plan");
        assert_eq!(plan.plan_hash().expect("plan hash").len(), 64);

        plan.releases[0].leakage_labels.proof_variants.clear();
        assert_eq!(
            plan.validate().expect_err("missing labels").code,
            "MCL_RL_LEAKAGE_LABELS_INVALID"
        );
        plan.releases[0].leakage_labels = labels();
        plan.releases[0].split = RlSplit::Train;
        assert_eq!(
            plan.validate().expect_err("future training data").code,
            "MCL_RL_TEMPORAL_LEAKAGE"
        );
    }

    #[test]
    fn task_hash_covers_payload_and_rejects_private_reasoning() {
        let mut task = RlTask {
            schema_version: RL_TASK_SCHEMA_VERSION.to_owned(),
            task_id: format!("rl_task_{}", "0".repeat(64)),
            family: RlTaskFamily::Formalization,
            split: RlSplit::HeldOutEvaluation,
            leakage_component_id: format!("rl_component_{}", "b".repeat(64)),
            input: json!({"claim": "Every prime is odd."}),
            target: json!({"formal_statement": "forall n, Prime n -> Odd n"}),
            evidence: vec![RlTaskEvidenceReference {
                path: "objects/claim/example.json".to_owned(),
                content_hash: "c".repeat(64),
            }],
            trust: RlTaskTrust {
                source_release_id: "pilot-a".to_owned(),
                source_release_manifest_hash: "d".repeat(64),
                publication_receipt_hash: "e".repeat(64),
                authority_evidence_ids: vec![uuid::Uuid::from_u128(1).to_string()],
                fidelity_evidence_ids: vec![uuid::Uuid::from_u128(2).to_string()],
                kernel_verified: true,
            },
            policy: RlTaskPolicy {
                restriction: ArtifactRestriction::Private,
                license_expressions: Vec::new(),
                license_complete: false,
            },
        };
        task.task_id = task.expected_task_id().expect("task ID");
        task.validate().expect("valid task");
        task.target["chain_of_thought"] = json!("hidden");
        assert_eq!(
            task.validate().expect_err("private reasoning").code,
            "MCL_RL_TASK_INVALID"
        );
    }

    #[test]
    fn private_split_and_public_license_policy_fail_closed() {
        let source = RlExportSourceBinding {
            release_id: "private-source".to_owned(),
            release_manifest_hash: "a".repeat(64),
            release_profile: ReleaseProfile::Private,
            split: RlSplit::Train,
            published_on: "2026-07-20".to_owned(),
            leakage_component_id: format!("rl_component_{}", "b".repeat(64)),
        };
        assert_eq!(
            source
                .validate()
                .expect_err("private training blocked")
                .code,
            "MCL_RL_EXPORT_SOURCE_INVALID"
        );

        let policy = RlTaskPolicy {
            restriction: ArtifactRestriction::Public,
            license_expressions: Vec::new(),
            license_complete: false,
        };
        assert_eq!(
            policy.validate().expect_err("unlicensed public task").code,
            "MCL_RL_TASK_POLICY_INVALID"
        );
    }
}
