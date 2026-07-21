use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::str::FromStr;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::canonical::{canonical_json, record_version_hash, value_hash};
use crate::domain::evidence::{AUTHORITATIVE_EVIDENCE_SCHEMA_VERSION, PublicationAuthorityBinding};
use crate::domain::schemas::{
    ClaimPayload, ConceptPayload, ExactVersionReference, FormalizationClaimPolarity,
    FormalizationPayload, LearningTargetKind, LearningUnitPayload, SourcePayload,
    validate_record_payload,
};
use crate::domain::{
    ArtifactCreationSource, ArtifactMediaType, ArtifactMetadata, ArtifactRestriction,
    ArtifactSnapshot, CLAIM_REPAIR_EDGE_SCHEMA_VERSION, COUNTEREXAMPLE_PACKAGE_SCHEMA_VERSION,
    ClaimRepairEdgePayload, CounterexampleRepairSnapshot, EdgeDraft, EdgeKind, EdgeSnapshot,
    EnvironmentManifest, EnvironmentSnapshot, EvidenceAuthorityClass, EvidenceKind,
    EvidencePayload, EvidenceResult, EvidenceSnapshot, LeanAuditJobSnapshot, LeanAuditRequest,
    PublicationAttestationVerification, PublicationIngestionReceiptSnapshot, PublicationOutcome,
    PublicationRequest, PublicationRetainedArtifactRole, PublicationStage,
    PublicationStageSnapshot, RecordDraft, RecordKind, RecordSnapshot, VerifierJobRequest,
    VerifierJobSnapshot, VerifierJobState,
};
use crate::error::AppError;

mod graph;
mod runs;

const MIGRATION_0001: &str = include_str!("../../migrations/0001_initial.sql");
const MIGRATION_0002: &str = include_str!("../../migrations/0002_idempotency.sql");
const MIGRATION_0003: &str = include_str!("../../migrations/0003_record_invariants.sql");
const MIGRATION_0004: &str = include_str!("../../migrations/0004_edge_invariants.sql");
const MIGRATION_0005: &str = include_str!("../../migrations/0005_run_event_invariants.sql");
const MIGRATION_0006: &str = include_str!("../../migrations/0006_environment_invariants.sql");
const MIGRATION_0007: &str = include_str!("../../migrations/0007_artifact_invariants.sql");
const MIGRATION_0008: &str = include_str!("../../migrations/0008_verifier_jobs.sql");
const MIGRATION_0009: &str = include_str!("../../migrations/0009_evidence_invariants.sql");
const MIGRATION_0010: &str = include_str!("../../migrations/0010_publication_ingestion.sql");
const MIGRATION_0011: &str = include_str!("../../migrations/0011_publication_authority.sql");
const MAX_CLAIM_STATUS_FORMALIZATIONS: usize = 256;
const MAX_CLAIM_STATUS_EVIDENCE_PER_FORMALIZATION: usize = 256;
const MAX_PUBLICATION_INPUT_BYTES: u64 = 16 * 1_048_576;
const MAX_REGISTERED_CAS_HASHES: usize = 100_000;
const PUBLICATION_INGESTION_OPERATION: &str = "publication.ingestion_receipt.register";
const PUBLICATION_AUTHORITY_OPERATION: &str = "evidence.create_publication_authority";
const COUNTEREXAMPLE_REPAIR_OPERATION: &str = "counterexample.repair";
const REQUIRED_TABLES: &[&str] = &[
    "artifacts",
    "edges",
    "environments",
    "evidence",
    "jobs",
    "idempotency_results",
    "record_versions",
    "records",
    "releases",
    "publication_ingestion_receipts",
    "publication_stages",
    "run_events",
    "runs",
    "schema_migrations",
];
type RawRecordRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    i64,
    String,
);
type RawEdgeRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ClaimStatusEvidenceReadBasis {
    pub evidence_id: String,
    pub evidence_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ClaimStatusFormalizationReadBasis {
    pub formalization: ExactVersionReference,
    pub fidelity_evidence: Vec<ClaimStatusEvidenceReadBasis>,
    pub authoritative_evidence: Vec<ClaimStatusEvidenceReadBasis>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ClaimStatusReadBasis {
    pub claim: ExactVersionReference,
    pub source: ExactVersionReference,
    pub current_claim_head_version_hash: Option<String>,
    pub current_source_head_version_hash: Option<String>,
    pub formalizations: Vec<ClaimStatusFormalizationReadBasis>,
}

#[derive(Clone, Debug)]
pub(crate) struct CounterexampleRepairCommit {
    pub package_artifact_hash: String,
    pub package_byte_size: u64,
    pub package_metadata: ArtifactMetadata,
    pub repaired_claim: RecordDraft,
    pub repaired_claim_version_hash: String,
    pub original_claim: ExactVersionReference,
    pub repair_edge_payload: ClaimRepairEdgePayload,
    pub claim_status_basis: ClaimStatusReadBasis,
    pub counterexample_search_run_id: String,
    pub counterexample_search_run_head_hash: String,
}

#[derive(Clone, Copy)]
enum ClaimStatusEvidenceSelection {
    Fidelity,
    Authoritative,
}

#[derive(Clone, Debug)]
pub(crate) struct PublicationAuthorityCommit {
    pub subject: ExactVersionReference,
    pub outcome: PublicationOutcome,
    pub environment_hash: String,
    pub binding: PublicationAuthorityBinding,
    pub artifact_hashes: Vec<String>,
}

impl PublicationAuthorityCommit {
    pub(crate) fn evidence_payload(&self) -> Result<EvidencePayload, AppError> {
        let evidence_kind = match self.outcome {
            PublicationOutcome::Proof => EvidenceKind::LeanKernelProof,
            PublicationOutcome::Refutation => EvidenceKind::LeanKernelRefutation,
        };
        let payload = EvidencePayload {
            schema_version: AUTHORITATIVE_EVIDENCE_SCHEMA_VERSION.to_owned(),
            subject: self.subject.clone(),
            evidence_kind,
            result: EvidenceResult::Accepted,
            authority_class: EvidenceAuthorityClass::Authoritative,
            producing_run_id: None,
            producing_job_id: None,
            artifact_hashes: self.artifact_hashes.clone(),
            verifier_or_reviewer_identity: format!(
                "publication-policy:{}",
                self.binding.publication_policy_hash
            ),
            environment_hash: Some(self.environment_hash.clone()),
            supersedes_evidence_id: None,
            stale: false,
            stale_reason: None,
            publication_authority: Some(self.binding.clone()),
        };
        payload.validate()?;
        Ok(payload)
    }
}

pub struct Store {
    connection: Connection,
    #[cfg(test)]
    counterexample_repair_fail_after_artifact: bool,
    #[cfg(test)]
    counterexample_repair_advance_run_before_commit: bool,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| AppError::io("create database directory", error))?;
        }
        let connection =
            Connection::open(path).map_err(|error| AppError::database("open database", error))?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| AppError::database("enable WAL mode", error))?;
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(|error| AppError::database("enable foreign keys", error))?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|error| AppError::database("configure busy timeout", error))?;
        Ok(Self {
            connection,
            #[cfg(test)]
            counterexample_repair_fail_after_artifact: false,
            #[cfg(test)]
            counterexample_repair_advance_run_before_commit: false,
        })
    }

    #[cfg(test)]
    pub(crate) fn inject_counterexample_repair_failure_after_artifact(&mut self) {
        self.counterexample_repair_fail_after_artifact = true;
    }

    #[cfg(test)]
    pub(crate) fn inject_counterexample_repair_run_head_change_before_commit(&mut self) {
        self.counterexample_repair_advance_run_before_commit = true;
    }

    #[cfg(test)]
    pub(crate) fn apply_counterexample_repair_run_head_change(
        &mut self,
        run_id: &str,
        expected_head_hash: &str,
    ) -> Result<(), AppError> {
        if !std::mem::take(&mut self.counterexample_repair_advance_run_before_commit) {
            return Ok(());
        }
        self.append_run_event(
            run_id,
            expected_head_hash,
            &crate::domain::RunEventDraft {
                kind: crate::domain::RunEventKind::Diagnostic,
                payload: json!({"test_fault": "concurrent_run_head_change"}),
            },
            "counterexample-concurrency-test",
            &format!("counterexample-concurrent-head:{expected_head_hash}"),
        )?;
        Ok(())
    }

    pub fn migrate(&mut self) -> Result<(), AppError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start migration transaction", error))?;
        let applied: Option<String> = transaction
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| AppError::database("inspect migration table", error))?;
        if applied.is_none() {
            transaction
                .execute_batch(MIGRATION_0001)
                .map_err(|error| AppError::database("apply migration 0001", error))?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, unixepoch())",
                    params![1_i64, "initial"],
                )
                .map_err(|error| AppError::database("record migration 0001", error))?;
        }
        let migration_0002_applied: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = 2)",
                [],
                |row| row.get(0),
            )
            .map_err(|error| AppError::database("inspect migration 0002", error))?;
        if !migration_0002_applied {
            transaction
                .execute_batch(MIGRATION_0002)
                .map_err(|error| AppError::database("apply migration 0002", error))?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, unixepoch())",
                    params![2_i64, "idempotency results"],
                )
                .map_err(|error| AppError::database("record migration 0002", error))?;
        }
        let migration_0003_applied: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = 3)",
                [],
                |row| row.get(0),
            )
            .map_err(|error| AppError::database("inspect migration 0003", error))?;
        if !migration_0003_applied {
            transaction
                .execute_batch(MIGRATION_0003)
                .map_err(|error| AppError::database("apply migration 0003", error))?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, unixepoch())",
                    params![3_i64, "record invariants"],
                )
                .map_err(|error| AppError::database("record migration 0003", error))?;
        }
        let migration_0004_applied: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = 4)",
                [],
                |row| row.get(0),
            )
            .map_err(|error| AppError::database("inspect migration 0004", error))?;
        if !migration_0004_applied {
            transaction
                .execute_batch(MIGRATION_0004)
                .map_err(|error| AppError::database("apply migration 0004", error))?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, unixepoch())",
                    params![4_i64, "edge invariants"],
                )
                .map_err(|error| AppError::database("record migration 0004", error))?;
        }
        let migration_0005_applied: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = 5)",
                [],
                |row| row.get(0),
            )
            .map_err(|error| AppError::database("inspect migration 0005", error))?;
        if !migration_0005_applied {
            transaction
                .execute_batch(MIGRATION_0005)
                .map_err(|error| AppError::database("apply migration 0005", error))?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, unixepoch())",
                    params![5_i64, "run event invariants"],
                )
                .map_err(|error| AppError::database("record migration 0005", error))?;
        }
        let migration_0006_applied: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = 6)",
                [],
                |row| row.get(0),
            )
            .map_err(|error| AppError::database("inspect migration 0006", error))?;
        if !migration_0006_applied {
            transaction
                .execute_batch(MIGRATION_0006)
                .map_err(|error| AppError::database("apply migration 0006", error))?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, unixepoch())",
                    params![6_i64, "environment invariants"],
                )
                .map_err(|error| AppError::database("record migration 0006", error))?;
        }
        let migration_0007_applied: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = 7)",
                [],
                |row| row.get(0),
            )
            .map_err(|error| AppError::database("inspect migration 0007", error))?;
        if !migration_0007_applied {
            transaction
                .execute_batch(MIGRATION_0007)
                .map_err(|error| AppError::database("apply migration 0007", error))?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, unixepoch())",
                    params![7_i64, "artifact invariants"],
                )
                .map_err(|error| AppError::database("record migration 0007", error))?;
        }
        let migration_0008_applied: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = 8)",
                [],
                |row| row.get(0),
            )
            .map_err(|error| AppError::database("inspect migration 0008", error))?;
        if !migration_0008_applied {
            transaction
                .execute_batch(MIGRATION_0008)
                .map_err(|error| AppError::database("apply migration 0008", error))?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, unixepoch())",
                    params![8_i64, "durable verifier jobs"],
                )
                .map_err(|error| AppError::database("record migration 0008", error))?;
        }
        let migration_0009_applied: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = 9)",
                [],
                |row| row.get(0),
            )
            .map_err(|error| AppError::database("inspect migration 0009", error))?;
        if !migration_0009_applied {
            transaction
                .execute_batch(MIGRATION_0009)
                .map_err(|error| AppError::database("apply migration 0009", error))?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, unixepoch())",
                    params![9_i64, "evidence invariants"],
                )
                .map_err(|error| AppError::database("record migration 0009", error))?;
        }
        let migration_0010_applied: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = 10)",
                [],
                |row| row.get(0),
            )
            .map_err(|error| AppError::database("inspect migration 0010", error))?;
        if !migration_0010_applied {
            transaction
                .execute_batch(MIGRATION_0010)
                .map_err(|error| AppError::database("apply migration 0010", error))?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, unixepoch())",
                    params![10_i64, "publication ingestion"],
                )
                .map_err(|error| AppError::database("record migration 0010", error))?;
        }
        let migration_0011_applied: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = 11)",
                [],
                |row| row.get(0),
            )
            .map_err(|error| AppError::database("inspect migration 0011", error))?;
        if !migration_0011_applied {
            transaction
                .execute_batch(MIGRATION_0011)
                .map_err(|error| AppError::database("apply migration 0011", error))?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, unixepoch())",
                    params![11_i64, "publication authority"],
                )
                .map_err(|error| AppError::database("record migration 0011", error))?;
        }
        transaction
            .commit()
            .map_err(|error| AppError::database("commit migrations", error))
    }

    pub fn integrity_check(&self) -> Result<String, AppError> {
        self.connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(|error| AppError::database("run database integrity check", error))
    }

    pub fn migration_version(&self) -> Result<i64, AppError> {
        self.connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(|error| AppError::database("read migration version", error))
    }

    pub fn journal_mode(&self) -> Result<String, AppError> {
        self.connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(|error| AppError::database("read database journal mode", error))
    }

    pub fn fts5_check(&self) -> Result<(), AppError> {
        self.connection
            .query_row(
                "SELECT COUNT(*) FROM record_search WHERE record_search MATCH 'mcldoctornevermatches'",
                [],
                |_row| Ok(()),
            )
            .map_err(|error| AppError::database("query FTS5 index", error))
    }

    pub fn schema_check(&self) -> Result<(), AppError> {
        for table in REQUIRED_TABLES {
            let exists: i64 = self
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                    [table],
                    |row| row.get(0),
                )
                .map_err(|error| AppError::database("inspect required schema", error))?;
            if exists != 1 {
                return Err(AppError::new(
                    "MCL_SCHEMA_INCOMPLETE",
                    format!("required table `{table}` is missing"),
                    false,
                    "Restore a verified backup or run the documented forward migration.",
                ));
            }
        }
        Ok(())
    }

    pub fn stale_lease_count(&self) -> Result<i64, AppError> {
        self.connection
            .query_row(
                "SELECT COUNT(*) FROM jobs WHERE state IN ('leased', 'running') AND lease_expires_at < unixepoch()",
                [],
                |row| row.get(0),
            )
            .map_err(|error| AppError::database("count stale leases", error))
    }

    pub fn register_environment(
        &mut self,
        manifest: &EnvironmentManifest,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<EnvironmentSnapshot, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        let manifest_value = manifest.canonical_value()?;
        let environment_hash = value_hash(&manifest_value)?;
        let input_hash = value_hash(&json!({
            "operation": "environment.register",
            "manifest": manifest_value,
            "actor": actor,
        }))?;
        let manifest_json =
            String::from_utf8(canonical_json(&manifest_value)?).map_err(|error| {
                AppError::new(
                    "MCL_CANONICAL_JSON_INVALID",
                    error.to_string(),
                    false,
                    "Report this canonical JSON encoding defect.",
                )
            })?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start environment registration", error))?;
        if let Some(existing) = read_idempotent_result(
            &transaction,
            "environment.register",
            idempotency_key,
            &input_hash,
        )? {
            return Ok(existing);
        }
        if transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM environments WHERE environment_hash = ?1)",
                [&environment_hash],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| AppError::database("search registered environment", error))?
        {
            return Err(AppError::new(
                "MCL_ENVIRONMENT_EXISTS",
                format!("environment {environment_hash} is already registered"),
                false,
                "Retrieve the existing environment or retry with the original idempotency key.",
            ));
        }
        transaction
            .execute(
                "INSERT INTO environments(environment_hash, manifest_json, trust_profile, created_at, created_by) VALUES (?1, ?2, ?3, unixepoch(), ?4)",
                params![environment_hash, manifest_json, manifest.trust_profile.as_str(), actor],
            )
            .map_err(|error| AppError::database("insert environment", error))?;
        let snapshot = read_environment(&transaction, &environment_hash)?;
        write_idempotent_result(
            &transaction,
            "environment.register",
            idempotency_key,
            &input_hash,
            &snapshot,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit environment registration", error))?;
        Ok(snapshot)
    }

    pub fn validate_environment_registration(
        &self,
        manifest: &EnvironmentManifest,
    ) -> Result<String, AppError> {
        let environment_hash = manifest.environment_hash()?;
        if self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM environments WHERE environment_hash = ?1)",
                [&environment_hash],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| AppError::database("preview environment registration", error))?
        {
            return Err(AppError::new(
                "MCL_ENVIRONMENT_EXISTS",
                format!("environment {environment_hash} is already registered"),
                false,
                "Retrieve the existing environment instead of registering it again.",
            ));
        }
        Ok(environment_hash)
    }

    pub fn get_environment(&self, environment_hash: &str) -> Result<EnvironmentSnapshot, AppError> {
        validate_hash(environment_hash, "environment")?;
        read_environment(&self.connection, environment_hash)
    }

    pub fn list_environments(&self, limit: usize) -> Result<Vec<EnvironmentSnapshot>, AppError> {
        if !(1..=100).contains(&limit) {
            return Err(AppError::new(
                "MCL_ENVIRONMENT_LIMIT_INVALID",
                "environment list limit must be between 1 and 100",
                false,
                "Use a bounded environment list limit.",
            ));
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT environment_hash FROM environments ORDER BY created_at, environment_hash LIMIT ?1",
            )
            .map_err(|error| AppError::database("prepare environment list", error))?;
        let hashes = statement
            .query_map([limit as i64], |row| row.get::<_, String>(0))
            .map_err(|error| AppError::database("list environments", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::database("read environment list", error))?;
        hashes
            .iter()
            .map(|hash| read_environment(&self.connection, hash))
            .collect()
    }

    pub fn environment_count(&self) -> Result<i64, AppError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM environments", [], |row| row.get(0))
            .map_err(|error| AppError::database("count environments", error))
    }

    pub fn register_artifact(
        &mut self,
        artifact_hash: &str,
        byte_size: u64,
        metadata: &ArtifactMetadata,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<ArtifactSnapshot, AppError> {
        self.register_artifact_with_policy(
            artifact_hash,
            byte_size,
            metadata,
            actor,
            idempotency_key,
            "artifact.register",
            false,
            None,
        )
    }

    pub fn register_publication_request_artifact(
        &mut self,
        artifact_hash: &str,
        byte_size: u64,
        metadata: &ArtifactMetadata,
        request: &PublicationRequest,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<ArtifactSnapshot, AppError> {
        request.validate()?;
        let request_hash = request.request_hash()?;
        if artifact_hash != request_hash
            || metadata.media_type != ArtifactMediaType::Json
            || metadata.creation_source != ArtifactCreationSource::Generated
            || metadata.restriction != ArtifactRestriction::Private
            || metadata
                .semantic_metadata
                .get("artifact_role")
                .is_none_or(|value| value != "publication_request")
            || metadata
                .semantic_metadata
                .get("request_hash")
                .is_none_or(|value| value != &request_hash)
            || metadata
                .semantic_metadata
                .get("formalization_object_id")
                .is_none_or(|value| value != &request.subject.object_id)
            || metadata
                .semantic_metadata
                .get("formalization_version_hash")
                .is_none_or(|value| value != &request.subject.version_hash)
            || metadata
                .semantic_metadata
                .get("source_commit_sha")
                .is_none_or(|value| value != &request.source_commit_sha)
            || metadata
                .semantic_metadata
                .get("source_tree_sha")
                .is_none_or(|value| value != &request.source_tree_sha)
        {
            return Err(AppError::new(
                "MCL_PUBLICATION_REQUEST_ARTIFACT_INVALID",
                "publication request artifact metadata does not bind the exact canonical request",
                false,
                "Create publication request artifacts only through the controlled application path.",
            ));
        }
        self.register_artifact_with_policy(
            artifact_hash,
            byte_size,
            metadata,
            actor,
            idempotency_key,
            "artifact.register_publication_request",
            true,
            Some(&request.subject),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn register_artifact_with_policy(
        &mut self,
        artifact_hash: &str,
        byte_size: u64,
        metadata: &ArtifactMetadata,
        actor: &str,
        idempotency_key: &str,
        operation: &'static str,
        accept_matching_existing: bool,
        required_current_subject: Option<&ExactVersionReference>,
    ) -> Result<ArtifactSnapshot, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        self.validate_artifact_registration(artifact_hash, byte_size, metadata)?;
        let input_hash = value_hash(&json!({
            "operation": operation,
            "artifact_hash": artifact_hash,
            "byte_size": byte_size,
            "metadata": metadata,
            "actor": actor,
        }))?;
        let metadata_json = String::from_utf8(canonical_json(
            &serde_json::to_value(metadata).map_err(|error| {
                AppError::new(
                    "MCL_ARTIFACT_METADATA_INVALID",
                    error.to_string(),
                    false,
                    "Report this deterministic artifact serialization defect.",
                )
            })?,
        )?)
        .map_err(|error| {
            AppError::new(
                "MCL_CANONICAL_JSON_INVALID",
                error.to_string(),
                false,
                "Report this canonical JSON encoding defect.",
            )
        })?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start artifact registration", error))?;
        if let Some(subject) = required_current_subject {
            validate_current_publication_subject(&transaction, subject)?;
        }
        if let Some(existing) =
            read_idempotent_result(&transaction, operation, idempotency_key, &input_hash)?
        {
            return Ok(existing);
        }
        let artifact_exists = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM artifacts WHERE artifact_hash = ?1)",
                [artifact_hash],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| AppError::database("search registered artifact", error))?;
        if artifact_exists && accept_matching_existing {
            let existing = read_artifact(&transaction, artifact_hash)?;
            if !artifact_snapshot_matches_metadata(&existing, metadata, byte_size) {
                return Err(AppError::new(
                    "MCL_ARTIFACT_METADATA_CONFLICT",
                    format!("existing artifact {artifact_hash} has incompatible metadata"),
                    false,
                    "Quarantine the conflicting artifact and inspect its provenance.",
                ));
            }
            write_idempotent_result(
                &transaction,
                operation,
                idempotency_key,
                &input_hash,
                &existing,
            )?;
            transaction
                .commit()
                .map_err(|error| AppError::database("commit artifact registration", error))?;
            return Ok(existing);
        }
        if artifact_exists {
            return Err(AppError::new(
                "MCL_ARTIFACT_EXISTS",
                format!("artifact {artifact_hash} is already registered"),
                false,
                "Retrieve the existing artifact or retry with the original idempotency key.",
            ));
        }
        transaction
            .execute(
                "INSERT INTO artifacts(artifact_hash, media_type, byte_size, creation_source, license_expression, restriction, metadata_json, created_at, created_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, unixepoch(), ?8)",
                params![
                    artifact_hash,
                    metadata.media_type.as_str(),
                    byte_size as i64,
                    metadata.creation_source.as_str(),
                    metadata.license_expression,
                    metadata.restriction.as_str(),
                    metadata_json,
                    actor,
                ],
            )
            .map_err(|error| AppError::database("insert artifact metadata", error))?;
        let snapshot = read_artifact(&transaction, artifact_hash)?;
        write_idempotent_result(
            &transaction,
            operation,
            idempotency_key,
            &input_hash,
            &snapshot,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit artifact registration", error))?;
        Ok(snapshot)
    }

    pub fn validate_artifact_registration(
        &self,
        artifact_hash: &str,
        byte_size: u64,
        metadata: &ArtifactMetadata,
    ) -> Result<(), AppError> {
        validate_hash(artifact_hash, "artifact")?;
        metadata.validate(byte_size)?;
        Ok(())
    }

    pub fn get_artifact(&self, artifact_hash: &str) -> Result<ArtifactSnapshot, AppError> {
        validate_hash(artifact_hash, "artifact")?;
        read_artifact(&self.connection, artifact_hash)
    }

    pub fn list_artifacts(&self, limit: usize) -> Result<Vec<ArtifactSnapshot>, AppError> {
        if !(1..=100).contains(&limit) {
            return Err(AppError::new(
                "MCL_ARTIFACT_LIMIT_INVALID",
                "artifact list limit must be between 1 and 100",
                false,
                "Use a bounded artifact list limit.",
            ));
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT artifact_hash FROM artifacts ORDER BY created_at, artifact_hash LIMIT ?1",
            )
            .map_err(|error| AppError::database("prepare artifact list", error))?;
        let hashes = statement
            .query_map([limit as i64], |row| row.get::<_, String>(0))
            .map_err(|error| AppError::database("list artifacts", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::database("read artifact list", error))?;
        hashes
            .iter()
            .map(|hash| read_artifact(&self.connection, hash))
            .collect()
    }

    pub fn artifact_count(&self) -> Result<i64, AppError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
            .map_err(|error| AppError::database("count artifacts", error))
    }

    pub fn all_artifact_hashes(&self) -> Result<Vec<String>, AppError> {
        let mut statement = self
            .connection
            .prepare("SELECT artifact_hash FROM artifacts ORDER BY artifact_hash")
            .map_err(|error| AppError::database("prepare artifact inventory", error))?;
        let hashes = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| AppError::database("inventory artifacts", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::database("read artifact inventory", error))?;
        if hashes.len() > 100_000 {
            return Err(AppError::new(
                "MCL_ARTIFACT_SCAN_LIMIT",
                "canonical artifact inventory exceeded its reviewed bound",
                false,
                "Inspect storage growth before increasing the inventory policy.",
            ));
        }
        Ok(hashes)
    }

    pub fn register_publication_stage(
        &mut self,
        stage: &PublicationStage,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<PublicationStageSnapshot, AppError> {
        const OPERATION: &str = "publication.stage.register";

        validate_mutation_inputs(actor, idempotency_key)?;
        validate_publication_actor(actor)?;
        stage.validate()?;
        let stage_hash = stage.stage_hash()?;
        let stage_value = serde_json::to_value(stage).map_err(|error| {
            AppError::new(
                "MCL_PUBLICATION_STAGE_INVALID",
                error.to_string(),
                false,
                "Report this deterministic publication-stage serialization defect.",
            )
        })?;
        let stage_json = canonical_string(&stage_value)?;
        let input_hash = value_hash(&json!({
            "operation": OPERATION,
            "stage": stage_value,
            "actor": actor,
        }))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start publication stage registration", error))?;

        if let Some(existing) = read_idempotent_result::<PublicationStageSnapshot>(
            &transaction,
            OPERATION,
            idempotency_key,
            &input_hash,
        )? {
            let stored = read_publication_stage(&transaction, &existing.stage_hash)?;
            if existing != stored
                || stored.stage != stage.clone()
                || stored.stage_hash != stage_hash
            {
                return Err(publication_stage_integrity_error(
                    "stored idempotency result disagrees with the immutable publication stage",
                ));
            }
            return Ok(stored);
        }

        let existing_stage_hash = transaction
            .query_row(
                "SELECT stage_hash FROM publication_stages WHERE (report_artifact_hash = ?1 AND attestation_bundle_artifact_hash = ?2) OR stage_hash = ?3 ORDER BY stage_hash LIMIT 1",
                params![
                    stage.report_artifact_hash,
                    stage.attestation_bundle_artifact_hash,
                    stage_hash,
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| AppError::database("find publication stage", error))?;
        if let Some(existing_stage_hash) = existing_stage_hash {
            let existing = read_publication_stage(&transaction, &existing_stage_hash)?;
            if existing.stage_hash != stage_hash || existing.stage != stage.clone() {
                return Err(AppError::new(
                    "MCL_PUBLICATION_STAGE_CONFLICT",
                    "the report and attestation bundle are already bound to a different publication stage",
                    false,
                    "Use the exact originally staged bytes or investigate the conflicting publication input.",
                ));
            }
            write_idempotent_result(
                &transaction,
                OPERATION,
                idempotency_key,
                &input_hash,
                &existing,
            )?;
            transaction.commit().map_err(|error| {
                AppError::database("commit matching publication stage registration", error)
            })?;
            return Ok(existing);
        }

        transaction
            .execute(
                "INSERT INTO publication_stages(stage_hash, schema_version, report_artifact_hash, report_byte_size, retained_closure_artifact_hash, retained_closure_byte_size, attestation_bundle_artifact_hash, attestation_bundle_byte_size, retained_artifact_count, stage_json, authoritative, created_at, created_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, unixepoch(), ?11)",
                params![
                    stage_hash,
                    stage.schema_version,
                    stage.report_artifact_hash,
                    stage.report_byte_size as i64,
                    stage.retained_closure_artifact_hash,
                    stage.retained_closure_byte_size as i64,
                    stage.attestation_bundle_artifact_hash,
                    stage.attestation_bundle_byte_size as i64,
                    stage.retained_artifacts.len() as i64,
                    stage_json,
                    actor,
                ],
            )
            .map_err(|error| AppError::database("insert publication stage", error))?;
        let snapshot = read_publication_stage(&transaction, &stage_hash)?;
        write_idempotent_result(
            &transaction,
            OPERATION,
            idempotency_key,
            &input_hash,
            &snapshot,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit publication stage registration", error))?;
        Ok(snapshot)
    }

    pub fn get_publication_stage(
        &self,
        report_artifact_hash: &str,
        attestation_bundle_artifact_hash: &str,
    ) -> Result<PublicationStageSnapshot, AppError> {
        validate_hash(report_artifact_hash, "publication report artifact")?;
        validate_hash(
            attestation_bundle_artifact_hash,
            "publication attestation bundle artifact",
        )?;
        let stage_hash = self
            .connection
            .query_row(
                "SELECT stage_hash FROM publication_stages WHERE report_artifact_hash = ?1 AND attestation_bundle_artifact_hash = ?2",
                params![report_artifact_hash, attestation_bundle_artifact_hash],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| AppError::database("find publication stage", error))?
            .ok_or_else(|| publication_stage_not_found(report_artifact_hash, attestation_bundle_artifact_hash))?;
        read_publication_stage(&self.connection, &stage_hash)
    }

    pub fn get_publication_stage_by_hash(
        &self,
        stage_hash: &str,
    ) -> Result<PublicationStageSnapshot, AppError> {
        validate_hash(stage_hash, "publication stage")?;
        read_publication_stage(&self.connection, stage_hash)
    }

    pub fn publication_ingestion_idempotency_result(
        &self,
        stage_hash: &str,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<Option<PublicationIngestionReceiptSnapshot>, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        validate_publication_actor(actor)?;
        validate_hash(stage_hash, "publication stage")?;
        let input_hash = publication_ingestion_input_hash(stage_hash, actor)?;
        let stage = read_publication_stage(&self.connection, stage_hash)?;
        let Some(existing) = read_idempotent_result::<PublicationIngestionReceiptSnapshot>(
            &self.connection,
            PUBLICATION_INGESTION_OPERATION,
            idempotency_key,
            &input_hash,
        )?
        else {
            return Ok(None);
        };
        let stored = read_publication_ingestion_receipt(&self.connection, &existing.receipt_hash)?;
        validate_publication_receipt_binding(&stage, &stored.verification)?;
        if existing != stored || stored.stage_hash != stage_hash {
            return Err(publication_receipt_integrity_error(
                "stored idempotency result disagrees with the immutable ingestion receipt",
            ));
        }
        Ok(Some(stored))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register_publication_ingestion_receipt(
        &mut self,
        stage_hash: &str,
        current_subject: &ExactVersionReference,
        verification: &PublicationAttestationVerification,
        raw_verification_byte_size: u64,
        receipt_byte_size: u64,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<PublicationIngestionReceiptSnapshot, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        validate_publication_actor(actor)?;
        validate_hash(stage_hash, "publication stage")?;
        validate_hash(&current_subject.version_hash, "publication subject version")?;
        Uuid::parse_str(&current_subject.object_id).map_err(|_| {
            AppError::new(
                "MCL_PUBLICATION_SUBJECT_INVALID",
                "publication subject object identity is not a UUID",
                false,
                "Use the exact publication request subject produced by the canonical store.",
            )
        })?;
        validate_publication_attestation_shape(verification)?;
        validate_publication_input_size(
            raw_verification_byte_size,
            "raw attestation verification",
        )?;
        validate_publication_input_size(receipt_byte_size, "attestation verification receipt")?;
        let verification_value = serde_json::to_value(verification).map_err(|error| {
            AppError::new(
                "MCL_PUBLICATION_ATTESTATION_INVALID",
                error.to_string(),
                false,
                "Report this deterministic attestation-verification serialization defect.",
            )
        })?;
        let verification_json = canonical_string(&verification_value)?;
        let computed_receipt_byte_size = u64::try_from(verification_json.len()).map_err(|_| {
            AppError::new(
                "MCL_PUBLICATION_RECEIPT_INVALID",
                "canonical attestation verification size cannot be represented",
                false,
                "Report this deterministic receipt-size defect.",
            )
        })?;
        if receipt_byte_size != computed_receipt_byte_size {
            return Err(AppError::new(
                "MCL_PUBLICATION_RECEIPT_INVALID",
                format!(
                    "declared receipt size {receipt_byte_size} does not match canonical verification size {computed_receipt_byte_size}"
                ),
                false,
                "Use the exact canonical attestation verification bytes.",
            ));
        }
        let receipt_hash = value_hash(&verification_value)?;
        let input_hash = publication_ingestion_input_hash(stage_hash, actor)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start publication receipt registration", error))?;
        let stage = read_publication_stage(&transaction, stage_hash)?;
        validate_current_publication_subject(&transaction, current_subject)?;
        let retained_request_subject = publication_request_subject_binding(&transaction, &stage)?;
        if retained_request_subject != *current_subject {
            return Err(AppError::new(
                "MCL_PUBLICATION_RECEIPT_SUBJECT_MISMATCH",
                "publication receipt subject does not match the formalization retained by the staged publication request",
                false,
                "Ingest only the exact current subject embedded in the retained publication request.",
            ));
        }
        validate_publication_receipt_binding(&stage, verification)?;

        if let Some(existing) = read_idempotent_result::<PublicationIngestionReceiptSnapshot>(
            &transaction,
            PUBLICATION_INGESTION_OPERATION,
            idempotency_key,
            &input_hash,
        )? {
            let stored = read_publication_ingestion_receipt(&transaction, &existing.receipt_hash)?;
            validate_publication_receipt_retry_subject(
                &transaction,
                &stored.receipt_hash,
                current_subject,
            )?;
            if existing != stored
                || stored.receipt_hash != receipt_hash
                || stored.stage_hash != stage_hash
                || stored.verification != verification.clone()
                || stored.raw_verification_byte_size != raw_verification_byte_size
                || stored.receipt_byte_size != receipt_byte_size
            {
                return Err(publication_receipt_integrity_error(
                    "stored idempotency result disagrees with the immutable ingestion receipt",
                ));
            }
            return Ok(stored);
        }

        let existing_receipt_hash = transaction
            .query_row(
                "SELECT receipt_hash FROM publication_ingestion_receipts WHERE stage_hash = ?1 OR receipt_hash = ?2 ORDER BY receipt_hash LIMIT 1",
                params![stage_hash, receipt_hash],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| AppError::database("find publication ingestion receipt", error))?;
        if let Some(existing_receipt_hash) = existing_receipt_hash {
            let existing =
                read_publication_ingestion_receipt(&transaction, &existing_receipt_hash)?;
            validate_publication_receipt_retry_subject(
                &transaction,
                &existing.receipt_hash,
                current_subject,
            )?;
            if existing.receipt_hash != receipt_hash
                || existing.stage_hash != stage_hash
                || existing.verification != verification.clone()
                || existing.raw_verification_byte_size != raw_verification_byte_size
                || existing.receipt_byte_size != receipt_byte_size
            {
                return Err(AppError::new(
                    "MCL_PUBLICATION_RECEIPT_CONFLICT",
                    "the publication stage already has a different ingestion receipt",
                    false,
                    "Use the exact original verified inputs or investigate the conflicting ingestion attempt.",
                ));
            }
            write_idempotent_result(
                &transaction,
                PUBLICATION_INGESTION_OPERATION,
                idempotency_key,
                &input_hash,
                &existing,
            )?;
            transaction.commit().map_err(|error| {
                AppError::database("commit matching publication receipt registration", error)
            })?;
            return Ok(existing);
        }

        transaction
            .execute(
                "INSERT INTO publication_ingestion_receipts(receipt_hash, schema_version, stage_hash, report_artifact_hash, attestation_bundle_artifact_hash, raw_verification_hash, raw_verification_byte_size, receipt_byte_size, verification_json, authoritative, created_at, created_by, subject_object_id, subject_version_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, unixepoch(), ?10, ?11, ?12)",
                params![
                    receipt_hash,
                    verification.schema_version,
                    stage_hash,
                    verification.report_artifact_hash,
                    verification.attestation_bundle_hash,
                    verification.raw_verification_hash,
                    raw_verification_byte_size as i64,
                    receipt_byte_size as i64,
                    verification_json,
                    actor,
                    current_subject.object_id,
                    current_subject.version_hash,
                ],
            )
            .map_err(|error| AppError::database("insert publication ingestion receipt", error))?;
        let snapshot = read_publication_ingestion_receipt(&transaction, &receipt_hash)?;
        write_idempotent_result(
            &transaction,
            PUBLICATION_INGESTION_OPERATION,
            idempotency_key,
            &input_hash,
            &snapshot,
        )?;
        transaction.commit().map_err(|error| {
            AppError::database("commit publication receipt registration", error)
        })?;
        Ok(snapshot)
    }

    pub fn get_publication_ingestion_receipt_for_stage(
        &self,
        stage_hash: &str,
    ) -> Result<PublicationIngestionReceiptSnapshot, AppError> {
        validate_hash(stage_hash, "publication stage")?;
        let receipt_hash = self
            .connection
            .query_row(
                "SELECT receipt_hash FROM publication_ingestion_receipts WHERE stage_hash = ?1",
                [stage_hash],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| AppError::database("find publication ingestion receipt", error))?
            .ok_or_else(|| publication_receipt_not_found(stage_hash))?;
        read_publication_ingestion_receipt(&self.connection, &receipt_hash)
    }

    pub fn get_publication_ingestion_receipt(
        &self,
        receipt_hash: &str,
    ) -> Result<PublicationIngestionReceiptSnapshot, AppError> {
        validate_hash(receipt_hash, "publication ingestion receipt")?;
        read_publication_ingestion_receipt(&self.connection, receipt_hash)
    }

    pub(crate) fn create_publication_authority_evidence(
        &mut self,
        commit: &PublicationAuthorityCommit,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<EvidenceSnapshot, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        validate_publication_actor(actor)?;
        let payload = commit.evidence_payload()?;
        let evidence_hash = payload.evidence_hash()?;
        let payload_value = serde_json::to_value(&payload).map_err(|error| {
            publication_authority_error(
                "MCL_PUBLICATION_AUTHORITY_INVALID",
                error.to_string(),
                "Report this deterministic authoritative-evidence serialization defect.",
            )
        })?;
        let payload_json = canonical_string(&payload_value)?;
        let artifact_hashes_json =
            serde_json::to_string(&payload.artifact_hashes).map_err(|error| {
                publication_authority_error(
                    "MCL_PUBLICATION_AUTHORITY_INVALID",
                    error.to_string(),
                    "Report this deterministic authority-artifact serialization defect.",
                )
            })?;
        let input_hash = value_hash(&json!({
            "operation": PUBLICATION_AUTHORITY_OPERATION,
            "payload": payload_value,
            "actor": actor,
        }))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start publication authority creation", error))?;
        let receipt = read_publication_ingestion_receipt(
            &transaction,
            &commit.binding.ingestion_receipt_hash,
        )?;
        let stage = read_publication_stage(&transaction, &commit.binding.stage_hash)?;
        validate_publication_authority_commit(&transaction, commit, &receipt, &stage)?;

        if let Some(existing) = read_idempotent_result::<EvidenceSnapshot>(
            &transaction,
            PUBLICATION_AUTHORITY_OPERATION,
            idempotency_key,
            &input_hash,
        )? {
            let stored = read_evidence(&transaction, &existing.evidence_id)?;
            if existing != stored
                || stored.evidence_hash != evidence_hash
                || stored.payload != payload
                || stored.created_by != actor
            {
                return Err(publication_authority_integrity_error(
                    "stored idempotency result disagrees with the immutable authoritative evidence",
                ));
            }
            return Ok(stored);
        }

        let existing_evidence_id = transaction
            .query_row(
                "SELECT evidence_id FROM evidence WHERE publication_receipt_hash = ?1",
                [&commit.binding.ingestion_receipt_hash],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| AppError::database("find publication authority evidence", error))?;
        if let Some(existing_evidence_id) = existing_evidence_id {
            let existing = read_evidence(&transaction, &existing_evidence_id)?;
            if existing.payload != payload || existing.evidence_hash != evidence_hash {
                return Err(publication_authority_integrity_error(
                    "the publication receipt is bound to conflicting authoritative evidence",
                ));
            }
            return Err(publication_authority_error(
                "MCL_PUBLICATION_AUTHORITY_EXISTS",
                format!(
                    "publication receipt {} already produced authoritative evidence {}",
                    commit.binding.ingestion_receipt_hash, existing.evidence_id
                ),
                "Retrieve the existing evidence or retry with the original idempotency key.",
            ));
        }

        let evidence_id = Uuid::now_v7().to_string();
        transaction
            .execute(
                "INSERT INTO evidence(evidence_id, subject_object_id, subject_version_hash, evidence_kind, result, authority_class, run_id, environment_hash, artifact_hash, metadata_json, created_at, superseded_by, evidence_hash, job_id, artifact_hashes_json, verifier_identity, created_by, stale_reason, publication_receipt_hash, publication_stage_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, NULL, ?8, unixepoch(), NULL, ?9, NULL, ?10, ?11, ?12, NULL, ?13, ?14)",
                params![
                    evidence_id,
                    payload.subject.object_id,
                    payload.subject.version_hash,
                    payload.evidence_kind.as_str(),
                    payload.result.as_str(),
                    payload.authority_class.as_str(),
                    payload.environment_hash,
                    payload_json,
                    evidence_hash,
                    artifact_hashes_json,
                    payload.verifier_or_reviewer_identity,
                    actor,
                    commit.binding.ingestion_receipt_hash,
                    commit.binding.stage_hash,
                ],
            )
            .map_err(|error| AppError::database("insert publication authority evidence", error))?;
        let snapshot = read_evidence(&transaction, &evidence_id)?;
        write_idempotent_result(
            &transaction,
            PUBLICATION_AUTHORITY_OPERATION,
            idempotency_key,
            &input_hash,
            &snapshot,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit publication authority evidence", error))?;
        Ok(snapshot)
    }

    pub fn all_registered_cas_hashes(&self) -> Result<Vec<String>, AppError> {
        let mut hashes = self
            .all_artifact_hashes()?
            .into_iter()
            .collect::<BTreeSet<_>>();
        ensure_registered_cas_bound(hashes.len())?;

        let stage_hashes = read_bounded_hash_column(
            &self.connection,
            "SELECT stage_hash FROM publication_stages ORDER BY stage_hash LIMIT ?1",
            "inventory publication stages",
        )?;
        for stage_hash in stage_hashes {
            let stage = read_publication_stage(&self.connection, &stage_hash)?.stage;
            hashes.insert(stage.report_artifact_hash);
            hashes.insert(stage.retained_closure_artifact_hash);
            hashes.insert(stage.attestation_bundle_artifact_hash);
            hashes.extend(
                stage
                    .retained_artifacts
                    .into_iter()
                    .map(|artifact| artifact.artifact_hash),
            );
            ensure_registered_cas_bound(hashes.len())?;
        }

        let receipt_hashes = read_bounded_hash_column(
            &self.connection,
            "SELECT receipt_hash FROM publication_ingestion_receipts ORDER BY receipt_hash LIMIT ?1",
            "inventory publication ingestion receipts",
        )?;
        for receipt_hash in receipt_hashes {
            let receipt = read_publication_ingestion_receipt(&self.connection, &receipt_hash)?;
            hashes.insert(receipt.receipt_hash);
            hashes.insert(receipt.verification.raw_verification_hash);
            ensure_registered_cas_bound(hashes.len())?;
        }
        Ok(hashes.into_iter().collect())
    }

    pub fn enqueue_verifier_job(
        &mut self,
        request: &VerifierJobRequest,
        priority: i32,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<VerifierJobSnapshot, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        validate_job_priority(priority)?;
        request.validate()?;
        validate_verifier_job_references(&self.connection, request)?;
        let request_value = serde_json::to_value(request).map_err(|error| {
            AppError::new(
                "MCL_VERIFIER_REQUEST_INVALID",
                error.to_string(),
                false,
                "Report this deterministic verifier request serialization defect.",
            )
        })?;
        let canonical_input_hash = value_hash(&request_value)?;
        let input_hash = value_hash(&json!({
            "operation": "verifier.enqueue",
            "request": request_value,
            "priority": priority,
            "actor": actor,
        }))?;
        let input_json = String::from_utf8(canonical_json(&request_value)?).map_err(|error| {
            AppError::new(
                "MCL_CANONICAL_JSON_INVALID",
                error.to_string(),
                false,
                "Report this canonical JSON encoding defect.",
            )
        })?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start verifier job enqueue", error))?;
        if let Some(existing) = read_idempotent_result(
            &transaction,
            "verifier.enqueue",
            idempotency_key,
            &input_hash,
        )? {
            return Ok(existing);
        }
        let job_id = Uuid::now_v7().to_string();
        transaction
            .execute(
                "INSERT INTO jobs(job_id, job_type, canonical_input_hash, idempotency_key, state, priority, lease_owner, lease_expires_at, attempt_count, progress_json, result_artifact_hash, last_error_json, created_at, updated_at, input_json, actor) VALUES (?1, 'lean_elaboration', ?2, ?3, 'queued', ?4, NULL, NULL, 0, '{}', NULL, NULL, unixepoch(), unixepoch(), ?5, ?6)",
                params![job_id, canonical_input_hash, idempotency_key, priority, input_json, actor],
            )
            .map_err(|error| AppError::database("insert verifier job", error))?;
        let snapshot = read_verifier_job(&transaction, &job_id)?;
        write_idempotent_result(
            &transaction,
            "verifier.enqueue",
            idempotency_key,
            &input_hash,
            &snapshot,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit verifier job enqueue", error))?;
        Ok(snapshot)
    }

    pub fn validate_verifier_job_enqueue(
        &self,
        request: &VerifierJobRequest,
        priority: i32,
    ) -> Result<String, AppError> {
        validate_job_priority(priority)?;
        request.validate()?;
        validate_verifier_job_references(&self.connection, request)?;
        value_hash(&serde_json::to_value(request).map_err(|error| {
            AppError::new(
                "MCL_VERIFIER_REQUEST_INVALID",
                error.to_string(),
                false,
                "Report this deterministic verifier request serialization defect.",
            )
        })?)
    }

    pub fn get_verifier_job(&self, job_id: &str) -> Result<VerifierJobSnapshot, AppError> {
        validate_uuid(job_id, "verifier job")?;
        read_verifier_job(&self.connection, job_id)
    }

    pub fn list_verifier_jobs(&self, limit: usize) -> Result<Vec<VerifierJobSnapshot>, AppError> {
        if !(1..=100).contains(&limit) {
            return Err(AppError::new(
                "MCL_VERIFIER_JOB_LIMIT_INVALID",
                "verifier job list limit must be between 1 and 100",
                false,
                "Use a bounded verifier job list limit.",
            ));
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT job_id FROM jobs WHERE job_type = 'lean_elaboration' ORDER BY created_at, job_id LIMIT ?1",
            )
            .map_err(|error| AppError::database("prepare verifier job list", error))?;
        let ids = statement
            .query_map([limit as i64], |row| row.get::<_, String>(0))
            .map_err(|error| AppError::database("list verifier jobs", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::database("read verifier job list", error))?;
        ids.iter()
            .map(|job_id| read_verifier_job(&self.connection, job_id))
            .collect()
    }

    pub fn lease_next_verifier_job(
        &mut self,
        worker: &str,
        lease_seconds: u64,
    ) -> Result<Option<VerifierJobSnapshot>, AppError> {
        validate_worker_lease(worker, lease_seconds)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start verifier job lease", error))?;
        requeue_expired_jobs(&transaction)?;
        let job_id = transaction
            .query_row(
                "SELECT job_id FROM jobs WHERE job_type = 'lean_elaboration' AND state = 'queued' ORDER BY priority DESC, created_at, job_id LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| AppError::database("select verifier job lease", error))?;
        let Some(job_id) = job_id else {
            transaction
                .commit()
                .map_err(|error| AppError::database("commit empty verifier job lease", error))?;
            return Ok(None);
        };
        transaction
            .execute(
                "UPDATE jobs SET state = 'leased', lease_owner = ?2, lease_expires_at = unixepoch() + ?3, attempt_count = attempt_count + 1, progress_json = '{\"phase\":\"leased\"}', updated_at = unixepoch() WHERE job_id = ?1 AND state = 'queued'",
                params![job_id, worker, lease_seconds as i64],
            )
            .map_err(|error| AppError::database("lease verifier job", error))?;
        let snapshot = read_verifier_job(&transaction, &job_id)?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit verifier job lease", error))?;
        Ok(Some(snapshot))
    }

    pub fn mark_verifier_job_running(
        &mut self,
        job_id: &str,
        worker: &str,
    ) -> Result<VerifierJobSnapshot, AppError> {
        validate_uuid(job_id, "verifier job")?;
        validate_worker_lease(worker, 1)?;
        let changed = self
            .connection
            .execute(
                "UPDATE jobs SET state = 'running', progress_json = '{\"phase\":\"running\"}', updated_at = unixepoch() WHERE job_id = ?1 AND state = 'leased' AND lease_owner = ?2 AND lease_expires_at >= unixepoch()",
                params![job_id, worker],
            )
            .map_err(|error| AppError::database("start verifier job", error))?;
        if changed != 1 {
            return Err(verifier_job_conflict(job_id));
        }
        read_verifier_job(&self.connection, job_id)
    }

    pub fn finish_verifier_job(
        &mut self,
        job_id: &str,
        worker: &str,
        result_artifact_hash: &str,
        succeeded: bool,
        last_error: Option<&Value>,
    ) -> Result<VerifierJobSnapshot, AppError> {
        validate_uuid(job_id, "verifier job")?;
        validate_worker_lease(worker, 1)?;
        validate_hash(result_artifact_hash, "verifier result artifact")?;
        let result_media_type = self
            .connection
            .query_row(
                "SELECT media_type FROM artifacts WHERE artifact_hash = ?1",
                [result_artifact_hash],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| AppError::database("validate verifier result artifact", error))?;
        if result_media_type.as_deref() != Some("application/json") {
            return Err(AppError::new(
                "MCL_VERIFIER_RESULT_INVALID",
                "verifier result must resolve to a registered JSON artifact",
                false,
                "Register the exact structured verifier report before finishing the job.",
            ));
        }
        let state = if succeeded { "succeeded" } else { "failed" };
        let progress = if succeeded {
            "{\"phase\":\"completed\"}"
        } else {
            "{\"phase\":\"failed\"}"
        };
        let last_error_json =
            last_error
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| {
                    AppError::new(
                        "MCL_VERIFIER_RESULT_INVALID",
                        error.to_string(),
                        false,
                        "Supply one structured verifier error object.",
                    )
                })?;
        let changed = self
            .connection
            .execute(
                "UPDATE jobs SET state = ?3, lease_owner = NULL, lease_expires_at = NULL, progress_json = ?4, result_artifact_hash = ?5, last_error_json = ?6, updated_at = unixepoch() WHERE job_id = ?1 AND state = 'running' AND lease_owner = ?2 AND lease_expires_at >= unixepoch()",
                params![
                    job_id,
                    worker,
                    state,
                    progress,
                    result_artifact_hash,
                    last_error_json,
                ],
            )
            .map_err(|error| AppError::database("finish verifier job", error))?;
        if changed != 1 {
            return Err(verifier_job_conflict(job_id));
        }
        read_verifier_job(&self.connection, job_id)
    }

    pub fn enqueue_audit_job(
        &mut self,
        request: &LeanAuditRequest,
        priority: i32,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<LeanAuditJobSnapshot, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        validate_job_priority(priority)?;
        request.validate()?;
        validate_audit_job_references(&self.connection, request)?;
        let request_value = serde_json::to_value(request).map_err(|error| {
            AppError::new(
                "MCL_AUDIT_REQUEST_INVALID",
                error.to_string(),
                false,
                "Report this deterministic audit request serialization defect.",
            )
        })?;
        let canonical_input_hash = value_hash(&request_value)?;
        let input_hash = value_hash(&json!({
            "operation": "audit.enqueue",
            "request": request_value,
            "priority": priority,
            "actor": actor,
        }))?;
        let input_json = String::from_utf8(canonical_json(&request_value)?).map_err(|error| {
            AppError::new(
                "MCL_CANONICAL_JSON_INVALID",
                error.to_string(),
                false,
                "Report this canonical JSON encoding defect.",
            )
        })?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start audit job enqueue", error))?;
        if let Some(existing) =
            read_idempotent_result(&transaction, "audit.enqueue", idempotency_key, &input_hash)?
        {
            return Ok(existing);
        }
        let job_id = Uuid::now_v7().to_string();
        transaction
            .execute(
                "INSERT INTO jobs(job_id, job_type, canonical_input_hash, idempotency_key, state, priority, lease_owner, lease_expires_at, attempt_count, progress_json, result_artifact_hash, last_error_json, created_at, updated_at, input_json, actor) VALUES (?1, 'lean_audit', ?2, ?3, 'queued', ?4, NULL, NULL, 0, '{}', NULL, NULL, unixepoch(), unixepoch(), ?5, ?6)",
                params![job_id, canonical_input_hash, idempotency_key, priority, input_json, actor],
            )
            .map_err(|error| AppError::database("insert audit job", error))?;
        let snapshot = read_audit_job(&transaction, &job_id)?;
        write_idempotent_result(
            &transaction,
            "audit.enqueue",
            idempotency_key,
            &input_hash,
            &snapshot,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit audit job enqueue", error))?;
        Ok(snapshot)
    }

    pub fn validate_audit_job_enqueue(
        &self,
        request: &LeanAuditRequest,
        priority: i32,
    ) -> Result<String, AppError> {
        validate_job_priority(priority)?;
        request.validate()?;
        validate_audit_job_references(&self.connection, request)?;
        request.request_hash()
    }

    pub fn get_audit_job(&self, job_id: &str) -> Result<LeanAuditJobSnapshot, AppError> {
        validate_uuid(job_id, "audit job")?;
        read_audit_job(&self.connection, job_id)
    }

    pub fn list_audit_jobs(&self, limit: usize) -> Result<Vec<LeanAuditJobSnapshot>, AppError> {
        if !(1..=100).contains(&limit) {
            return Err(AppError::new(
                "MCL_AUDIT_JOB_LIMIT_INVALID",
                "audit job list limit must be between 1 and 100",
                false,
                "Use a bounded audit job list limit.",
            ));
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT job_id FROM jobs WHERE job_type = 'lean_audit' ORDER BY created_at, job_id LIMIT ?1",
            )
            .map_err(|error| AppError::database("prepare audit job list", error))?;
        let ids = statement
            .query_map([limit as i64], |row| row.get::<_, String>(0))
            .map_err(|error| AppError::database("list audit jobs", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::database("read audit job list", error))?;
        ids.iter()
            .map(|job_id| read_audit_job(&self.connection, job_id))
            .collect()
    }

    pub fn lease_next_audit_job(
        &mut self,
        worker: &str,
        lease_seconds: u64,
    ) -> Result<Option<LeanAuditJobSnapshot>, AppError> {
        validate_worker_lease(worker, lease_seconds)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start audit job lease", error))?;
        requeue_expired_jobs(&transaction)?;
        let job_id = transaction
            .query_row(
                "SELECT job_id FROM jobs WHERE job_type = 'lean_audit' AND state = 'queued' ORDER BY priority DESC, created_at, job_id LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| AppError::database("select audit job lease", error))?;
        let Some(job_id) = job_id else {
            transaction
                .commit()
                .map_err(|error| AppError::database("commit empty audit job lease", error))?;
            return Ok(None);
        };
        transaction
            .execute(
                "UPDATE jobs SET state = 'leased', lease_owner = ?2, lease_expires_at = unixepoch() + ?3, attempt_count = attempt_count + 1, progress_json = '{\"phase\":\"leased\"}', updated_at = unixepoch() WHERE job_id = ?1 AND state = 'queued'",
                params![job_id, worker, lease_seconds as i64],
            )
            .map_err(|error| AppError::database("lease audit job", error))?;
        let snapshot = read_audit_job(&transaction, &job_id)?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit audit job lease", error))?;
        Ok(Some(snapshot))
    }

    pub fn mark_audit_job_running(
        &mut self,
        job_id: &str,
        worker: &str,
    ) -> Result<LeanAuditJobSnapshot, AppError> {
        validate_uuid(job_id, "audit job")?;
        validate_worker_lease(worker, 1)?;
        let changed = self
            .connection
            .execute(
                "UPDATE jobs SET state = 'running', progress_json = '{\"phase\":\"running\"}', updated_at = unixepoch() WHERE job_id = ?1 AND job_type = 'lean_audit' AND state = 'leased' AND lease_owner = ?2 AND lease_expires_at >= unixepoch()",
                params![job_id, worker],
            )
            .map_err(|error| AppError::database("start audit job", error))?;
        if changed != 1 {
            return Err(audit_job_conflict(job_id));
        }
        read_audit_job(&self.connection, job_id)
    }

    pub fn finish_audit_job(
        &mut self,
        job_id: &str,
        worker: &str,
        result_artifact_hash: &str,
        succeeded: bool,
        last_error: Option<&Value>,
    ) -> Result<LeanAuditJobSnapshot, AppError> {
        validate_uuid(job_id, "audit job")?;
        validate_worker_lease(worker, 1)?;
        validate_hash(result_artifact_hash, "audit result artifact")?;
        let result_media_type = self
            .connection
            .query_row(
                "SELECT media_type FROM artifacts WHERE artifact_hash = ?1",
                [result_artifact_hash],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| AppError::database("validate audit result artifact", error))?;
        if result_media_type.as_deref() != Some("application/json") {
            return Err(AppError::new(
                "MCL_AUDIT_RESULT_INVALID",
                "audit result must resolve to a registered JSON artifact",
                false,
                "Register the exact structured audit report before finishing the job.",
            ));
        }
        let state = if succeeded { "succeeded" } else { "failed" };
        let progress = if succeeded {
            "{\"phase\":\"completed\"}"
        } else {
            "{\"phase\":\"failed\"}"
        };
        let last_error_json =
            last_error
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| {
                    AppError::new(
                        "MCL_AUDIT_RESULT_INVALID",
                        error.to_string(),
                        false,
                        "Supply one structured audit error object.",
                    )
                })?;
        let changed = self
            .connection
            .execute(
                "UPDATE jobs SET state = ?3, lease_owner = NULL, lease_expires_at = NULL, progress_json = ?4, result_artifact_hash = ?5, last_error_json = ?6, updated_at = unixepoch() WHERE job_id = ?1 AND job_type = 'lean_audit' AND state = 'running' AND lease_owner = ?2 AND lease_expires_at >= unixepoch()",
                params![job_id, worker, state, progress, result_artifact_hash, last_error_json],
            )
            .map_err(|error| AppError::database("finish audit job", error))?;
        if changed != 1 {
            return Err(audit_job_conflict(job_id));
        }
        read_audit_job(&self.connection, job_id)
    }

    pub fn create_diagnostic_evidence(
        &mut self,
        payload: &EvidencePayload,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<EvidenceSnapshot, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        payload.validate()?;
        if payload.evidence_kind != EvidenceKind::LeanElaboration
            || payload.authority_class != EvidenceAuthorityClass::Diagnostic
        {
            return Err(AppError::new(
                "MCL_EVIDENCE_AUTHORITY_FORBIDDEN",
                "this path can create only non-authoritative Lean elaboration evidence",
                false,
                "Complete reviewed proof-closure controls before adding another evidence path.",
            ));
        }
        validate_diagnostic_evidence_references(&self.connection, payload)?;
        let evidence_hash = payload.evidence_hash()?;
        let input_hash = value_hash(&json!({
            "operation": "evidence.create_diagnostic",
            "payload": payload,
            "actor": actor,
        }))?;
        let payload_json = String::from_utf8(canonical_json(
            &serde_json::to_value(payload).map_err(|error| {
                AppError::new(
                    "MCL_EVIDENCE_INVALID",
                    error.to_string(),
                    false,
                    "Report this deterministic evidence serialization defect.",
                )
            })?,
        )?)
        .map_err(|error| {
            AppError::new(
                "MCL_CANONICAL_JSON_INVALID",
                error.to_string(),
                false,
                "Report this canonical JSON encoding defect.",
            )
        })?;
        let artifact_hashes_json =
            serde_json::to_string(&payload.artifact_hashes).map_err(|error| {
                AppError::new(
                    "MCL_EVIDENCE_INVALID",
                    error.to_string(),
                    false,
                    "Report this deterministic evidence artifact serialization defect.",
                )
            })?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start diagnostic evidence creation", error))?;
        if let Some(existing) = read_idempotent_result(
            &transaction,
            "evidence.create_diagnostic",
            idempotency_key,
            &input_hash,
        )? {
            return Ok(existing);
        }
        if transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM evidence WHERE evidence_hash = ?1)",
                [&evidence_hash],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| AppError::database("search diagnostic evidence", error))?
        {
            return Err(AppError::new(
                "MCL_EVIDENCE_EXISTS",
                format!("evidence {evidence_hash} already exists"),
                false,
                "Retrieve the existing evidence or retry with the original idempotency key.",
            ));
        }
        let evidence_id = Uuid::now_v7().to_string();
        transaction
            .execute(
                "INSERT INTO evidence(evidence_id, subject_object_id, subject_version_hash, evidence_kind, result, authority_class, run_id, environment_hash, artifact_hash, metadata_json, created_at, superseded_by, evidence_hash, job_id, artifact_hashes_json, verifier_identity, created_by, stale_reason) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, unixepoch(), NULL, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    evidence_id,
                    payload.subject.object_id,
                    payload.subject.version_hash,
                    payload.evidence_kind.as_str(),
                    payload.result.as_str(),
                    payload.authority_class.as_str(),
                    payload.producing_run_id,
                    payload.environment_hash,
                    payload.artifact_hashes.first(),
                    payload_json,
                    evidence_hash,
                    payload.producing_job_id,
                    artifact_hashes_json,
                    payload.verifier_or_reviewer_identity,
                    actor,
                    payload.stale_reason,
                ],
            )
            .map_err(|error| AppError::database("insert diagnostic evidence", error))?;
        let snapshot = read_evidence(&transaction, &evidence_id)?;
        write_idempotent_result(
            &transaction,
            "evidence.create_diagnostic",
            idempotency_key,
            &input_hash,
            &snapshot,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit diagnostic evidence", error))?;
        Ok(snapshot)
    }

    pub fn create_fidelity_evidence(
        &mut self,
        payload: &EvidencePayload,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<EvidenceSnapshot, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        payload.validate()?;
        if payload.evidence_kind != EvidenceKind::StatementFidelityReview
            || payload.authority_class != EvidenceAuthorityClass::Reviewed
            || payload.verifier_or_reviewer_identity != actor
        {
            return Err(AppError::new(
                "MCL_FIDELITY_EVIDENCE_FORBIDDEN",
                "this path can create only actor-bound reviewed statement-fidelity evidence",
                false,
                "Submit the closed fidelity review through the shared application service.",
            ));
        }
        validate_fidelity_evidence_references(&self.connection, payload)?;
        let evidence_hash = payload.evidence_hash()?;
        let input_hash = value_hash(&json!({
            "operation": "evidence.create_fidelity",
            "payload": payload,
            "actor": actor,
        }))?;
        let payload_json = String::from_utf8(canonical_json(
            &serde_json::to_value(payload).map_err(|error| {
                AppError::new(
                    "MCL_EVIDENCE_INVALID",
                    error.to_string(),
                    false,
                    "Report this deterministic fidelity evidence serialization defect.",
                )
            })?,
        )?)
        .map_err(|error| {
            AppError::new(
                "MCL_CANONICAL_JSON_INVALID",
                error.to_string(),
                false,
                "Report this canonical JSON encoding defect.",
            )
        })?;
        let artifact_hashes_json =
            serde_json::to_string(&payload.artifact_hashes).map_err(|error| {
                AppError::new(
                    "MCL_EVIDENCE_INVALID",
                    error.to_string(),
                    false,
                    "Report this fidelity evidence artifact serialization defect.",
                )
            })?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start fidelity evidence creation", error))?;
        if let Some(existing) = read_idempotent_result(
            &transaction,
            "evidence.create_fidelity",
            idempotency_key,
            &input_hash,
        )? {
            return Ok(existing);
        }
        validate_fidelity_evidence_head(&transaction, payload)?;
        if transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM evidence WHERE evidence_hash = ?1)",
                [&evidence_hash],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| AppError::database("search fidelity evidence", error))?
        {
            return Err(AppError::new(
                "MCL_EVIDENCE_EXISTS",
                format!("evidence {evidence_hash} already exists"),
                false,
                "Retrieve the existing review or retry with the original idempotency key.",
            ));
        }
        let evidence_id = Uuid::now_v7().to_string();
        transaction
            .execute(
                "INSERT INTO evidence(evidence_id, subject_object_id, subject_version_hash, evidence_kind, result, authority_class, run_id, environment_hash, artifact_hash, metadata_json, created_at, superseded_by, evidence_hash, job_id, artifact_hashes_json, verifier_identity, created_by, stale_reason) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, unixepoch(), NULL, ?10, NULL, ?11, ?12, ?13, ?14)",
                params![
                    evidence_id,
                    payload.subject.object_id,
                    payload.subject.version_hash,
                    payload.evidence_kind.as_str(),
                    payload.result.as_str(),
                    payload.authority_class.as_str(),
                    payload.producing_run_id,
                    payload.artifact_hashes.first(),
                    payload_json,
                    evidence_hash,
                    artifact_hashes_json,
                    payload.verifier_or_reviewer_identity,
                    actor,
                    payload.stale_reason,
                ],
            )
            .map_err(|error| AppError::database("insert fidelity evidence", error))?;
        let snapshot = read_evidence(&transaction, &evidence_id)?;
        write_idempotent_result(
            &transaction,
            "evidence.create_fidelity",
            idempotency_key,
            &input_hash,
            &snapshot,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit fidelity evidence", error))?;
        Ok(snapshot)
    }

    pub fn create_audit_evidence_pair(
        &mut self,
        payloads: &[EvidencePayload; 2],
        actor: &str,
        idempotency_key: &str,
    ) -> Result<Vec<EvidenceSnapshot>, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        let mut kinds = payloads
            .iter()
            .map(|payload| payload.evidence_kind)
            .collect::<Vec<_>>();
        kinds.sort_by_key(|kind| kind.as_str());
        if kinds != [EvidenceKind::AxiomAudit, EvidenceKind::ProofClosureScan] {
            return Err(AppError::new(
                "MCL_AUDIT_EVIDENCE_INVALID",
                "audit promotion must create exactly one axiom audit and one proof-closure scan",
                false,
                "Derive the exact audit evidence pair from one completed audit report.",
            ));
        }
        for payload in payloads {
            payload.validate()?;
            if payload.authority_class != EvidenceAuthorityClass::Diagnostic {
                return Err(AppError::new(
                    "MCL_EVIDENCE_AUTHORITY_FORBIDDEN",
                    "local audit evidence cannot be authoritative",
                    false,
                    "Complete publication isolation before adding an authoritative evidence path.",
                ));
            }
            validate_audit_evidence_references(&self.connection, payload)?;
        }
        let input_hash = value_hash(&json!({
            "operation": "evidence.create_audit_pair",
            "payloads": payloads,
            "actor": actor,
        }))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start audit evidence creation", error))?;
        if let Some(existing) = read_idempotent_result(
            &transaction,
            "evidence.create_audit_pair",
            idempotency_key,
            &input_hash,
        )? {
            return Ok(existing);
        }
        let mut prepared = Vec::with_capacity(2);
        for payload in payloads {
            let evidence_hash = payload.evidence_hash()?;
            if transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM evidence WHERE evidence_hash = ?1)",
                    [&evidence_hash],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(|error| AppError::database("search audit evidence", error))?
            {
                return Err(AppError::new(
                    "MCL_EVIDENCE_EXISTS",
                    format!("evidence {evidence_hash} already exists"),
                    false,
                    "Retrieve the existing evidence or retry with the original idempotency key.",
                ));
            }
            let payload_json = String::from_utf8(canonical_json(
                &serde_json::to_value(payload).map_err(|error| {
                    AppError::new(
                        "MCL_EVIDENCE_INVALID",
                        error.to_string(),
                        false,
                        "Report this deterministic audit evidence serialization defect.",
                    )
                })?,
            )?)
            .map_err(|error| {
                AppError::new(
                    "MCL_CANONICAL_JSON_INVALID",
                    error.to_string(),
                    false,
                    "Report this canonical JSON encoding defect.",
                )
            })?;
            let artifact_hashes_json =
                serde_json::to_string(&payload.artifact_hashes).map_err(|error| {
                    AppError::new(
                        "MCL_EVIDENCE_INVALID",
                        error.to_string(),
                        false,
                        "Report this audit evidence artifact serialization defect.",
                    )
                })?;
            prepared.push((evidence_hash, payload_json, artifact_hashes_json));
        }
        let mut snapshots = Vec::with_capacity(2);
        for (payload, (evidence_hash, payload_json, artifact_hashes_json)) in
            payloads.iter().zip(prepared)
        {
            let evidence_id = Uuid::now_v7().to_string();
            transaction
                .execute(
                    "INSERT INTO evidence(evidence_id, subject_object_id, subject_version_hash, evidence_kind, result, authority_class, run_id, environment_hash, artifact_hash, metadata_json, created_at, superseded_by, evidence_hash, job_id, artifact_hashes_json, verifier_identity, created_by, stale_reason) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, unixepoch(), NULL, ?11, ?12, ?13, ?14, ?15, ?16)",
                    params![
                        evidence_id,
                        payload.subject.object_id,
                        payload.subject.version_hash,
                        payload.evidence_kind.as_str(),
                        payload.result.as_str(),
                        payload.authority_class.as_str(),
                        payload.producing_run_id,
                        payload.environment_hash,
                        payload.artifact_hashes.first(),
                        payload_json,
                        evidence_hash,
                        payload.producing_job_id,
                        artifact_hashes_json,
                        payload.verifier_or_reviewer_identity,
                        actor,
                        payload.stale_reason,
                    ],
                )
                .map_err(|error| AppError::database("insert audit evidence", error))?;
            snapshots.push(read_evidence(&transaction, &evidence_id)?);
        }
        write_idempotent_result(
            &transaction,
            "evidence.create_audit_pair",
            idempotency_key,
            &input_hash,
            &snapshots,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit audit evidence", error))?;
        Ok(snapshots)
    }

    pub fn get_evidence(&self, evidence_id: &str) -> Result<EvidenceSnapshot, AppError> {
        validate_uuid(evidence_id, "evidence")?;
        read_evidence(&self.connection, evidence_id)
    }

    pub fn list_evidence(&self, limit: usize) -> Result<Vec<EvidenceSnapshot>, AppError> {
        if !(1..=100).contains(&limit) {
            return Err(AppError::new(
                "MCL_EVIDENCE_LIMIT_INVALID",
                "evidence list limit must be between 1 and 100",
                false,
                "Use a bounded evidence list limit.",
            ));
        }
        let mut statement = self
            .connection
            .prepare("SELECT evidence_id FROM evidence WHERE evidence_hash IS NOT NULL ORDER BY created_at, evidence_id LIMIT ?1")
            .map_err(|error| AppError::database("prepare evidence list", error))?;
        let ids = statement
            .query_map([limit as i64], |row| row.get::<_, String>(0))
            .map_err(|error| AppError::database("list evidence", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::database("read evidence list", error))?;
        ids.iter()
            .map(|evidence_id| read_evidence(&self.connection, evidence_id))
            .collect()
    }

    pub fn list_fidelity_evidence(
        &self,
        subject: &ExactVersionReference,
    ) -> Result<Vec<EvidenceSnapshot>, AppError> {
        let mut statement = self
            .connection
            .prepare("SELECT evidence_id FROM evidence WHERE subject_object_id = ?1 AND subject_version_hash = ?2 AND evidence_kind = 'statement_fidelity_review' ORDER BY created_at, evidence_id")
            .map_err(|error| AppError::database("prepare fidelity evidence list", error))?;
        let ids = statement
            .query_map(params![subject.object_id, subject.version_hash], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| AppError::database("list fidelity evidence", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::database("read fidelity evidence list", error))?;
        ids.iter()
            .map(|evidence_id| read_evidence(&self.connection, evidence_id))
            .collect()
    }

    #[cfg(test)]
    fn list_current_formalizations_for_claim(
        &self,
        claim: &ExactVersionReference,
    ) -> Result<Vec<RecordSnapshot>, AppError> {
        self.list_current_formalizations_for_claim_bounded(claim, MAX_CLAIM_STATUS_FORMALIZATIONS)
    }

    #[cfg(test)]
    fn list_current_formalizations_for_claim_bounded(
        &self,
        claim: &ExactVersionReference,
        limit: usize,
    ) -> Result<Vec<RecordSnapshot>, AppError> {
        read_current_formalizations_for_claim_bounded(&self.connection, claim, limit)
    }

    #[cfg(test)]
    fn list_authoritative_evidence_for_subject(
        &self,
        subject: &ExactVersionReference,
    ) -> Result<Vec<EvidenceSnapshot>, AppError> {
        self.list_authoritative_evidence_for_subject_bounded(
            subject,
            MAX_CLAIM_STATUS_EVIDENCE_PER_FORMALIZATION,
        )
    }

    #[cfg(test)]
    fn list_authoritative_evidence_for_subject_bounded(
        &self,
        subject: &ExactVersionReference,
        limit: usize,
    ) -> Result<Vec<EvidenceSnapshot>, AppError> {
        read_authoritative_evidence_for_subject_bounded(&self.connection, subject, limit)
    }

    pub(crate) fn capture_claim_status_read_basis(
        &self,
        claim: &ExactVersionReference,
    ) -> Result<ClaimStatusReadBasis, AppError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| AppError::database("start claim status read snapshot", error))?;
        let basis = capture_claim_status_read_basis(&transaction, claim)?;
        transaction
            .commit()
            .map_err(|error| AppError::database("finish claim status read snapshot", error))?;
        Ok(basis)
    }

    pub(crate) fn recheck_claim_status_read_basis(
        &self,
        basis: &ClaimStatusReadBasis,
    ) -> Result<(), AppError> {
        let current = self.capture_claim_status_read_basis(&basis.claim)?;
        if current != *basis {
            return Err(AppError::new(
                "MCL_CLAIM_STATUS_READ_CONFLICT",
                "claim status inputs changed while the derived read was replaying evidence",
                true,
                "Retry the read against the new source, claim, formalization, fidelity, and authority heads.",
            ));
        }
        Ok(())
    }

    #[cfg(test)]
    fn requeue_expired_verifier_jobs(&mut self) -> Result<usize, AppError> {
        requeue_expired_jobs(&self.connection)
    }

    pub(crate) fn existing_record_create_result(
        &self,
        draft: &RecordDraft,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<Option<RecordSnapshot>, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        validate_record_payload(draft.kind, &draft.schema_version, &draft.payload)?;
        record_version_hash(&draft.schema_version, &draft.payload)?;
        let input_hash = mutation_input_hash("record.create", None, None, draft, actor)?;
        read_idempotent_result(
            &self.connection,
            "record.create",
            idempotency_key,
            &input_hash,
        )
    }

    pub(crate) fn existing_record_version_result(
        &self,
        object_id: &str,
        expected_head: &str,
        draft: &RecordDraft,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<Option<RecordSnapshot>, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        validate_record_payload(draft.kind, &draft.schema_version, &draft.payload)?;
        validate_hash(expected_head, "expected head")?;
        let version_hash = record_version_hash(&draft.schema_version, &draft.payload)?;
        if version_hash == expected_head {
            return Err(AppError::new(
                "MCL_RECORD_VERSION_UNCHANGED",
                "new canonical content is identical to the expected head",
                false,
                "Do not create a new version unless canonical content changes.",
            ));
        }
        let input_hash = mutation_input_hash(
            "record.version",
            Some(object_id),
            Some(expected_head),
            draft,
            actor,
        )?;
        read_idempotent_result(
            &self.connection,
            "record.version",
            idempotency_key,
            &input_hash,
        )
    }

    pub(crate) fn existing_edge_create_result(
        &self,
        draft: &EdgeDraft,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<Option<EdgeSnapshot>, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        reject_controlled_repair_edge(draft.kind)?;
        validate_hash(&draft.source_version_hash, "source version")?;
        validate_hash(&draft.target_version_hash, "target version")?;
        canonical_json(&draft.payload)?;
        let input_hash = edge_create_input_hash(draft, actor)?;
        read_idempotent_result(
            &self.connection,
            "edge.create",
            idempotency_key,
            &input_hash,
        )
    }

    pub fn create_record(
        &mut self,
        draft: &RecordDraft,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<RecordSnapshot, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        validate_record_payload(draft.kind, &draft.schema_version, &draft.payload)?;
        let version_hash = record_version_hash(&draft.schema_version, &draft.payload)?;
        let input_hash = mutation_input_hash("record.create", None, None, draft, actor)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start record creation", error))?;
        if let Some(existing) =
            read_idempotent_result(&transaction, "record.create", idempotency_key, &input_hash)?
        {
            return Ok(existing);
        }
        validate_record_references(&transaction, draft)?;
        if let Some(object_id) = transaction
            .query_row(
                "SELECT object_id FROM record_versions WHERE version_hash = ?1",
                [&version_hash],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| AppError::database("search duplicate record version", error))?
        {
            return Err(AppError::new(
                "MCL_RECORD_VERSION_EXISTS",
                format!("identical canonical content already belongs to object {object_id}"),
                false,
                "Retrieve the existing object instead of creating a duplicate.",
            ));
        }

        let object_id = Uuid::now_v7().to_string();
        let payload_json = String::from_utf8(canonical_json(&draft.payload)?).map_err(|error| {
            AppError::new(
                "MCL_CANONICAL_JSON_INVALID",
                error.to_string(),
                false,
                "Report this canonical JSON encoding defect.",
            )
        })?;
        transaction
            .execute(
                "INSERT INTO records(object_id, record_type, head_version_hash, created_at, created_by) VALUES (?1, ?2, NULL, unixepoch(), ?3)",
                params![object_id, draft.kind.as_str(), actor],
            )
            .map_err(|error| AppError::database("insert record", error))?;
        transaction
            .execute(
                "INSERT INTO record_versions(version_hash, object_id, schema_version, payload_json, predecessor_hash, created_at, created_by) VALUES (?1, ?2, ?3, ?4, NULL, unixepoch(), ?5)",
                params![version_hash, object_id, draft.schema_version, payload_json, actor],
            )
            .map_err(|error| AppError::database("insert initial record version", error))?;
        transaction
            .execute(
                "UPDATE records SET head_version_hash = ?1 WHERE object_id = ?2 AND head_version_hash IS NULL",
                params![version_hash, object_id],
            )
            .map_err(|error| AppError::database("set initial record head", error))?;
        update_search_projection(&transaction, &object_id, draft)?;
        let snapshot = read_snapshot(&transaction, &object_id, Some(&version_hash))?;
        write_idempotent_result(
            &transaction,
            "record.create",
            idempotency_key,
            &input_hash,
            &snapshot,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit record creation", error))?;
        Ok(snapshot)
    }

    pub fn validate_record_create(&self, draft: &RecordDraft) -> Result<String, AppError> {
        validate_record_payload(draft.kind, &draft.schema_version, &draft.payload)?;
        validate_record_references(&self.connection, draft)?;
        let version_hash = record_version_hash(&draft.schema_version, &draft.payload)?;
        let existing = self
            .connection
            .query_row(
                "SELECT object_id FROM record_versions WHERE version_hash = ?1",
                [&version_hash],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| AppError::database("preview duplicate record version", error))?;
        if let Some(object_id) = existing {
            return Err(AppError::new(
                "MCL_RECORD_VERSION_EXISTS",
                format!("identical canonical content already belongs to object {object_id}"),
                false,
                "Retrieve the existing object instead of creating a duplicate.",
            ));
        }
        Ok(version_hash)
    }

    pub fn version_record(
        &mut self,
        object_id: &str,
        expected_head: &str,
        draft: &RecordDraft,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<RecordSnapshot, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        validate_record_payload(draft.kind, &draft.schema_version, &draft.payload)?;
        validate_hash(expected_head, "expected head")?;
        let version_hash = record_version_hash(&draft.schema_version, &draft.payload)?;
        if version_hash == expected_head {
            return Err(AppError::new(
                "MCL_RECORD_VERSION_UNCHANGED",
                "new canonical content is identical to the expected head",
                false,
                "Do not create a new version unless canonical content changes.",
            ));
        }
        let input_hash = mutation_input_hash(
            "record.version",
            Some(object_id),
            Some(expected_head),
            draft,
            actor,
        )?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start record version", error))?;
        if let Some(existing) =
            read_idempotent_result(&transaction, "record.version", idempotency_key, &input_hash)?
        {
            return Ok(existing);
        }
        validate_record_references(&transaction, draft)?;
        let current: Option<(String, String)> = transaction
            .query_row(
                "SELECT record_type, head_version_hash FROM records WHERE object_id = ?1 AND tombstoned = 0",
                [object_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| AppError::database("read record head", error))?;
        let Some((record_type, current_head)) = current else {
            return Err(AppError::new(
                "MCL_RECORD_NOT_FOUND",
                format!("canonical object {object_id} does not exist"),
                false,
                "Search for the stable object ID before attempting to version it.",
            ));
        };
        if record_type != draft.kind.as_str() {
            return Err(AppError::new(
                "MCL_RECORD_KIND_IMMUTABLE",
                format!(
                    "object {object_id} is `{record_type}`, not `{}`",
                    draft.kind
                ),
                false,
                "Create a distinct object when the logical record kind changes.",
            ));
        }
        if current_head != expected_head {
            return Err(conflict(object_id, expected_head, &current_head));
        }
        if let Some(existing_object) = transaction
            .query_row(
                "SELECT object_id FROM record_versions WHERE version_hash = ?1",
                [&version_hash],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| AppError::database("search duplicate record version", error))?
        {
            return Err(AppError::new(
                "MCL_RECORD_VERSION_EXISTS",
                format!("canonical version already exists on object {existing_object}"),
                false,
                "Use the existing version or submit different canonical content.",
            ));
        }

        let payload_json = String::from_utf8(canonical_json(&draft.payload)?).map_err(|error| {
            AppError::new(
                "MCL_CANONICAL_JSON_INVALID",
                error.to_string(),
                false,
                "Report this canonical JSON encoding defect.",
            )
        })?;
        transaction
            .execute(
                "INSERT INTO record_versions(version_hash, object_id, schema_version, payload_json, predecessor_hash, created_at, created_by) VALUES (?1, ?2, ?3, ?4, ?5, unixepoch(), ?6)",
                params![version_hash, object_id, draft.schema_version, payload_json, expected_head, actor],
            )
            .map_err(|error| AppError::database("insert record version", error))?;
        let updated = transaction
            .execute(
                "UPDATE records SET head_version_hash = ?1 WHERE object_id = ?2 AND head_version_hash = ?3",
                params![version_hash, object_id, expected_head],
            )
            .map_err(|error| AppError::database("compare and swap record head", error))?;
        if updated != 1 {
            let actual: String = transaction
                .query_row(
                    "SELECT head_version_hash FROM records WHERE object_id = ?1",
                    [object_id],
                    |row| row.get(0),
                )
                .map_err(|error| AppError::database("read conflicting record head", error))?;
            return Err(conflict(object_id, expected_head, &actual));
        }
        update_search_projection(&transaction, object_id, draft)?;
        let snapshot = read_snapshot(&transaction, object_id, Some(&version_hash))?;
        write_idempotent_result(
            &transaction,
            "record.version",
            idempotency_key,
            &input_hash,
            &snapshot,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit record version", error))?;
        Ok(snapshot)
    }

    pub fn validate_record_version(
        &self,
        object_id: &str,
        expected_head: &str,
        draft: &RecordDraft,
    ) -> Result<String, AppError> {
        validate_record_payload(draft.kind, &draft.schema_version, &draft.payload)?;
        validate_hash(expected_head, "expected head")?;
        validate_record_references(&self.connection, draft)?;
        let current = self.get_record(object_id)?;
        if current.kind != draft.kind {
            return Err(AppError::new(
                "MCL_RECORD_KIND_IMMUTABLE",
                format!(
                    "object {object_id} is `{}`, not `{}`",
                    current.kind, draft.kind
                ),
                false,
                "Create a distinct object when the logical record kind changes.",
            ));
        }
        if current.version_hash != expected_head {
            return Err(conflict(object_id, expected_head, &current.version_hash));
        }
        let version_hash = record_version_hash(&draft.schema_version, &draft.payload)?;
        if version_hash == expected_head {
            return Err(AppError::new(
                "MCL_RECORD_VERSION_UNCHANGED",
                "new canonical content is identical to the expected head",
                false,
                "Do not create a new version unless canonical content changes.",
            ));
        }
        if self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM record_versions WHERE version_hash = ?1)",
                [&version_hash],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| AppError::database("preview duplicate record version", error))?
        {
            return Err(AppError::new(
                "MCL_RECORD_VERSION_EXISTS",
                "canonical version already exists",
                false,
                "Use the existing version or submit different canonical content.",
            ));
        }
        Ok(version_hash)
    }

    pub fn get_record(&self, object_id: &str) -> Result<RecordSnapshot, AppError> {
        read_snapshot(&self.connection, object_id, None)
    }

    pub fn get_record_version(&self, version_hash: &str) -> Result<RecordSnapshot, AppError> {
        validate_hash(version_hash, "record version")?;
        read_snapshot_by_version(&self.connection, version_hash)
    }

    pub fn search_records(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<RecordSnapshot>, AppError> {
        if query.trim().is_empty() || !(1..=100).contains(&limit) {
            return Err(AppError::new(
                "MCL_SEARCH_INPUT_INVALID",
                "search query must be nonempty and limit must be between 1 and 100",
                false,
                "Supply an FTS5 query and a bounded result limit.",
            ));
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT object_id FROM record_search WHERE record_search MATCH ?1 ORDER BY rank LIMIT ?2",
            )
            .map_err(|error| AppError::database("prepare record search", error))?;
        let object_ids = statement
            .query_map(params![query, limit as i64], |row| row.get::<_, String>(0))
            .map_err(|error| AppError::database("search canonical records", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::database("read record search result", error))?;
        object_ids
            .iter()
            .map(|object_id| read_snapshot(&self.connection, object_id, None))
            .collect()
    }

    pub fn create_edge(
        &mut self,
        draft: &EdgeDraft,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<EdgeSnapshot, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        reject_controlled_repair_edge(draft.kind)?;
        validate_hash(&draft.source_version_hash, "source version")?;
        validate_hash(&draft.target_version_hash, "target version")?;
        let input_hash = edge_create_input_hash(draft, actor)?;
        let payload_json = String::from_utf8(canonical_json(&draft.payload)?).map_err(|error| {
            AppError::new(
                "MCL_CANONICAL_JSON_INVALID",
                error.to_string(),
                false,
                "Report this canonical JSON encoding defect.",
            )
        })?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start edge creation", error))?;
        if let Some(existing) =
            read_idempotent_result(&transaction, "edge.create", idempotency_key, &input_hash)?
        {
            return Ok(existing);
        }
        validate_edge_endpoint(
            &transaction,
            "source",
            &draft.source_object_id,
            &draft.source_version_hash,
        )?;
        validate_edge_endpoint(
            &transaction,
            "target",
            &draft.target_object_id,
            &draft.target_version_hash,
        )?;
        if draft.kind == EdgeKind::PedagogyHardPrerequisite
            && hard_prerequisite_would_cycle(
                &transaction,
                &draft.source_object_id,
                &draft.target_object_id,
            )?
        {
            return Err(AppError::new(
                "MCL_PEDAGOGY_CYCLE",
                "hard pedagogical prerequisite edge would create a cycle",
                false,
                "Use a soft prerequisite or revise the curriculum dependency direction.",
            ));
        }
        let edge_id = Uuid::now_v7().to_string();
        transaction
            .execute(
                "INSERT INTO edges(edge_id, edge_type, source_object_id, source_version_hash, target_object_id, target_version_hash, payload_json, created_at, created_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, unixepoch(), ?8)",
                params![edge_id, draft.kind.as_str(), draft.source_object_id, draft.source_version_hash, draft.target_object_id, draft.target_version_hash, payload_json, actor],
            )
            .map_err(|error| AppError::database("insert canonical edge", error))?;
        let snapshot = read_edge(&transaction, &edge_id)?;
        write_idempotent_result(
            &transaction,
            "edge.create",
            idempotency_key,
            &input_hash,
            &snapshot,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit edge creation", error))?;
        Ok(snapshot)
    }

    pub fn validate_edge_create(&self, draft: &EdgeDraft) -> Result<(), AppError> {
        reject_controlled_repair_edge(draft.kind)?;
        validate_hash(&draft.source_version_hash, "source version")?;
        validate_hash(&draft.target_version_hash, "target version")?;
        canonical_json(&draft.payload)?;
        validate_edge_endpoint(
            &self.connection,
            "source",
            &draft.source_object_id,
            &draft.source_version_hash,
        )?;
        validate_edge_endpoint(
            &self.connection,
            "target",
            &draft.target_object_id,
            &draft.target_version_hash,
        )?;
        if draft.kind == EdgeKind::PedagogyHardPrerequisite
            && hard_prerequisite_would_cycle(
                &self.connection,
                &draft.source_object_id,
                &draft.target_object_id,
            )?
        {
            return Err(AppError::new(
                "MCL_PEDAGOGY_CYCLE",
                "hard pedagogical prerequisite edge would create a cycle",
                false,
                "Use a soft prerequisite or revise the curriculum dependency direction.",
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_counterexample_repair(
        &self,
        commit: &CounterexampleRepairCommit,
    ) -> Result<(), AppError> {
        validate_counterexample_repair_commit_shape(commit)?;
        let current = capture_claim_status_read_basis(&self.connection, &commit.original_claim)?;
        if current != commit.claim_status_basis {
            return Err(counterexample_repair_conflict(
                "claim truth inputs changed before the repair dry-run completed",
            ));
        }
        recheck_counterexample_search_run(
            &self.connection,
            &commit.counterexample_search_run_id,
            &commit.counterexample_search_run_head_hash,
        )?;
        validate_record_references(&self.connection, &commit.repaired_claim)?;
        reject_duplicate_record_version(&self.connection, &commit.repaired_claim_version_hash)?;
        if artifact_exists(&self.connection, &commit.package_artifact_hash)? {
            return Err(AppError::new(
                "MCL_ARTIFACT_EXISTS",
                format!(
                    "counterexample package artifact {} is already registered",
                    commit.package_artifact_hash
                ),
                false,
                "Retrieve the existing repair or use the original idempotency key.",
            ));
        }
        Ok(())
    }

    pub(crate) fn commit_counterexample_repair(
        &mut self,
        commit: &CounterexampleRepairCommit,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<CounterexampleRepairSnapshot, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        validate_counterexample_repair_commit_shape(commit)?;
        #[cfg(test)]
        let fail_after_artifact =
            std::mem::take(&mut self.counterexample_repair_fail_after_artifact);
        let input_hash = value_hash(&json!({
            "operation": COUNTEREXAMPLE_REPAIR_OPERATION,
            "package_artifact_hash": commit.package_artifact_hash,
            "package_byte_size": commit.package_byte_size,
            "package_metadata": commit.package_metadata,
            "repaired_claim": {
                "kind": commit.repaired_claim.kind,
                "schema_version": commit.repaired_claim.schema_version,
                "payload": commit.repaired_claim.payload,
                "searchable_text": commit.repaired_claim.searchable_text,
            },
            "repaired_claim_version_hash": commit.repaired_claim_version_hash,
            "original_claim": commit.original_claim,
            "repair_edge_payload": commit.repair_edge_payload,
            "claim_status_basis": commit.claim_status_basis,
            "counterexample_search_run_id": commit.counterexample_search_run_id,
            "counterexample_search_run_head_hash": commit.counterexample_search_run_head_hash,
            "actor": actor,
        }))?;
        let package_metadata_json = canonical_string(
            &serde_json::to_value(&commit.package_metadata).map_err(|error| {
                AppError::new(
                    "MCL_ARTIFACT_METADATA_INVALID",
                    error.to_string(),
                    false,
                    "Report this deterministic artifact serialization defect.",
                )
            })?,
        )?;
        let repaired_payload_json = canonical_string(&commit.repaired_claim.payload)?;
        let repair_edge_payload_json = canonical_string(
            &serde_json::to_value(&commit.repair_edge_payload).map_err(|error| {
                AppError::new(
                    "MCL_COUNTEREXAMPLE_SERIALIZATION_FAILED",
                    error.to_string(),
                    false,
                    "Report this closed repair-edge serialization defect.",
                )
            })?,
        )?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start atomic counterexample repair", error))?;
        if let Some(existing) = read_idempotent_result(
            &transaction,
            COUNTEREXAMPLE_REPAIR_OPERATION,
            idempotency_key,
            &input_hash,
        )? {
            return Ok(existing);
        }

        let current = capture_claim_status_read_basis(&transaction, &commit.original_claim)?;
        if current != commit.claim_status_basis {
            return Err(counterexample_repair_conflict(
                "claim truth inputs changed before the atomic repair commit",
            ));
        }
        recheck_counterexample_search_run(
            &transaction,
            &commit.counterexample_search_run_id,
            &commit.counterexample_search_run_head_hash,
        )?;
        validate_record_references(&transaction, &commit.repaired_claim)?;
        reject_duplicate_record_version(&transaction, &commit.repaired_claim_version_hash)?;
        if artifact_exists(&transaction, &commit.package_artifact_hash)? {
            return Err(AppError::new(
                "MCL_ARTIFACT_EXISTS",
                format!(
                    "counterexample package artifact {} is already registered without this idempotency receipt",
                    commit.package_artifact_hash
                ),
                false,
                "Retrieve the existing repair or retry with the original idempotency key.",
            ));
        }

        transaction
            .execute(
                "INSERT INTO artifacts(artifact_hash, media_type, byte_size, creation_source, license_expression, restriction, metadata_json, created_at, created_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, unixepoch(), ?8)",
                params![
                    commit.package_artifact_hash,
                    commit.package_metadata.media_type.as_str(),
                    commit.package_byte_size as i64,
                    commit.package_metadata.creation_source.as_str(),
                    commit.package_metadata.license_expression,
                    commit.package_metadata.restriction.as_str(),
                    package_metadata_json,
                    actor,
                ],
            )
            .map_err(|error| AppError::database("insert counterexample package artifact", error))?;
        #[cfg(test)]
        if fail_after_artifact {
            return Err(AppError::new(
                "MCL_COUNTEREXAMPLE_REPAIR_INJECTED_FAILURE",
                "injected failure after counterexample package registration",
                true,
                "Retry the exact idempotent repair after the transient test fault clears.",
            ));
        }

        let repaired_claim_object_id = Uuid::now_v7().to_string();
        transaction
            .execute(
                "INSERT INTO records(object_id, record_type, head_version_hash, created_at, created_by) VALUES (?1, 'claim', NULL, unixepoch(), ?2)",
                params![repaired_claim_object_id, actor],
            )
            .map_err(|error| AppError::database("insert repaired claim object", error))?;
        transaction
            .execute(
                "INSERT INTO record_versions(version_hash, object_id, schema_version, payload_json, predecessor_hash, created_at, created_by) VALUES (?1, ?2, ?3, ?4, NULL, unixepoch(), ?5)",
                params![
                    commit.repaired_claim_version_hash,
                    repaired_claim_object_id,
                    commit.repaired_claim.schema_version,
                    repaired_payload_json,
                    actor,
                ],
            )
            .map_err(|error| AppError::database("insert repaired claim version", error))?;
        transaction
            .execute(
                "UPDATE records SET head_version_hash = ?1 WHERE object_id = ?2 AND head_version_hash IS NULL",
                params![commit.repaired_claim_version_hash, repaired_claim_object_id],
            )
            .map_err(|error| AppError::database("set repaired claim head", error))?;
        update_search_projection(
            &transaction,
            &repaired_claim_object_id,
            &commit.repaired_claim,
        )?;

        let repair_edge_id = Uuid::now_v7().to_string();
        transaction
            .execute(
                "INSERT INTO edges(edge_id, edge_type, source_object_id, source_version_hash, target_object_id, target_version_hash, payload_json, created_at, created_by) VALUES (?1, 'research.repairs', ?2, ?3, ?4, ?5, ?6, unixepoch(), ?7)",
                params![
                    repair_edge_id,
                    repaired_claim_object_id,
                    commit.repaired_claim_version_hash,
                    commit.original_claim.object_id,
                    commit.original_claim.version_hash,
                    repair_edge_payload_json,
                    actor,
                ],
            )
            .map_err(|error| AppError::database("insert controlled claim repair edge", error))?;

        let snapshot = CounterexampleRepairSnapshot {
            package_artifact: read_artifact(&transaction, &commit.package_artifact_hash)?,
            repaired_claim: read_snapshot(
                &transaction,
                &repaired_claim_object_id,
                Some(&commit.repaired_claim_version_hash),
            )?,
            repair_edge: read_edge(&transaction, &repair_edge_id)?,
        };
        write_idempotent_result(
            &transaction,
            COUNTEREXAMPLE_REPAIR_OPERATION,
            idempotency_key,
            &input_hash,
            &snapshot,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit atomic counterexample repair", error))?;
        Ok(snapshot)
    }

    pub fn get_edge(&self, edge_id: &str) -> Result<EdgeSnapshot, AppError> {
        read_edge(&self.connection, edge_id)
    }
}

fn validate_counterexample_repair_commit_shape(
    commit: &CounterexampleRepairCommit,
) -> Result<(), AppError> {
    validate_hash(
        &commit.package_artifact_hash,
        "counterexample package artifact",
    )?;
    commit.package_metadata.validate(commit.package_byte_size)?;
    if commit.package_metadata.media_type != ArtifactMediaType::Json
        || commit.package_metadata.creation_source != ArtifactCreationSource::Generated
        || commit.package_metadata.license_expression.is_some()
        || commit.package_metadata.restriction != ArtifactRestriction::Private
    {
        return Err(counterexample_repair_integrity_error(
            "counterexample package metadata is not controlled generated private canonical JSON",
        ));
    }
    let expected_metadata = BTreeMap::from([
        (
            "artifact_role".to_owned(),
            "counterexample_package".to_owned(),
        ),
        (
            "schema_version".to_owned(),
            COUNTEREXAMPLE_PACKAGE_SCHEMA_VERSION.to_owned(),
        ),
        (
            "package_hash".to_owned(),
            commit.package_artifact_hash.clone(),
        ),
        (
            "original_claim_object_id".to_owned(),
            commit.original_claim.object_id.clone(),
        ),
        (
            "original_claim_version_hash".to_owned(),
            commit.original_claim.version_hash.clone(),
        ),
        (
            "refutation_formalization_object_id".to_owned(),
            commit
                .repair_edge_payload
                .refutation_formalization
                .object_id
                .clone(),
        ),
        (
            "refutation_formalization_version_hash".to_owned(),
            commit
                .repair_edge_payload
                .refutation_formalization
                .version_hash
                .clone(),
        ),
        (
            "counterexample_search_run_id".to_owned(),
            commit.counterexample_search_run_id.clone(),
        ),
        (
            "counterexample_search_run_head_hash".to_owned(),
            commit.counterexample_search_run_head_hash.clone(),
        ),
    ]);
    if commit.package_metadata.semantic_metadata != expected_metadata {
        return Err(counterexample_repair_integrity_error(
            "counterexample package metadata does not bind the exact package, claim, refutation, and search run",
        ));
    }
    if commit.repaired_claim.kind != RecordKind::Claim
        || commit.repaired_claim.schema_version != crate::domain::schemas::CLAIM_SCHEMA_VERSION
    {
        return Err(counterexample_repair_integrity_error(
            "counterexample repair must create one new claim/1 object",
        ));
    }
    validate_record_payload(
        commit.repaired_claim.kind,
        &commit.repaired_claim.schema_version,
        &commit.repaired_claim.payload,
    )?;
    let reproduced = record_version_hash(
        &commit.repaired_claim.schema_version,
        &commit.repaired_claim.payload,
    )?;
    if reproduced != commit.repaired_claim_version_hash
        || reproduced == commit.original_claim.version_hash
    {
        return Err(counterexample_repair_integrity_error(
            "repaired claim payload does not reproduce a distinct proposed version hash",
        ));
    }
    let repaired_payload: ClaimPayload =
        serde_json::from_value(commit.repaired_claim.payload.clone()).map_err(|error| {
            counterexample_repair_integrity_error(format!(
                "repaired claim payload cannot be decoded: {error}"
            ))
        })?;
    if commit.claim_status_basis.claim != commit.original_claim
        || commit
            .claim_status_basis
            .current_claim_head_version_hash
            .as_deref()
            != Some(commit.original_claim.version_hash.as_str())
        || commit
            .claim_status_basis
            .current_source_head_version_hash
            .as_deref()
            != Some(commit.claim_status_basis.source.version_hash.as_str())
        || repaired_payload.source_reference != commit.claim_status_basis.source
    {
        return Err(counterexample_repair_integrity_error(
            "repair does not preserve the exact current original claim and source lineage",
        ));
    }
    commit.repair_edge_payload.validate()?;
    if commit.repair_edge_payload.schema_version != CLAIM_REPAIR_EDGE_SCHEMA_VERSION
        || commit
            .repair_edge_payload
            .counterexample_package_artifact_hash
            != commit.package_artifact_hash
        || commit.repair_edge_payload.counterexample_search_run_id
            != commit.counterexample_search_run_id
        || commit
            .repair_edge_payload
            .counterexample_search_run_head_hash
            != commit.counterexample_search_run_head_hash
        || !commit
            .claim_status_basis
            .formalizations
            .iter()
            .any(|entry| entry.formalization == commit.repair_edge_payload.refutation_formalization)
    {
        return Err(counterexample_repair_integrity_error(
            "repair edge does not bind the package, selected refutation, and exact search run",
        ));
    }
    Ok(())
}

fn recheck_counterexample_search_run(
    connection: &Connection,
    run_id: &str,
    expected_head_hash: &str,
) -> Result<(), AppError> {
    let row = connection
        .query_row(
            "SELECT run_kind, state, event_count, event_head_hash FROM runs WHERE run_id = ?1",
            [run_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|error| AppError::database("read counterexample search run", error))?;
    let Some((kind, state, event_count, head_hash)) = row else {
        return Err(AppError::new(
            "MCL_COUNTEREXAMPLE_SEARCH_RUN_INVALID",
            format!("counterexample search run {run_id} does not exist"),
            false,
            "Start and reference one exact counterexample_search run.",
        ));
    };
    if kind != "counterexample_search"
        || state == "failed"
        || !matches!(state.as_str(), "active" | "frozen" | "closed")
        || event_count < 1
        || head_hash.as_deref() != Some(expected_head_hash)
    {
        return Err(counterexample_repair_conflict(
            "counterexample search run kind, state, chain, or head changed",
        ));
    }
    Ok(())
}

fn reject_duplicate_record_version(
    connection: &Connection,
    version_hash: &str,
) -> Result<(), AppError> {
    if let Some(object_id) = connection
        .query_row(
            "SELECT object_id FROM record_versions WHERE version_hash = ?1",
            [version_hash],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| AppError::database("search duplicate repaired claim", error))?
    {
        return Err(AppError::new(
            "MCL_RECORD_VERSION_EXISTS",
            format!("identical repaired claim content already belongs to object {object_id}"),
            false,
            "Retrieve the existing claim instead of creating a duplicate repair.",
        ));
    }
    Ok(())
}

fn artifact_exists(connection: &Connection, artifact_hash: &str) -> Result<bool, AppError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM artifacts WHERE artifact_hash = ?1)",
            [artifact_hash],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| AppError::database("search counterexample package artifact", error))
}

fn reject_controlled_repair_edge(kind: EdgeKind) -> Result<(), AppError> {
    if kind == EdgeKind::ResearchRepairs {
        return Err(AppError::new(
            "MCL_CLAIM_REPAIR_EDGE_CONTROLLED",
            "research.repairs edges may only be created by the atomic counterexample repair path",
            false,
            "Use the shared counterexample repair application capability.",
        ));
    }
    Ok(())
}

fn counterexample_repair_conflict(message: impl Into<String>) -> AppError {
    AppError::new(
        "MCL_COUNTEREXAMPLE_REPAIR_CONFLICT",
        message,
        true,
        "Reload the claim status and counterexample search run, then retry against their exact current heads.",
    )
}

fn counterexample_repair_integrity_error(message: impl Into<String>) -> AppError {
    AppError::new(
        "MCL_COUNTEREXAMPLE_REPAIR_INTEGRITY_FAILED",
        message,
        false,
        "Quarantine the repair attempt and rebuild it through the controlled application path.",
    )
}

fn read_current_formalizations_for_claim_bounded(
    connection: &Connection,
    claim: &ExactVersionReference,
    limit: usize,
) -> Result<Vec<RecordSnapshot>, AppError> {
    validate_claim_status_record_reference(connection, claim, RecordKind::Claim, "claim")?;
    validate_claim_status_limit(limit, MAX_CLAIM_STATUS_FORMALIZATIONS, "formalization")?;
    let query_limit = i64::try_from(limit + 1).expect("claim-status limit fits in i64");
    let mut statement = connection
        .prepare(
            "SELECT record.object_id, record.head_version_hash FROM records AS record JOIN record_versions AS version ON version.object_id = record.object_id AND version.version_hash = record.head_version_hash WHERE record.record_type = 'formalization' AND record.tombstoned = 0 AND json_extract(version.payload_json, '$.claim_version.object_id') = ?1 AND json_extract(version.payload_json, '$.claim_version.version_hash') = ?2 ORDER BY record.object_id, record.head_version_hash LIMIT ?3",
        )
        .map_err(|error| AppError::database("prepare current claim formalization list", error))?;
    let references = statement
        .query_map(
            params![claim.object_id, claim.version_hash, query_limit],
            |row| {
                Ok(ExactVersionReference {
                    object_id: row.get(0)?,
                    version_hash: row.get(1)?,
                })
            },
        )
        .map_err(|error| AppError::database("list current claim formalizations", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::database("read current claim formalization list", error))?;
    reject_claim_status_overflow(references.len(), limit, "current formalizations")?;

    references
        .iter()
        .map(|reference| {
            let snapshot = validate_claim_status_record_reference(
                connection,
                reference,
                RecordKind::Formalization,
                "current formalization",
            )?;
            let payload: FormalizationPayload = serde_json::from_value(snapshot.payload.clone())
                .map_err(|error| {
                    claim_status_integrity_error(format!(
                        "current formalization {} cannot be decoded: {error}",
                        snapshot.version_hash
                    ))
                })?;
            if payload.claim_version != *claim {
                return Err(claim_status_integrity_error(format!(
                    "current formalization {} does not reference the requested exact claim",
                    snapshot.version_hash
                )));
            }
            Ok(snapshot)
        })
        .collect()
}

fn read_authoritative_evidence_for_subject_bounded(
    connection: &Connection,
    subject: &ExactVersionReference,
    limit: usize,
) -> Result<Vec<EvidenceSnapshot>, AppError> {
    validate_claim_status_record_reference(
        connection,
        subject,
        RecordKind::Formalization,
        "formalization",
    )?;
    validate_claim_status_limit(
        limit,
        MAX_CLAIM_STATUS_EVIDENCE_PER_FORMALIZATION,
        "authoritative evidence",
    )?;
    list_claim_status_evidence_bounded(
        connection,
        subject,
        ClaimStatusEvidenceSelection::Authoritative,
        limit,
    )
}

fn capture_claim_status_read_basis(
    connection: &Connection,
    claim: &ExactVersionReference,
) -> Result<ClaimStatusReadBasis, AppError> {
    let claim_snapshot =
        validate_claim_status_record_reference(connection, claim, RecordKind::Claim, "claim")?;
    let claim_payload: ClaimPayload =
        serde_json::from_value(claim_snapshot.payload).map_err(|error| {
            claim_status_integrity_error(format!(
                "stored claim payload cannot resolve its exact source: {error}"
            ))
        })?;
    validate_claim_status_record_reference(
        connection,
        &claim_payload.source_reference,
        RecordKind::Source,
        "source",
    )?;
    let current_claim_head_version_hash = read_current_claim_status_head(connection, claim)?;
    let current_source_head_version_hash = read_current_claim_status_record_head(
        connection,
        &claim_payload.source_reference,
        RecordKind::Source,
        "source",
    )?;
    let formalizations =
        if current_claim_head_version_hash.as_deref() == Some(claim.version_hash.as_str()) {
            read_current_formalizations_for_claim_bounded(
                connection,
                claim,
                MAX_CLAIM_STATUS_FORMALIZATIONS,
            )?
            .into_iter()
            .map(|snapshot| {
                let formalization = ExactVersionReference {
                    object_id: snapshot.object_id,
                    version_hash: snapshot.version_hash,
                };
                let fidelity_evidence = list_claim_status_evidence_bounded(
                    connection,
                    &formalization,
                    ClaimStatusEvidenceSelection::Fidelity,
                    MAX_CLAIM_STATUS_EVIDENCE_PER_FORMALIZATION,
                )?
                .into_iter()
                .map(claim_status_evidence_basis)
                .collect();
                let authoritative_evidence = read_authoritative_evidence_for_subject_bounded(
                    connection,
                    &formalization,
                    MAX_CLAIM_STATUS_EVIDENCE_PER_FORMALIZATION,
                )?
                .into_iter()
                .map(claim_status_evidence_basis)
                .collect();
                Ok(ClaimStatusFormalizationReadBasis {
                    formalization,
                    fidelity_evidence,
                    authoritative_evidence,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?
        } else {
            Vec::new()
        };
    Ok(ClaimStatusReadBasis {
        claim: claim.clone(),
        source: claim_payload.source_reference,
        current_claim_head_version_hash,
        current_source_head_version_hash,
        formalizations,
    })
}

fn validate_claim_status_record_reference(
    connection: &Connection,
    reference: &ExactVersionReference,
    expected_kind: RecordKind,
    label: &str,
) -> Result<RecordSnapshot, AppError> {
    validate_hash(&reference.version_hash, label)?;
    let snapshot = read_snapshot_by_version(connection, &reference.version_hash)?;
    if snapshot.object_id != reference.object_id || snapshot.kind != expected_kind {
        return Err(AppError::new(
            "MCL_CLAIM_STATUS_SUBJECT_INVALID",
            format!("{label} is not the requested exact {expected_kind} object and version"),
            false,
            format!("Use one exact canonical {expected_kind} reference."),
        ));
    }
    validate_record_payload(snapshot.kind, &snapshot.schema_version, &snapshot.payload).map_err(
        |error| {
            claim_status_integrity_error(format!(
                "stored {label} payload fails validation: {}",
                error.message
            ))
        },
    )?;
    let reproduced =
        record_version_hash(&snapshot.schema_version, &snapshot.payload).map_err(|error| {
            claim_status_integrity_error(format!(
                "stored {label} payload cannot reproduce a canonical identity: {}",
                error.message
            ))
        })?;
    if reproduced != snapshot.version_hash {
        return Err(claim_status_integrity_error(format!(
            "stored {label} payload does not reproduce version {}",
            snapshot.version_hash
        )));
    }
    Ok(snapshot)
}

fn read_current_claim_status_head(
    connection: &Connection,
    claim: &ExactVersionReference,
) -> Result<Option<String>, AppError> {
    read_current_claim_status_record_head(connection, claim, RecordKind::Claim, "claim")
}

fn read_current_claim_status_record_head(
    connection: &Connection,
    reference: &ExactVersionReference,
    expected_kind: RecordKind,
    label: &str,
) -> Result<Option<String>, AppError> {
    let row = connection
        .query_row(
            "SELECT record_type, head_version_hash, tombstoned FROM records WHERE object_id = ?1",
            [&reference.object_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| AppError::database("read claim status head", error))?
        .ok_or_else(|| {
            claim_status_integrity_error(format!(
                "exact {label} version has no owning canonical record"
            ))
        })?;
    let (kind, head, tombstoned) = row;
    if kind != expected_kind.as_str() || !matches!(tombstoned, 0 | 1) {
        return Err(claim_status_integrity_error(format!(
            "{label} record type or tombstone projection is invalid"
        )));
    }
    let head = head.ok_or_else(|| {
        claim_status_integrity_error(format!("{label} record has no current version head"))
    })?;
    validate_claim_status_record_reference(
        connection,
        &ExactVersionReference {
            object_id: reference.object_id.clone(),
            version_hash: head.clone(),
        },
        expected_kind,
        &format!("current {label} head"),
    )?;
    Ok((tombstoned == 0).then_some(head))
}

fn list_claim_status_evidence_bounded(
    connection: &Connection,
    subject: &ExactVersionReference,
    selection: ClaimStatusEvidenceSelection,
    limit: usize,
) -> Result<Vec<EvidenceSnapshot>, AppError> {
    validate_claim_status_limit(
        limit,
        MAX_CLAIM_STATUS_EVIDENCE_PER_FORMALIZATION,
        "evidence",
    )?;
    let query_limit = i64::try_from(limit + 1).expect("claim-status limit fits in i64");
    let sql = match selection {
        ClaimStatusEvidenceSelection::Fidelity => {
            "SELECT evidence.evidence_id FROM evidence WHERE evidence.subject_object_id = ?1 AND evidence.subject_version_hash = ?2 AND (evidence.evidence_kind = 'statement_fidelity_review' OR json_extract(evidence.metadata_json, '$.evidence_kind') = 'statement_fidelity_review' OR EXISTS (SELECT 1 FROM json_each(evidence.artifact_hashes_json) AS member JOIN artifacts ON artifacts.artifact_hash = member.value WHERE json_extract(artifacts.metadata_json, '$.semantic_metadata.artifact_role') = 'fidelity_review_report')) ORDER BY evidence.evidence_hash, evidence.evidence_id LIMIT ?3"
        }
        ClaimStatusEvidenceSelection::Authoritative => {
            "SELECT evidence_id FROM evidence WHERE subject_object_id = ?1 AND subject_version_hash = ?2 AND (authority_class = 'authoritative' OR evidence_kind IN ('lean_kernel_proof', 'lean_kernel_refutation') OR publication_receipt_hash IS NOT NULL OR publication_stage_hash IS NOT NULL OR json_extract(metadata_json, '$.schema_version') = 'evidence/2' OR json_type(metadata_json, '$.publication_authority') IS NOT NULL) ORDER BY evidence_hash, evidence_id LIMIT ?3"
        }
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| AppError::database("prepare claim status evidence list", error))?;
    let evidence_ids = statement
        .query_map(
            params![subject.object_id, subject.version_hash, query_limit],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| AppError::database("list claim status evidence", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::database("read claim status evidence list", error))?;
    reject_claim_status_overflow(evidence_ids.len(), limit, "evidence records")?;
    evidence_ids
        .iter()
        .map(|evidence_id| read_evidence(connection, evidence_id))
        .collect()
}

fn claim_status_evidence_basis(evidence: EvidenceSnapshot) -> ClaimStatusEvidenceReadBasis {
    ClaimStatusEvidenceReadBasis {
        evidence_id: evidence.evidence_id,
        evidence_hash: evidence.evidence_hash,
    }
}

fn validate_claim_status_limit(limit: usize, maximum: usize, label: &str) -> Result<(), AppError> {
    if !(1..=maximum).contains(&limit) {
        return Err(AppError::new(
            "MCL_CLAIM_STATUS_LIMIT_INVALID",
            format!("claim status {label} limit must be between 1 and {maximum}"),
            false,
            "Use the fixed bounded derived-status read path.",
        ));
    }
    Ok(())
}

fn reject_claim_status_overflow(
    observed: usize,
    limit: usize,
    label: &str,
) -> Result<(), AppError> {
    if observed > limit {
        return Err(AppError::new(
            "MCL_CLAIM_STATUS_LIMIT_EXCEEDED",
            format!("claim status has more than {limit} {label}"),
            false,
            "Refine or explicitly retire excess canonical inputs; derived status never truncates them.",
        ));
    }
    Ok(())
}

fn claim_status_integrity_error(message: impl Into<String>) -> AppError {
    AppError::new(
        "MCL_CLAIM_STATUS_INTEGRITY_FAILED",
        message,
        false,
        "Quarantine the affected claim lineage and restore a verified database and artifact backup.",
    )
}

fn validate_record_references(
    connection: &Connection,
    draft: &RecordDraft,
) -> Result<(), AppError> {
    match draft.kind {
        RecordKind::Formalization => validate_formalization_record_references(connection, draft),
        RecordKind::LearningUnit => validate_learning_unit_store_references(connection, draft),
        RecordKind::Concept => validate_concept_record_references(connection, draft),
        RecordKind::Source | RecordKind::Claim => Ok(()),
    }
}

fn validate_concept_record_references(
    connection: &Connection,
    draft: &RecordDraft,
) -> Result<(), AppError> {
    let concept: ConceptPayload =
        serde_json::from_value(draft.payload.clone()).map_err(|error| {
            AppError::new(
                "MCL_SCHEMA_VALIDATION_FAILED",
                format!("concept payload could not be decoded after validation: {error}"),
                false,
                "Submit a payload matching the committed concept schema.",
            )
        })?;
    for crosswalk in concept.external_taxonomy_crosswalks {
        validate_current_learning_reference(
            connection,
            &crosswalk.source_reference,
            RecordKind::Source,
            "external taxonomy source",
        )?;
        let source_json = connection
            .query_row(
                "SELECT payload_json FROM record_versions WHERE object_id = ?1 AND version_hash = ?2",
                params![
                    crosswalk.source_reference.object_id,
                    crosswalk.source_reference.version_hash
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| AppError::database("read external taxonomy source", error))?;
        let source: SourcePayload = serde_json::from_str(&source_json).map_err(|error| {
            AppError::new(
                "MCL_TAXONOMY_SOURCE_INVALID",
                format!("external taxonomy source payload is invalid: {error}"),
                false,
                "Quarantine the source and restore a schema-valid canonical version.",
            )
        })?;
        if source.license_expression.as_deref() != Some(crosswalk.license_expression.as_str()) {
            return Err(AppError::new(
                "MCL_TAXONOMY_LICENSE_MISMATCH",
                "external taxonomy crosswalk license does not match its exact source",
                false,
                "Record the exact reviewed source license on the crosswalk.",
            ));
        }
    }
    Ok(())
}

fn validate_formalization_record_references(
    connection: &Connection,
    draft: &RecordDraft,
) -> Result<(), AppError> {
    let formalization: FormalizationPayload = serde_json::from_value(draft.payload.clone())
        .map_err(|error| {
            AppError::new(
                "MCL_SCHEMA_VALIDATION_FAILED",
                format!("formalization payload could not be decoded after validation: {error}"),
                false,
                "Submit a payload matching the committed formalization schema.",
            )
        })?;
    let actual_kind = connection
        .query_row(
            "SELECT r.record_type FROM record_versions rv JOIN records r ON r.object_id = rv.object_id WHERE rv.version_hash = ?1 AND rv.object_id = ?2",
            params![
                formalization.claim_version.version_hash,
                formalization.claim_version.object_id
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| AppError::database("validate formalization claim version", error))?;
    match actual_kind.as_deref() {
        Some("claim") => {}
        Some(kind) => {
            return Err(AppError::new(
                "MCL_FORMALIZATION_CLAIM_INVALID",
                format!("formalization claim reference resolves to `{kind}`, not `claim`"),
                false,
                "Reference an exact existing claim object and version.",
            ));
        }
        None => {
            return Err(AppError::new(
                "MCL_FORMALIZATION_CLAIM_INVALID",
                "formalization claim reference does not resolve to an exact existing version",
                false,
                "Reference an exact existing claim object and version.",
            ));
        }
    }
    let environment_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM environments WHERE environment_hash = ?1)",
            [&formalization.environment_hash],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| AppError::database("validate formalization environment", error))?;
    if !environment_exists {
        return Err(AppError::new(
            "MCL_FORMALIZATION_ENVIRONMENT_INVALID",
            format!(
                "formalization environment {} is not registered",
                formalization.environment_hash
            ),
            false,
            "Register and select an exact pinned environment before creating the formalization.",
        ));
    }
    let artifact_media_type = connection
        .query_row(
            "SELECT media_type FROM artifacts WHERE artifact_hash = ?1",
            [&formalization.module_artifact_hash],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| AppError::database("validate formalization module artifact", error))?;
    match artifact_media_type.as_deref() {
        Some("text/x-lean") => {}
        Some(media_type) => {
            return Err(AppError::new(
                "MCL_FORMALIZATION_ARTIFACT_INVALID",
                format!(
                    "formalization module artifact {} has media type `{media_type}`, not `text/x-lean`",
                    formalization.module_artifact_hash
                ),
                false,
                "Attach an exact registered Lean source artifact before creating the formalization.",
            ));
        }
        None => {
            return Err(AppError::new(
                "MCL_FORMALIZATION_ARTIFACT_INVALID",
                format!(
                    "formalization module artifact {} is not registered",
                    formalization.module_artifact_hash
                ),
                false,
                "Ingest and register the exact Lean source artifact before creating the formalization.",
            ));
        }
    }
    Ok(())
}

fn validate_learning_unit_store_references(
    connection: &Connection,
    draft: &RecordDraft,
) -> Result<(), AppError> {
    let learning_unit: LearningUnitPayload = serde_json::from_value(draft.payload.clone())
        .map_err(|error| {
            AppError::new(
                "MCL_SCHEMA_VALIDATION_FAILED",
                format!("learning-unit payload could not be decoded after validation: {error}"),
                false,
                "Submit a payload matching the committed learning-unit schema.",
            )
        })?;
    let target_kind = match learning_unit.target.kind {
        LearningTargetKind::Claim => RecordKind::Claim,
        LearningTargetKind::Concept => RecordKind::Concept,
    };
    validate_current_learning_reference(
        connection,
        &ExactVersionReference {
            object_id: learning_unit.target.object_id,
            version_hash: learning_unit.target.version_hash,
        },
        target_kind,
        "learning unit target",
    )?;
    for reference in &learning_unit.grounded_source_references {
        validate_current_learning_reference(
            connection,
            reference,
            RecordKind::Source,
            "learning unit grounded source",
        )?;
    }
    for reference in learning_unit
        .hard_prerequisites
        .iter()
        .chain(&learning_unit.soft_prerequisites)
        .chain(&learning_unit.examples)
        .chain(&learning_unit.nonexamples)
        .chain(&learning_unit.counterexamples)
        .chain(&learning_unit.misconceptions)
        .chain(&learning_unit.exercises)
        .chain(&learning_unit.mastery_checks)
        .chain(&learning_unit.application_references)
        .chain(&learning_unit.frontier_references)
    {
        validate_current_learning_reference(
            connection,
            reference,
            RecordKind::LearningUnit,
            "learning unit relation",
        )?;
    }
    for reference in &learning_unit.formalization_references {
        validate_current_learning_reference(
            connection,
            reference,
            RecordKind::Formalization,
            "learning unit formalization",
        )?;
    }
    let artifact_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM artifacts WHERE artifact_hash = ?1)",
            [&learning_unit.content_artifact_hash],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| AppError::database("validate learning-unit content artifact", error))?;
    if !artifact_exists {
        return Err(AppError::new(
            "MCL_PEDAGOGY_CONTENT_ARTIFACT_INVALID",
            "learning-unit content artifact is not registered",
            false,
            "Ingest and verify the exact content artifact before creating the unit.",
        ));
    }
    Ok(())
}

fn validate_current_learning_reference(
    connection: &Connection,
    reference: &ExactVersionReference,
    expected_kind: RecordKind,
    label: &str,
) -> Result<(), AppError> {
    let resolved = connection
        .query_row(
            "SELECT record.record_type, record.head_version_hash FROM records AS record JOIN record_versions AS version ON version.object_id = record.object_id WHERE record.object_id = ?1 AND version.version_hash = ?2 AND record.tombstoned = 0",
            params![reference.object_id, reference.version_hash],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| AppError::database("validate current learning-unit reference", error))?;
    let Some((actual_kind, current_head)) = resolved else {
        return Err(AppError::new(
            "MCL_PEDAGOGY_REFERENCE_INVALID",
            format!(
                "{label} {}@{} does not resolve to an exact canonical record",
                reference.object_id, reference.version_hash
            ),
            false,
            "Use an exact current object and version returned by canonical lookup.",
        ));
    };
    if actual_kind != expected_kind.as_str() {
        return Err(AppError::new(
            "MCL_PEDAGOGY_REFERENCE_KIND_INVALID",
            format!("{label} resolves to `{actual_kind}`, not `{expected_kind}`"),
            false,
            "Use an exact canonical reference of the required kind.",
        ));
    }
    if current_head != reference.version_hash {
        return Err(AppError::new(
            "MCL_PEDAGOGY_REFERENCE_STALE",
            format!(
                "{label} {}@{} is not current; head is {current_head}",
                reference.object_id, reference.version_hash
            ),
            false,
            "Rebase the learning unit on current exact canonical references.",
        ));
    }
    Ok(())
}

fn validate_mutation_inputs(actor: &str, idempotency_key: &str) -> Result<(), AppError> {
    if actor.trim().is_empty() || idempotency_key.trim().is_empty() {
        return Err(AppError::new(
            "MCL_ATTRIBUTION_REQUIRED",
            "actor and idempotency key must be nonempty",
            false,
            "Supply stable actor attribution and an idempotency key.",
        ));
    }
    Ok(())
}

fn validate_hash(hash: &str, label: &str) -> Result<(), AppError> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AppError::new(
            "MCL_HASH_INVALID",
            format!("{label} must be a lowercase hexadecimal SHA-256 identity"),
            false,
            "Use the exact hash returned by the canonical store.",
        ));
    }
    Ok(())
}

fn validate_publication_actor(actor: &str) -> Result<(), AppError> {
    if actor.chars().count() > 256 {
        return Err(AppError::new(
            "MCL_ATTRIBUTION_INVALID",
            "publication actor attribution exceeds 256 characters",
            false,
            "Use a short stable actor identity.",
        ));
    }
    Ok(())
}

fn publication_ingestion_input_hash(stage_hash: &str, actor: &str) -> Result<String, AppError> {
    value_hash(&json!({
        "operation": PUBLICATION_INGESTION_OPERATION,
        "stage_hash": stage_hash,
        "actor": actor,
    }))
}

fn validate_current_publication_subject(
    connection: &Connection,
    subject: &ExactVersionReference,
) -> Result<(), AppError> {
    let current = connection
        .query_row(
            "SELECT record_type, head_version_hash FROM records WHERE object_id = ?1 AND tombstoned = 0",
            [&subject.object_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(|error| AppError::database("validate publication subject head", error))?;
    if current.as_ref().is_none_or(|(kind, head)| {
        kind != "formalization" || head.as_deref() != Some(subject.version_hash.as_str())
    }) {
        return Err(AppError::new(
            "MCL_PUBLICATION_SUBJECT_STALE",
            "publication subject is no longer the current canonical formalization version",
            false,
            "Select the current formalization head and reproduce its exact diagnostic and audit evidence.",
        ));
    }
    Ok(())
}

fn read_publication_receipt_subject_binding(
    connection: &Connection,
    receipt_hash: &str,
) -> Result<Option<ExactVersionReference>, AppError> {
    let binding = connection
        .query_row(
            "SELECT subject_object_id, subject_version_hash FROM publication_ingestion_receipts WHERE receipt_hash = ?1",
            [receipt_hash],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .optional()
        .map_err(|error| AppError::database("read publication receipt subject binding", error))?
        .ok_or_else(|| publication_receipt_not_found(receipt_hash))?;
    let (Some(object_id), Some(version_hash)) = binding else {
        if binding.0.is_none() && binding.1.is_none() {
            return Ok(None);
        }
        return Err(publication_receipt_integrity_error(format!(
            "stored publication receipt {receipt_hash} has a partial subject binding"
        )));
    };
    let exact_formalization_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM records AS record JOIN record_versions AS version ON version.object_id = record.object_id WHERE record.object_id = ?1 AND version.version_hash = ?2 AND record.record_type = 'formalization')",
            params![object_id, version_hash],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| AppError::database("validate publication receipt subject binding", error))?;
    if Uuid::parse_str(&object_id).is_err()
        || !is_lower_hex(&version_hash, 64)
        || !exact_formalization_exists
    {
        return Err(publication_receipt_integrity_error(format!(
            "stored publication receipt {receipt_hash} has an invalid exact formalization binding"
        )));
    }
    Ok(Some(ExactVersionReference {
        object_id,
        version_hash,
    }))
}

fn validate_publication_receipt_retry_subject(
    connection: &Connection,
    receipt_hash: &str,
    expected_subject: &ExactVersionReference,
) -> Result<(), AppError> {
    match read_publication_receipt_subject_binding(connection, receipt_hash)? {
        Some(bound_subject) if bound_subject == *expected_subject => Ok(()),
        Some(bound_subject) => Err(AppError::new(
            "MCL_PUBLICATION_RECEIPT_SUBJECT_MISMATCH",
            format!(
                "publication receipt {receipt_hash} is bound to {}@{}, not {}@{}",
                bound_subject.object_id,
                bound_subject.version_hash,
                expected_subject.object_id,
                expected_subject.version_hash
            ),
            false,
            "Use the exact publication-request formalization retained by this receipt.",
        )),
        None => Err(AppError::new(
            "MCL_PUBLICATION_RECEIPT_SUBJECT_UNBOUND",
            format!("publication receipt {receipt_hash} predates immutable formalization binding"),
            false,
            "Reingest a freshly staged protected candidate before granting authority.",
        )),
    }
}

fn validate_publication_authority_commit(
    connection: &Connection,
    commit: &PublicationAuthorityCommit,
    receipt: &PublicationIngestionReceiptSnapshot,
    stage: &PublicationStageSnapshot,
) -> Result<(), AppError> {
    commit.binding.validate()?;
    validate_hash(
        &commit.subject.version_hash,
        "publication authority subject version",
    )?;
    Uuid::parse_str(&commit.subject.object_id).map_err(|_| {
        publication_authority_error(
            "MCL_PUBLICATION_AUTHORITY_SUBJECT_INVALID",
            "publication authority subject object identity is not a UUID",
            "Use the exact application-replayed publication subject.",
        )
    })?;
    validate_hash(
        &commit.environment_hash,
        "publication authority environment",
    )?;
    let receipt_subject = read_publication_receipt_subject_binding(
        connection,
        &commit.binding.ingestion_receipt_hash,
    )?;
    let retained_request_subject = publication_request_subject_binding(connection, stage)?;
    if receipt_subject.as_ref() != Some(&commit.subject)
        || retained_request_subject != commit.subject
    {
        return Err(publication_authority_error(
            "MCL_PUBLICATION_AUTHORITY_SUBJECT_MISMATCH",
            "publication authority subject does not match the exact formalization retained by the ingestion receipt",
            "Replay only the publication-request subject immutably bound during controlled ingestion.",
        ));
    }

    if receipt.receipt_hash != commit.binding.ingestion_receipt_hash
        || receipt.stage_hash != commit.binding.stage_hash
        || stage.stage_hash != commit.binding.stage_hash
        || stage.stage.report_artifact_hash != commit.binding.report_artifact_hash
        || stage.stage.retained_closure_artifact_hash
            != commit.binding.retained_closure_artifact_hash
        || stage.stage.attestation_bundle_artifact_hash
            != commit.binding.attestation_bundle_artifact_hash
        || receipt.verification.report_artifact_hash != commit.binding.report_artifact_hash
        || receipt.verification.report_content_hash != commit.binding.report_artifact_hash
        || receipt.verification.attestation_bundle_hash
            != commit.binding.attestation_bundle_artifact_hash
        || receipt.verification.raw_verification_hash != commit.binding.raw_verification_hash
        || receipt.verification.authoritative
        || stage.stage.authoritative
    {
        return Err(publication_authority_error(
            "MCL_PUBLICATION_AUTHORITY_BINDING_INVALID",
            "publication authority commit does not reproduce the exact immutable receipt and stage projections",
            "Replay the exact receipt and complete staged publication closure through the application service.",
        ));
    }

    let request_hash = publication_stage_role_artifact_hash(
        stage,
        PublicationRetainedArtifactRole::PublicationRequest,
    )?;
    let policy_hash = publication_stage_role_artifact_hash(
        stage,
        PublicationRetainedArtifactRole::PublicationPolicy,
    )?;
    if request_hash != commit.binding.publication_request_hash
        || policy_hash != commit.binding.publication_policy_hash
    {
        return Err(publication_authority_error(
            "MCL_PUBLICATION_AUTHORITY_BINDING_INVALID",
            "publication authority request or policy hash does not match the immutable stage",
            "Replay the request and committed policy retained by the exact publication stage.",
        ));
    }

    let expected_artifact_hashes = publication_authority_artifact_hashes(stage, receipt);
    if commit.artifact_hashes != expected_artifact_hashes {
        return Err(publication_authority_error(
            "MCL_PUBLICATION_AUTHORITY_ARTIFACTS_INVALID",
            "publication authority artifact set is not the complete exact staged and verified CAS closure",
            "Use the application-derived sorted union of the stage, report, bundle, raw verification, and receipt hashes.",
        ));
    }

    validate_current_publication_subject(connection, &commit.subject)?;
    let (schema_version, payload_json) = connection
        .query_row(
            "SELECT schema_version, payload_json FROM record_versions WHERE object_id = ?1 AND version_hash = ?2",
            params![commit.subject.object_id, commit.subject.version_hash],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|error| AppError::database("read publication authority subject", error))?;
    let payload_value: Value = serde_json::from_str(&payload_json).map_err(|error| {
        publication_authority_integrity_error(format!(
            "stored formalization payload is invalid: {error}"
        ))
    })?;
    validate_record_payload(RecordKind::Formalization, &schema_version, &payload_value).map_err(
        |error| {
            publication_authority_integrity_error(format!(
                "stored formalization fails validation: {}",
                error.message
            ))
        },
    )?;
    if record_version_hash(&schema_version, &payload_value)? != commit.subject.version_hash {
        return Err(publication_authority_integrity_error(
            "stored formalization does not reproduce its exact version identity",
        ));
    }
    let formalization: FormalizationPayload =
        serde_json::from_value(payload_value).map_err(|error| {
            publication_authority_integrity_error(format!(
                "stored formalization cannot be decoded: {error}"
            ))
        })?;
    let expected_outcome = match formalization.claim_polarity {
        Some(FormalizationClaimPolarity::Claim) => PublicationOutcome::Proof,
        Some(FormalizationClaimPolarity::Negation) => PublicationOutcome::Refutation,
        None => {
            return Err(publication_authority_error(
                "MCL_PUBLICATION_OUTCOME_UNBOUND",
                "publication authority subject has no typed claim polarity",
                "Create and republish an exact formalization version with typed claim polarity.",
            ));
        }
    };
    if formalization.environment_hash != commit.environment_hash {
        return Err(publication_authority_error(
            "MCL_PUBLICATION_AUTHORITY_ENVIRONMENT_MISMATCH",
            "publication authority environment does not match the exact formalization",
            "Replay the environment bound by the exact publication request and formalization.",
        ));
    }
    if commit.outcome != expected_outcome {
        return Err(publication_authority_error(
            "MCL_PUBLICATION_OUTCOME_MISMATCH",
            "publication authority outcome conflicts with the formalization's typed claim polarity",
            "Use only the outcome derived from the exact protected publication request.",
        ));
    }
    Ok(())
}

fn publication_stage_role_artifact_hash(
    stage: &PublicationStageSnapshot,
    role: PublicationRetainedArtifactRole,
) -> Result<&str, AppError> {
    stage
        .stage
        .retained_artifacts
        .iter()
        .find(|artifact| artifact.role == role)
        .map(|artifact| artifact.artifact_hash.as_str())
        .ok_or_else(|| {
            publication_authority_integrity_error(format!(
                "publication stage is missing retained role {}",
                role.as_str()
            ))
        })
}

fn publication_request_subject_binding(
    connection: &Connection,
    stage: &PublicationStageSnapshot,
) -> Result<ExactVersionReference, AppError> {
    let request_hash = stage
        .stage
        .retained_artifacts
        .iter()
        .find(|artifact| artifact.role == PublicationRetainedArtifactRole::PublicationRequest)
        .map(|artifact| artifact.artifact_hash.as_str())
        .ok_or_else(|| {
            AppError::new(
                "MCL_PUBLICATION_REQUEST_BINDING_INVALID",
                "publication stage is missing its retained publication request",
                false,
                "Restage the exact protected publication closure.",
            )
        })?;
    let artifact = read_artifact(connection, request_hash).map_err(|error| {
        AppError::new(
            "MCL_PUBLICATION_REQUEST_BINDING_INVALID",
            format!(
                "retained publication request metadata is unavailable: {}",
                error.message
            ),
            false,
            "Restore the exact controlled publication-request artifact registration.",
        )
    })?;
    let object_id = artifact
        .semantic_metadata
        .get("formalization_object_id")
        .cloned();
    let version_hash = artifact
        .semantic_metadata
        .get("formalization_version_hash")
        .cloned();
    let (Some(object_id), Some(version_hash)) = (object_id, version_hash) else {
        return Err(AppError::new(
            "MCL_PUBLICATION_REQUEST_BINDING_INVALID",
            "retained publication request metadata omits its exact formalization subject",
            false,
            "Restore the exact controlled publication-request artifact registration.",
        ));
    };
    let exact_formalization_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM records AS record JOIN record_versions AS version ON version.object_id = record.object_id WHERE record.object_id = ?1 AND version.version_hash = ?2 AND record.record_type = 'formalization')",
            params![object_id, version_hash],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| AppError::database("validate retained publication request subject", error))?;
    if artifact.media_type != ArtifactMediaType::Json
        || artifact.creation_source != ArtifactCreationSource::Generated
        || artifact.restriction != ArtifactRestriction::Private
        || artifact
            .semantic_metadata
            .get("artifact_role")
            .is_none_or(|role| role != "publication_request")
        || artifact
            .semantic_metadata
            .get("request_hash")
            .is_none_or(|hash| hash != request_hash)
        || Uuid::parse_str(&object_id).is_err()
        || !is_lower_hex(&version_hash, 64)
        || !exact_formalization_exists
    {
        return Err(AppError::new(
            "MCL_PUBLICATION_REQUEST_BINDING_INVALID",
            "retained publication request artifact metadata does not reproduce one exact formalization binding",
            false,
            "Restore the exact artifact registered through the controlled publication-request path.",
        ));
    }
    Ok(ExactVersionReference {
        object_id,
        version_hash,
    })
}

fn publication_authority_artifact_hashes(
    stage: &PublicationStageSnapshot,
    receipt: &PublicationIngestionReceiptSnapshot,
) -> Vec<String> {
    let mut hashes = stage
        .stage
        .retained_artifacts
        .iter()
        .map(|artifact| artifact.artifact_hash.clone())
        .collect::<Vec<_>>();
    hashes.extend([
        stage.stage.report_artifact_hash.clone(),
        stage.stage.retained_closure_artifact_hash.clone(),
        stage.stage.attestation_bundle_artifact_hash.clone(),
        receipt.verification.raw_verification_hash.clone(),
        receipt.receipt_hash.clone(),
    ]);
    hashes.sort_unstable();
    hashes.dedup();
    hashes
}

fn validate_stored_publication_authority_evidence(
    connection: &Connection,
    payload: &EvidencePayload,
    binding: &PublicationAuthorityBinding,
) -> Result<(), AppError> {
    let receipt = read_publication_ingestion_receipt(connection, &binding.ingestion_receipt_hash)?;
    let stage = read_publication_stage(connection, &binding.stage_hash)?;
    let receipt_subject =
        read_publication_receipt_subject_binding(connection, &binding.ingestion_receipt_hash)?;
    let retained_request_subject = publication_request_subject_binding(connection, &stage)?;
    let request_hash = publication_stage_role_artifact_hash(
        &stage,
        PublicationRetainedArtifactRole::PublicationRequest,
    )?;
    let policy_hash = publication_stage_role_artifact_hash(
        &stage,
        PublicationRetainedArtifactRole::PublicationPolicy,
    )?;
    if receipt_subject.as_ref() != Some(&payload.subject)
        || retained_request_subject != payload.subject
        || receipt.stage_hash != binding.stage_hash
        || stage.stage.report_artifact_hash != binding.report_artifact_hash
        || stage.stage.retained_closure_artifact_hash != binding.retained_closure_artifact_hash
        || stage.stage.attestation_bundle_artifact_hash != binding.attestation_bundle_artifact_hash
        || receipt.verification.report_artifact_hash != binding.report_artifact_hash
        || receipt.verification.report_content_hash != binding.report_artifact_hash
        || receipt.verification.attestation_bundle_hash != binding.attestation_bundle_artifact_hash
        || receipt.verification.raw_verification_hash != binding.raw_verification_hash
        || request_hash != binding.publication_request_hash
        || policy_hash != binding.publication_policy_hash
        || payload.artifact_hashes != publication_authority_artifact_hashes(&stage, &receipt)
    {
        return Err(publication_authority_integrity_error(
            "stored authoritative evidence does not reproduce its receipt-bound publication closure",
        ));
    }
    Ok(())
}

fn validate_publication_input_size(byte_size: u64, label: &str) -> Result<(), AppError> {
    if byte_size == 0 || byte_size > MAX_PUBLICATION_INPUT_BYTES {
        return Err(AppError::new(
            "MCL_PUBLICATION_INPUT_SIZE_INVALID",
            format!("{label} must contain between 1 and {MAX_PUBLICATION_INPUT_BYTES} bytes"),
            false,
            "Use the exact bounded retained publication input.",
        ));
    }
    Ok(())
}

fn validate_publication_attestation_shape(
    verification: &PublicationAttestationVerification,
) -> Result<(), AppError> {
    use crate::domain::publication::PUBLICATION_ATTESTATION_VERIFICATION_SCHEMA_VERSION;

    let policy = crate::domain::publication::committed_publication_policy()?;
    let expected_signer_workflow = format!("{}/{}", policy.repository, policy.workflow_path);
    let expected_certificate_identity = format!(
        "https://github.com/{}/{}@{}",
        policy.repository, policy.workflow_path, policy.required_source_ref
    );
    if verification.schema_version != PUBLICATION_ATTESTATION_VERIFICATION_SCHEMA_VERSION
        || !is_lower_hex(&verification.report_content_hash, 64)
        || verification.report_artifact_hash != verification.report_content_hash
        || !is_lower_hex(&verification.attestation_bundle_hash, 64)
        || !is_lower_hex(&verification.raw_verification_hash, 64)
        || verification.verifier_name != "gh"
        || verification.verifier_version != policy.attestation_verifier_version
        || verification.verifier_binary_sha256 != policy.attestation_verifier_binary_sha256
        || verification.repository != policy.repository
        || verification.signer_workflow != expected_signer_workflow
        || verification.certificate_identity != expected_certificate_identity
        || verification.source_ref != policy.required_source_ref
        || !is_lower_hex(&verification.source_commit_sha, 40)
        || verification.predicate_type != policy.attestation_predicate_type
        || !verification.self_hosted_runners_denied
        || verification.verified_attestation_count != 1
        || !(1..=crate::domain::publication::MAX_PUBLICATION_VERIFIED_TIMESTAMPS)
            .contains(&verification.verified_timestamp_count)
        || verification.authoritative
    {
        return Err(AppError::new(
            "MCL_PUBLICATION_ATTESTATION_INVALID",
            "attestation verification does not satisfy the closed publication receipt shape",
            false,
            "Use the fully validated record produced from the pinned verifier and committed publication policy.",
        ));
    }
    Ok(())
}

fn validate_publication_receipt_binding(
    stage: &PublicationStageSnapshot,
    verification: &PublicationAttestationVerification,
) -> Result<(), AppError> {
    if stage.stage.authoritative
        || verification.authoritative
        || stage.stage.report_artifact_hash != verification.report_artifact_hash
        || stage.stage.report_artifact_hash != verification.report_content_hash
        || stage.stage.attestation_bundle_artifact_hash != verification.attestation_bundle_hash
    {
        return Err(AppError::new(
            "MCL_PUBLICATION_RECEIPT_BINDING_INVALID",
            "attestation verification does not bind the exact non-authoritative publication stage",
            false,
            "Verify the exact staged report and Sigstore bundle before recording ingestion.",
        ));
    }
    Ok(())
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn canonical_string(value: &Value) -> Result<String, AppError> {
    String::from_utf8(canonical_json(value)?).map_err(|error| {
        AppError::new(
            "MCL_CANONICAL_JSON_INVALID",
            error.to_string(),
            false,
            "Report this canonical JSON encoding defect.",
        )
    })
}

fn ensure_registered_cas_bound(count: usize) -> Result<(), AppError> {
    if count > MAX_REGISTERED_CAS_HASHES {
        return Err(AppError::new(
            "MCL_CAS_SCAN_LIMIT",
            "registered CAS inventory exceeded its reviewed bound",
            false,
            "Inspect storage growth before increasing the CAS inventory policy.",
        ));
    }
    Ok(())
}

fn read_bounded_hash_column(
    connection: &Connection,
    query: &str,
    context: &'static str,
) -> Result<Vec<String>, AppError> {
    let mut statement = connection
        .prepare(query)
        .map_err(|error| AppError::database(context, error))?;
    let hashes = statement
        .query_map([(MAX_REGISTERED_CAS_HASHES + 1) as i64], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| AppError::database(context, error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::database(context, error))?;
    ensure_registered_cas_bound(hashes.len())?;
    Ok(hashes)
}

fn validate_uuid(value: &str, label: &str) -> Result<(), AppError> {
    let parsed = Uuid::parse_str(value).map_err(|_| {
        AppError::new(
            "MCL_ID_INVALID",
            format!("{label} identity is not a UUID"),
            false,
            "Use the exact stable ID returned by the engine.",
        )
    })?;
    if parsed.get_version_num() != 7 {
        return Err(AppError::new(
            "MCL_ID_INVALID",
            format!("{label} identity is not UUIDv7"),
            false,
            "Use the exact stable ID returned by the engine.",
        ));
    }
    Ok(())
}

fn validate_job_priority(priority: i32) -> Result<(), AppError> {
    if !(-1_000..=1_000).contains(&priority) {
        return Err(AppError::new(
            "MCL_VERIFIER_JOB_PRIORITY_INVALID",
            "verifier job priority must be between -1000 and 1000",
            false,
            "Use a bounded scheduling priority.",
        ));
    }
    Ok(())
}

fn validate_worker_lease(worker: &str, lease_seconds: u64) -> Result<(), AppError> {
    if worker.trim().is_empty()
        || worker.len() > 128
        || !worker
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || !(1..=7_200).contains(&lease_seconds)
    {
        return Err(AppError::new(
            "MCL_VERIFIER_LEASE_INVALID",
            "verifier worker identity or lease duration is outside policy",
            false,
            "Use a short stable worker name and a lease between 1 and 7200 seconds.",
        ));
    }
    Ok(())
}

fn validate_verifier_job_references(
    connection: &Connection,
    request: &VerifierJobRequest,
) -> Result<(), AppError> {
    let environment_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM environments WHERE environment_hash = ?1)",
            [&request.environment_hash],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| AppError::database("validate verifier job environment", error))?;
    if !environment_exists {
        return Err(AppError::new(
            "MCL_VERIFIER_ENVIRONMENT_INVALID",
            format!(
                "verifier environment {} is not registered",
                request.environment_hash
            ),
            false,
            "Register and select an exact pinned environment before verification.",
        ));
    }
    let media_type = connection
        .query_row(
            "SELECT media_type FROM artifacts WHERE artifact_hash = ?1",
            [&request.module_artifact_hash],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| AppError::database("validate verifier job module artifact", error))?;
    if media_type.as_deref() != Some("text/x-lean") {
        return Err(AppError::new(
            "MCL_VERIFIER_ARTIFACT_INVALID",
            format!(
                "verifier module artifact {} is missing or is not Lean source",
                request.module_artifact_hash
            ),
            false,
            "Ingest and select an exact registered Lean source artifact before verification.",
        ));
    }
    Ok(())
}

fn requeue_expired_jobs(connection: &Connection) -> Result<usize, AppError> {
    connection
        .execute(
            "UPDATE jobs SET state = 'queued', lease_owner = NULL, lease_expires_at = NULL, progress_json = '{\"phase\":\"requeued_after_expired_lease\"}', updated_at = unixepoch() WHERE job_type IN ('lean_elaboration', 'lean_audit') AND state IN ('leased', 'running') AND lease_expires_at < unixepoch()",
            [],
        )
        .map_err(|error| AppError::database("requeue expired verifier jobs", error))
}

fn verifier_job_conflict(job_id: &str) -> AppError {
    AppError::new(
        "MCL_VERIFIER_JOB_CONFLICT",
        format!("verifier job {job_id} is not leased by this worker with a live lease"),
        true,
        "Reload job status and lease the next eligible job before starting work.",
    )
}

fn audit_job_conflict(job_id: &str) -> AppError {
    AppError::new(
        "MCL_AUDIT_JOB_CONFLICT",
        format!("audit job {job_id} is not leased by this worker with a live lease"),
        true,
        "Reload audit status and lease the next eligible audit before starting work.",
    )
}

fn validate_audit_job_references(
    connection: &Connection,
    request: &LeanAuditRequest,
) -> Result<(), AppError> {
    let subject = connection
        .query_row(
            "SELECT r.record_type, v.payload_json FROM records r JOIN record_versions v ON v.object_id = r.object_id WHERE r.object_id = ?1 AND v.version_hash = ?2",
            params![request.subject.object_id, request.subject.version_hash],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| AppError::database("validate audit formalization", error))?;
    let Some((kind, payload_json)) = subject else {
        return Err(AppError::new(
            "MCL_AUDIT_SUBJECT_INVALID",
            "audit request does not resolve to an exact formalization version",
            false,
            "Select an exact canonical formalization object and version.",
        ));
    };
    if kind != "formalization" {
        return Err(AppError::new(
            "MCL_AUDIT_SUBJECT_INVALID",
            "audit subject is not a formalization",
            false,
            "Select an exact canonical formalization object and version.",
        ));
    }
    let formalization: FormalizationPayload =
        serde_json::from_str(&payload_json).map_err(|error| {
            AppError::new(
                "MCL_AUDIT_SUBJECT_INVALID",
                format!("stored formalization payload is invalid: {error}"),
                false,
                "Quarantine the database and restore a verified backup.",
            )
        })?;
    if formalization.environment_hash != request.environment_hash
        || formalization.module_artifact_hash != request.module_artifact_hash
        || formalization.declaration_name != request.declaration_name
    {
        return Err(AppError::new(
            "MCL_AUDIT_FORMALIZATION_MISMATCH",
            "audit request does not match the exact formalization target",
            false,
            "Derive audit inputs from the exact formalization version.",
        ));
    }
    let evidence = read_evidence(connection, &request.diagnostic_evidence_id)?;
    if evidence.evidence_hash != request.diagnostic_evidence_hash
        || evidence.payload.subject != request.subject
        || evidence.payload.evidence_kind != EvidenceKind::LeanElaboration
        || evidence.payload.result != EvidenceResult::Accepted
        || evidence.payload.authority_class != EvidenceAuthorityClass::Diagnostic
        || evidence.payload.stale
        || evidence.payload.environment_hash.as_deref() != Some(request.environment_hash.as_str())
        || !evidence
            .payload
            .artifact_hashes
            .iter()
            .any(|hash| hash == &request.module_artifact_hash)
    {
        return Err(AppError::new(
            "MCL_AUDIT_EVIDENCE_INVALID",
            "audit requires current accepted diagnostic elaboration evidence for the exact target",
            false,
            "Promote an accepted exact verifier diagnostic before requesting an audit.",
        ));
    }
    let policy = crate::domain::audit::committed_audit_policy()?;
    if policy.policy_hash()? != request.policy_hash {
        return Err(AppError::new(
            "MCL_AUDIT_POLICY_MISMATCH",
            "audit request does not use the committed policy identity",
            false,
            "Derive the audit policy hash from the committed policy.",
        ));
    }
    let environment_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM environments WHERE environment_hash = ?1)",
            [&request.environment_hash],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| AppError::database("validate audit environment", error))?;
    let module_media = connection
        .query_row(
            "SELECT media_type FROM artifacts WHERE artifact_hash = ?1",
            [&request.module_artifact_hash],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| AppError::database("validate audit module", error))?;
    if !environment_exists || module_media.as_deref() != Some("text/x-lean") {
        return Err(AppError::new(
            "MCL_AUDIT_REFERENCE_INVALID",
            "audit environment or Lean module is not registered",
            false,
            "Restore the exact registered audit inputs before enqueueing.",
        ));
    }
    Ok(())
}

fn validate_audit_evidence_references(
    connection: &Connection,
    payload: &EvidencePayload,
) -> Result<(), AppError> {
    if !matches!(
        payload.evidence_kind,
        EvidenceKind::ProofClosureScan | EvidenceKind::AxiomAudit
    ) {
        return Err(AppError::new(
            "MCL_AUDIT_EVIDENCE_INVALID",
            "audit evidence kind is not proof closure or axiom audit",
            false,
            "Derive the exact audit evidence pair from one completed audit report.",
        ));
    }
    let job_id = payload
        .producing_job_id
        .as_deref()
        .expect("validated audit job ID");
    let job = read_audit_job(connection, job_id)?;
    if !matches!(
        job.state,
        VerifierJobState::Succeeded | VerifierJobState::Failed
    ) || job.result_artifact_hash.is_none()
    {
        return Err(AppError::new(
            "MCL_AUDIT_EVIDENCE_JOB_INVALID",
            "audit evidence requires one completed audit job with a result artifact",
            true,
            "Wait for the exact audit job to finish before promoting evidence.",
        ));
    }
    if payload.subject != job.request.subject
        || payload.environment_hash.as_deref() != Some(job.request.environment_hash.as_str())
        || !payload
            .artifact_hashes
            .iter()
            .any(|hash| hash == &job.request.module_artifact_hash)
        || !payload
            .artifact_hashes
            .iter()
            .any(|hash| Some(hash) == job.result_artifact_hash.as_ref())
    {
        return Err(AppError::new(
            "MCL_AUDIT_EVIDENCE_MISMATCH",
            "audit evidence does not match its exact subject, job, environment, module, and report",
            false,
            "Derive audit evidence only from the exact completed audit job.",
        ));
    }
    for artifact_hash in &payload.artifact_hashes {
        if !connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM artifacts WHERE artifact_hash = ?1)",
                [artifact_hash],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| AppError::database("validate audit evidence artifact", error))?
        {
            return Err(AppError::new(
                "MCL_AUDIT_EVIDENCE_ARTIFACT_INVALID",
                format!("audit evidence artifact {artifact_hash} is not registered"),
                false,
                "Register and verify every exact audit artifact before promotion.",
            ));
        }
    }
    Ok(())
}

fn validate_fidelity_evidence_references(
    connection: &Connection,
    payload: &EvidencePayload,
) -> Result<(), AppError> {
    let kind = connection
        .query_row(
            "SELECT r.record_type FROM records r JOIN record_versions v ON v.object_id = r.object_id WHERE r.object_id = ?1 AND v.version_hash = ?2",
            params![payload.subject.object_id, payload.subject.version_hash],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| AppError::database("validate fidelity subject", error))?;
    if kind.as_deref() != Some("formalization") {
        return Err(AppError::new(
            "MCL_FIDELITY_SUBJECT_INVALID",
            "fidelity evidence subject is not an exact formalization version",
            false,
            "Review one exact canonical formalization version.",
        ));
    }
    let run_id = payload
        .producing_run_id
        .as_deref()
        .expect("validated fidelity run ID");
    if !connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE run_id = ?1)",
            [run_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| AppError::database("validate fidelity run", error))?
    {
        return Err(AppError::new(
            "MCL_FIDELITY_RUN_INVALID",
            "fidelity evidence producing run does not exist",
            false,
            "Start and reference an exact canonical review run.",
        ));
    }
    let mut controlled_report = false;
    for hash in &payload.artifact_hashes {
        let artifact = read_artifact(connection, hash)?;
        if artifact.media_type == crate::domain::ArtifactMediaType::Json
            && artifact.creation_source == crate::domain::ArtifactCreationSource::HumanReview
            && artifact.restriction == crate::domain::ArtifactRestriction::Private
            && artifact
                .semantic_metadata
                .get("artifact_role")
                .is_some_and(|role| role == "fidelity_review_report")
        {
            controlled_report = true;
        }
    }
    if !controlled_report {
        return Err(AppError::new(
            "MCL_FIDELITY_REPORT_INVALID",
            "fidelity evidence lacks a controlled private review report",
            false,
            "Create the report through the shared fidelity review application path.",
        ));
    }
    Ok(())
}

fn validate_fidelity_evidence_head(
    connection: &Connection,
    payload: &EvidencePayload,
) -> Result<(), AppError> {
    let mut statement = connection
        .prepare("SELECT evidence_id FROM evidence WHERE subject_object_id = ?1 AND subject_version_hash = ?2 AND evidence_kind = 'statement_fidelity_review' ORDER BY created_at, evidence_id")
        .map_err(|error| AppError::database("prepare fidelity evidence head", error))?;
    let rows = statement
        .query_map(
            params![payload.subject.object_id, payload.subject.version_hash],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| AppError::database("query fidelity evidence head", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::database("read fidelity evidence head", error))?;
    let ids = rows.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let mut superseded = BTreeSet::new();
    for id in &rows {
        let existing = read_evidence(connection, id).map_err(|error| {
            AppError::new(
                "MCL_FIDELITY_EVIDENCE_INTEGRITY_FAILED",
                format!(
                    "stored fidelity evidence failed integrity validation: {}",
                    error.message
                ),
                false,
                "Quarantine the database and restore a verified backup.",
            )
        })?;
        if let Some(id) = existing.payload.supersedes_evidence_id {
            superseded.insert(id);
        }
    }
    let heads = ids
        .into_iter()
        .filter(|id| !superseded.contains(*id))
        .collect::<Vec<_>>();
    let expected = match heads.as_slice() {
        [] => None,
        [head] => Some(*head),
        _ => {
            return Err(AppError::new(
                "MCL_FIDELITY_EVIDENCE_INTEGRITY_FAILED",
                "fidelity evidence has multiple unsuperseded heads",
                false,
                "Quarantine the conflicting review history for explicit resolution.",
            ));
        }
    };
    if payload.supersedes_evidence_id.as_deref() != expected {
        return Err(AppError::new(
            "MCL_FIDELITY_REVIEW_CONFLICT",
            "fidelity review does not supersede the current exact evidence head",
            true,
            "Reload fidelity status and retry against the current evidence head.",
        ));
    }
    Ok(())
}

fn validate_diagnostic_evidence_references(
    connection: &Connection,
    payload: &EvidencePayload,
) -> Result<(), AppError> {
    let subject = connection
        .query_row(
            "SELECT r.record_type, v.payload_json FROM records r JOIN record_versions v ON v.object_id = r.object_id WHERE r.object_id = ?1 AND v.version_hash = ?2",
            params![payload.subject.object_id, payload.subject.version_hash],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| AppError::database("validate evidence subject", error))?;
    let Some((subject_kind, subject_payload_json)) = subject else {
        return Err(AppError::new(
            "MCL_EVIDENCE_SUBJECT_INVALID",
            "diagnostic Lean evidence must reference one exact formalization version",
            false,
            "Select an exact canonical formalization object and version.",
        ));
    };
    if subject_kind != "formalization" {
        return Err(AppError::new(
            "MCL_EVIDENCE_SUBJECT_INVALID",
            "diagnostic Lean evidence must reference one exact formalization version",
            false,
            "Select an exact canonical formalization object and version.",
        ));
    }
    let formalization: FormalizationPayload =
        serde_json::from_str(&subject_payload_json).map_err(|error| {
            AppError::new(
                "MCL_EVIDENCE_SUBJECT_INVALID",
                format!("stored formalization payload is invalid: {error}"),
                false,
                "Quarantine the database and restore a verified backup.",
            )
        })?;
    let job_id = payload
        .producing_job_id
        .as_deref()
        .expect("validated job ID");
    let job = read_verifier_job(connection, job_id)?;
    if !matches!(
        job.state,
        VerifierJobState::Succeeded | VerifierJobState::Failed
    ) || job.result_artifact_hash.is_none()
    {
        return Err(AppError::new(
            "MCL_EVIDENCE_JOB_INVALID",
            "diagnostic evidence requires one completed verifier job with a result artifact",
            true,
            "Wait for the exact verifier job to finish before creating evidence.",
        ));
    }
    if formalization.environment_hash != job.request.environment_hash
        || formalization.module_artifact_hash != job.request.module_artifact_hash
        || formalization.declaration_name != job.request.declaration_name
    {
        return Err(AppError::new(
            "MCL_EVIDENCE_FORMALIZATION_MISMATCH",
            "formalization and verifier job do not describe the same exact target",
            false,
            "Run verification against the exact formalization environment, module, and declaration.",
        ));
    }
    if payload.environment_hash.as_deref() != Some(&job.request.environment_hash) {
        return Err(AppError::new(
            "MCL_EVIDENCE_ENVIRONMENT_MISMATCH",
            "evidence environment does not match its producing verifier job",
            false,
            "Derive diagnostic evidence from the exact producing job.",
        ));
    }
    if !payload
        .artifact_hashes
        .iter()
        .any(|hash| hash == &job.request.module_artifact_hash)
        || !payload
            .artifact_hashes
            .iter()
            .any(|hash| Some(hash) == job.result_artifact_hash.as_ref())
    {
        return Err(AppError::new(
            "MCL_EVIDENCE_ARTIFACT_MISMATCH",
            "evidence artifacts do not contain the exact module and terminal report",
            false,
            "Derive the artifact closure from the completed verifier job and report.",
        ));
    }
    for artifact_hash in &payload.artifact_hashes {
        if !connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM artifacts WHERE artifact_hash = ?1)",
                [artifact_hash],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| AppError::database("validate evidence artifact", error))?
        {
            return Err(AppError::new(
                "MCL_EVIDENCE_ARTIFACT_INVALID",
                format!("evidence artifact {artifact_hash} is not registered"),
                false,
                "Register and verify every exact evidence artifact before promotion.",
            ));
        }
    }
    if let Some(run_id) = &payload.producing_run_id {
        if !connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM runs WHERE run_id = ?1)",
                [run_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| AppError::database("validate evidence run", error))?
        {
            return Err(AppError::new(
                "MCL_EVIDENCE_RUN_INVALID",
                format!("evidence run {run_id} does not exist"),
                false,
                "Select an exact producing run or omit it for a job-only diagnostic.",
            ));
        }
    }
    Ok(())
}

fn read_evidence(connection: &Connection, evidence_id: &str) -> Result<EvidenceSnapshot, AppError> {
    let row = connection
        .query_row(
            "SELECT evidence_hash, metadata_json, created_at, created_by, subject_object_id, subject_version_hash, evidence_kind, result, authority_class, run_id, job_id, environment_hash, artifact_hashes_json, verifier_identity, stale_reason, publication_receipt_hash, publication_stage_hash FROM evidence WHERE evidence_id = ?1 AND evidence_hash IS NOT NULL",
            [evidence_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?, row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?, row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?, row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?, row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?, row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?, row.get::<_, Option<String>>(11)?,
                    row.get::<_, String>(12)?, row.get::<_, String>(13)?,
                    row.get::<_, Option<String>>(14)?,
                    row.get::<_, Option<String>>(15)?, row.get::<_, Option<String>>(16)?,
                ))
            },
        )
        .optional()
        .map_err(|error| AppError::database("read evidence", error))?;
    let Some((
        evidence_hash,
        payload_json,
        created_at,
        created_by,
        subject_object_id,
        subject_version_hash,
        kind,
        result,
        authority,
        run_id,
        job_id,
        environment_hash,
        artifact_hashes_json,
        verifier_identity,
        stale_reason,
        publication_receipt_hash,
        publication_stage_hash,
    )) = row
    else {
        return Err(AppError::new(
            "MCL_EVIDENCE_NOT_FOUND",
            format!("evidence {evidence_id} does not exist"),
            false,
            "Use an exact evidence ID returned by the canonical store.",
        ));
    };
    let payload: EvidencePayload = serde_json::from_str(&payload_json).map_err(|error| {
        AppError::new(
            "MCL_EVIDENCE_INTEGRITY_FAILED",
            format!("stored evidence payload is invalid: {error}"),
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?;
    payload.validate().map_err(|error| {
        AppError::new(
            "MCL_EVIDENCE_INTEGRITY_FAILED",
            error.message,
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?;
    let artifacts: Vec<String> = serde_json::from_str(&artifact_hashes_json).map_err(|error| {
        AppError::new(
            "MCL_EVIDENCE_INTEGRITY_FAILED",
            error.to_string(),
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?;
    let publication_projection_matches = match &payload.publication_authority {
        Some(binding) => {
            publication_receipt_hash.as_deref() == Some(binding.ingestion_receipt_hash.as_str())
                && publication_stage_hash.as_deref() == Some(binding.stage_hash.as_str())
        }
        None => publication_receipt_hash.is_none() && publication_stage_hash.is_none(),
    };
    if payload.evidence_hash()? != evidence_hash
        || payload.subject.object_id != subject_object_id
        || payload.subject.version_hash != subject_version_hash
        || payload.evidence_kind.as_str() != kind
        || payload.result.as_str() != result
        || payload.authority_class.as_str() != authority
        || payload.producing_run_id != run_id
        || payload.producing_job_id != job_id
        || payload.environment_hash != environment_hash
        || payload.artifact_hashes != artifacts
        || payload.verifier_or_reviewer_identity != verifier_identity
        || payload.stale_reason != stale_reason
        || !publication_projection_matches
    {
        return Err(AppError::new(
            "MCL_EVIDENCE_INTEGRITY_FAILED",
            format!("stored evidence projections disagree with {evidence_id}"),
            false,
            "Quarantine the database and restore a verified backup.",
        ));
    }
    if let Some(binding) = &payload.publication_authority {
        validate_stored_publication_authority_evidence(connection, &payload, binding)
            .map_err(|error| publication_authority_integrity_error(error.message))?;
    }
    Ok(EvidenceSnapshot {
        evidence_id: evidence_id.to_owned(),
        evidence_hash,
        payload,
        created_at,
        created_by,
    })
}

fn mutation_input_hash(
    operation: &str,
    object_id: Option<&str>,
    expected_head: Option<&str>,
    draft: &RecordDraft,
    actor: &str,
) -> Result<String, AppError> {
    value_hash(&json!({
        "operation": operation,
        "object_id": object_id,
        "expected_head": expected_head,
        "record_type": draft.kind,
        "schema_version": draft.schema_version,
        "payload": draft.payload,
        "searchable_text": draft.searchable_text,
        "actor": actor,
    }))
}

fn edge_create_input_hash(draft: &EdgeDraft, actor: &str) -> Result<String, AppError> {
    value_hash(&json!({
        "operation": "edge.create",
        "kind": draft.kind,
        "source_object_id": draft.source_object_id,
        "source_version_hash": draft.source_version_hash,
        "target_object_id": draft.target_object_id,
        "target_version_hash": draft.target_version_hash,
        "payload": draft.payload,
        "actor": actor,
    }))
}

fn read_idempotent_result<T: DeserializeOwned>(
    connection: &Connection,
    operation: &str,
    key: &str,
    input_hash: &str,
) -> Result<Option<T>, AppError> {
    let existing: Option<(String, String)> = connection
        .query_row(
            "SELECT input_hash, result_json FROM idempotency_results WHERE operation = ?1 AND idempotency_key = ?2",
            params![operation, key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| AppError::database("read idempotency result", error))?;
    let Some((existing_hash, result_json)) = existing else {
        return Ok(None);
    };
    if existing_hash != input_hash {
        return Err(AppError::new(
            "MCL_IDEMPOTENCY_CONFLICT",
            format!("idempotency key `{key}` was already used with different input"),
            false,
            "Use the original input or choose a new idempotency key.",
        ));
    }
    serde_json::from_str(&result_json)
        .map(Some)
        .map_err(|error| {
            AppError::new(
                "MCL_IDEMPOTENCY_RESULT_INVALID",
                error.to_string(),
                false,
                "Run `mcl doctor` and restore a verified backup if stored state was altered.",
            )
        })
}

fn write_idempotent_result<T: Serialize>(
    connection: &Connection,
    operation: &str,
    key: &str,
    input_hash: &str,
    result: &T,
) -> Result<(), AppError> {
    let result_json = serde_json::to_string(result).map_err(|error| {
        AppError::new(
            "MCL_IDEMPOTENCY_RESULT_INVALID",
            error.to_string(),
            false,
            "Report this deterministic serialization defect.",
        )
    })?;
    connection
        .execute(
            "INSERT INTO idempotency_results(operation, idempotency_key, input_hash, result_json, created_at) VALUES (?1, ?2, ?3, ?4, unixepoch())",
            params![operation, key, input_hash, result_json],
        )
        .map_err(|error| AppError::database("write idempotency result", error))?;
    Ok(())
}

fn update_search_projection(
    connection: &Connection,
    object_id: &str,
    draft: &RecordDraft,
) -> Result<(), AppError> {
    connection
        .execute(
            "DELETE FROM record_search WHERE object_id = ?1",
            [object_id],
        )
        .map_err(|error| AppError::database("remove old search projection", error))?;
    connection
        .execute(
            "INSERT INTO record_search(object_id, record_type, searchable_text) VALUES (?1, ?2, ?3)",
            params![object_id, draft.kind.as_str(), draft.searchable_text],
        )
        .map_err(|error| AppError::database("write search projection", error))?;
    Ok(())
}

fn read_environment(
    connection: &Connection,
    environment_hash: &str,
) -> Result<EnvironmentSnapshot, AppError> {
    let row = connection
        .query_row(
            "SELECT manifest_json, trust_profile, created_at, created_by FROM environments WHERE environment_hash = ?1",
            [environment_hash],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|error| AppError::database("read environment", error))?;
    let Some((manifest_json, stored_trust_profile, created_at, created_by)) = row else {
        return Err(AppError::new(
            "MCL_ENVIRONMENT_NOT_FOUND",
            format!("environment {environment_hash} is not registered"),
            false,
            "Register or select an exact environment hash before formalization or verification.",
        ));
    };
    let manifest: EnvironmentManifest = serde_json::from_str(&manifest_json).map_err(|error| {
        AppError::new(
            "MCL_ENVIRONMENT_INTEGRITY_FAILED",
            format!("stored environment manifest is invalid: {error}"),
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?;
    let computed_hash = manifest.environment_hash().map_err(|error| {
        AppError::new(
            "MCL_ENVIRONMENT_INTEGRITY_FAILED",
            format!("stored environment manifest fails validation: {error}"),
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?;
    if computed_hash != environment_hash || stored_trust_profile != manifest.trust_profile.as_str()
    {
        return Err(AppError::new(
            "MCL_ENVIRONMENT_INTEGRITY_FAILED",
            format!("stored environment identity does not match {environment_hash}"),
            false,
            "Quarantine the database and restore a verified backup.",
        ));
    }
    Ok(EnvironmentSnapshot {
        environment_hash: environment_hash.to_owned(),
        manifest,
        created_at,
        created_by,
    })
}

fn read_verifier_job(
    connection: &Connection,
    job_id: &str,
) -> Result<VerifierJobSnapshot, AppError> {
    type RawJob = (
        String,
        String,
        String,
        i32,
        Option<String>,
        Option<i64>,
        i64,
        String,
        Option<String>,
        Option<String>,
        String,
        i64,
        i64,
    );
    let row: Option<RawJob> = connection
        .query_row(
            "SELECT input_json, canonical_input_hash, state, priority, lease_owner, lease_expires_at, attempt_count, progress_json, result_artifact_hash, last_error_json, actor, created_at, updated_at FROM jobs WHERE job_id = ?1 AND job_type = 'lean_elaboration'",
            [job_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                ))
            },
        )
        .optional()
        .map_err(|error| AppError::database("read verifier job", error))?;
    let Some((
        input_json,
        canonical_input_hash,
        state,
        priority,
        lease_owner,
        lease_expires_at,
        attempt_count,
        progress_json,
        result_artifact_hash,
        last_error_json,
        actor,
        created_at,
        updated_at,
    )) = row
    else {
        return Err(AppError::new(
            "MCL_VERIFIER_JOB_NOT_FOUND",
            format!("verifier job {job_id} does not exist"),
            false,
            "Use an exact verifier job ID returned by enqueue or listing.",
        ));
    };
    let request: VerifierJobRequest = serde_json::from_str(&input_json).map_err(|error| {
        AppError::new(
            "MCL_VERIFIER_JOB_INTEGRITY_FAILED",
            format!("stored verifier request is invalid: {error}"),
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?;
    request.validate().map_err(|error| {
        AppError::new(
            "MCL_VERIFIER_JOB_INTEGRITY_FAILED",
            format!("stored verifier request fails validation: {error}"),
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?;
    let computed_hash = value_hash(&serde_json::to_value(&request).map_err(|error| {
        AppError::new(
            "MCL_VERIFIER_JOB_INTEGRITY_FAILED",
            error.to_string(),
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?)?;
    if computed_hash != canonical_input_hash {
        return Err(AppError::new(
            "MCL_VERIFIER_JOB_INTEGRITY_FAILED",
            format!("stored verifier job input hash disagrees for {job_id}"),
            false,
            "Quarantine the database and restore a verified backup.",
        ));
    }
    let attempt_count = u32::try_from(attempt_count).map_err(|_| {
        AppError::new(
            "MCL_VERIFIER_JOB_INTEGRITY_FAILED",
            "stored verifier attempt count is outside range",
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?;
    Ok(VerifierJobSnapshot {
        job_id: job_id.to_owned(),
        request,
        canonical_input_hash,
        state: VerifierJobState::from_str(&state)?,
        priority,
        lease_owner,
        lease_expires_at,
        attempt_count,
        progress: serde_json::from_str(&progress_json).map_err(|error| {
            AppError::new(
                "MCL_VERIFIER_JOB_INTEGRITY_FAILED",
                format!("stored verifier progress is invalid: {error}"),
                false,
                "Quarantine the database and restore a verified backup.",
            )
        })?,
        result_artifact_hash,
        last_error: last_error_json
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(|error| {
                AppError::new(
                    "MCL_VERIFIER_JOB_INTEGRITY_FAILED",
                    format!("stored verifier error is invalid: {error}"),
                    false,
                    "Quarantine the database and restore a verified backup.",
                )
            })?,
        actor,
        created_at,
        updated_at,
    })
}

fn read_audit_job(connection: &Connection, job_id: &str) -> Result<LeanAuditJobSnapshot, AppError> {
    type RawJob = (
        String,
        String,
        String,
        i32,
        Option<String>,
        Option<i64>,
        i64,
        String,
        Option<String>,
        Option<String>,
        String,
        i64,
        i64,
    );
    let row: Option<RawJob> = connection
        .query_row(
            "SELECT input_json, canonical_input_hash, state, priority, lease_owner, lease_expires_at, attempt_count, progress_json, result_artifact_hash, last_error_json, actor, created_at, updated_at FROM jobs WHERE job_id = ?1 AND job_type = 'lean_audit'",
            [job_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                ))
            },
        )
        .optional()
        .map_err(|error| AppError::database("read audit job", error))?;
    let Some((
        input_json,
        canonical_input_hash,
        state,
        priority,
        lease_owner,
        lease_expires_at,
        attempt_count,
        progress_json,
        result_artifact_hash,
        last_error_json,
        actor,
        created_at,
        updated_at,
    )) = row
    else {
        return Err(AppError::new(
            "MCL_AUDIT_JOB_NOT_FOUND",
            format!("audit job {job_id} does not exist"),
            false,
            "Use an exact audit job ID returned by enqueue or listing.",
        ));
    };
    let request: LeanAuditRequest = serde_json::from_str(&input_json).map_err(|error| {
        AppError::new(
            "MCL_AUDIT_JOB_INTEGRITY_FAILED",
            format!("stored audit request is invalid: {error}"),
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?;
    request.validate().map_err(|error| {
        AppError::new(
            "MCL_AUDIT_JOB_INTEGRITY_FAILED",
            format!("stored audit request fails validation: {error}"),
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?;
    let computed_hash = request.request_hash()?;
    if computed_hash != canonical_input_hash {
        return Err(AppError::new(
            "MCL_AUDIT_JOB_INTEGRITY_FAILED",
            format!("stored audit job input hash disagrees for {job_id}"),
            false,
            "Quarantine the database and restore a verified backup.",
        ));
    }
    let attempt_count = u32::try_from(attempt_count).map_err(|_| {
        AppError::new(
            "MCL_AUDIT_JOB_INTEGRITY_FAILED",
            "stored audit attempt count is outside range",
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?;
    Ok(LeanAuditJobSnapshot {
        job_id: job_id.to_owned(),
        request,
        canonical_input_hash,
        state: VerifierJobState::from_str(&state)?,
        priority,
        lease_owner,
        lease_expires_at,
        attempt_count,
        progress: serde_json::from_str(&progress_json).map_err(|error| {
            AppError::new(
                "MCL_AUDIT_JOB_INTEGRITY_FAILED",
                format!("stored audit progress is invalid: {error}"),
                false,
                "Quarantine the database and restore a verified backup.",
            )
        })?,
        result_artifact_hash,
        last_error: last_error_json
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(|error| {
                AppError::new(
                    "MCL_AUDIT_JOB_INTEGRITY_FAILED",
                    format!("stored audit error is invalid: {error}"),
                    false,
                    "Quarantine the database and restore a verified backup.",
                )
            })?,
        actor,
        created_at,
        updated_at,
    })
}

fn read_publication_stage(
    connection: &Connection,
    stage_hash: &str,
) -> Result<PublicationStageSnapshot, AppError> {
    let row = connection
        .query_row(
            "SELECT schema_version, report_artifact_hash, report_byte_size, retained_closure_artifact_hash, retained_closure_byte_size, attestation_bundle_artifact_hash, attestation_bundle_byte_size, retained_artifact_count, stage_json, authoritative, created_at, created_by FROM publication_stages WHERE stage_hash = ?1",
            [stage_hash],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, String>(11)?,
                ))
            },
        )
        .optional()
        .map_err(|error| AppError::database("read publication stage", error))?;
    let Some((
        stored_schema_version,
        stored_report_artifact_hash,
        stored_report_byte_size,
        stored_retained_closure_artifact_hash,
        stored_retained_closure_byte_size,
        stored_attestation_bundle_artifact_hash,
        stored_attestation_bundle_byte_size,
        stored_retained_artifact_count,
        stage_json,
        stored_authoritative,
        created_at,
        created_by,
    )) = row
    else {
        return Err(AppError::new(
            "MCL_PUBLICATION_STAGE_NOT_FOUND",
            format!("publication stage {stage_hash} is not registered"),
            false,
            "Stage and verify the exact retained publication candidate first.",
        ));
    };
    let stage: PublicationStage = serde_json::from_str(&stage_json).map_err(|error| {
        publication_stage_integrity_error(format!(
            "stored publication stage JSON is invalid: {error}"
        ))
    })?;
    stage.validate().map_err(|error| {
        publication_stage_integrity_error(format!(
            "stored publication stage fails validation: {error}"
        ))
    })?;
    let stage_value = serde_json::to_value(&stage).map_err(|error| {
        publication_stage_integrity_error(format!(
            "stored publication stage cannot be serialized: {error}"
        ))
    })?;
    let canonical_stage_json = canonical_string(&stage_value).map_err(|error| {
        publication_stage_integrity_error(format!(
            "stored publication stage cannot be canonicalized: {error}"
        ))
    })?;
    let computed_stage_hash = stage.stage_hash().map_err(|error| {
        publication_stage_integrity_error(format!(
            "stored publication stage identity cannot be recomputed: {error}"
        ))
    })?;
    let report_byte_size = u64::try_from(stored_report_byte_size).map_err(|_| {
        publication_stage_integrity_error("stored publication report byte size is negative")
    })?;
    let retained_closure_byte_size =
        u64::try_from(stored_retained_closure_byte_size).map_err(|_| {
            publication_stage_integrity_error(
                "stored retained publication closure byte size is negative",
            )
        })?;
    let attestation_bundle_byte_size =
        u64::try_from(stored_attestation_bundle_byte_size).map_err(|_| {
            publication_stage_integrity_error(
                "stored publication attestation bundle byte size is negative",
            )
        })?;
    let retained_artifact_count =
        usize::try_from(stored_retained_artifact_count).map_err(|_| {
            publication_stage_integrity_error("stored retained artifact count is negative")
        })?;
    if !is_lower_hex(stage_hash, 64)
        || computed_stage_hash != stage_hash
        || stage_json != canonical_stage_json
        || stored_schema_version != stage.schema_version
        || stored_report_artifact_hash != stage.report_artifact_hash
        || report_byte_size != stage.report_byte_size
        || stored_retained_closure_artifact_hash != stage.retained_closure_artifact_hash
        || retained_closure_byte_size != stage.retained_closure_byte_size
        || stored_attestation_bundle_artifact_hash != stage.attestation_bundle_artifact_hash
        || attestation_bundle_byte_size != stage.attestation_bundle_byte_size
        || retained_artifact_count != stage.retained_artifacts.len()
        || stored_authoritative != 0
        || stage.authoritative
        || created_by.trim().is_empty()
        || created_by.chars().count() > 256
    {
        return Err(publication_stage_integrity_error(format!(
            "stored publication stage projections disagree for {stage_hash}"
        )));
    }
    Ok(PublicationStageSnapshot {
        stage_hash: stage_hash.to_owned(),
        stage,
        created_at,
        created_by,
    })
}

fn read_publication_ingestion_receipt(
    connection: &Connection,
    receipt_hash: &str,
) -> Result<PublicationIngestionReceiptSnapshot, AppError> {
    let row = connection
        .query_row(
            "SELECT schema_version, stage_hash, report_artifact_hash, attestation_bundle_artifact_hash, raw_verification_hash, raw_verification_byte_size, receipt_byte_size, verification_json, authoritative, created_at, created_by FROM publication_ingestion_receipts WHERE receipt_hash = ?1",
            [receipt_hash],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, String>(10)?,
                ))
            },
        )
        .optional()
        .map_err(|error| AppError::database("read publication ingestion receipt", error))?;
    let Some((
        stored_schema_version,
        stage_hash,
        stored_report_artifact_hash,
        stored_attestation_bundle_artifact_hash,
        stored_raw_verification_hash,
        stored_raw_verification_byte_size,
        stored_receipt_byte_size,
        verification_json,
        stored_authoritative,
        created_at,
        created_by,
    )) = row
    else {
        return Err(publication_receipt_not_found(receipt_hash));
    };
    let verification: PublicationAttestationVerification = serde_json::from_str(&verification_json)
        .map_err(|error| {
            publication_receipt_integrity_error(format!(
                "stored attestation verification JSON is invalid: {error}"
            ))
        })?;
    validate_publication_attestation_shape(&verification).map_err(|error| {
        publication_receipt_integrity_error(format!(
            "stored attestation verification fails validation: {error}"
        ))
    })?;
    let verification_value = serde_json::to_value(&verification).map_err(|error| {
        publication_receipt_integrity_error(format!(
            "stored attestation verification cannot be serialized: {error}"
        ))
    })?;
    let canonical_verification_json = canonical_string(&verification_value).map_err(|error| {
        publication_receipt_integrity_error(format!(
            "stored attestation verification cannot be canonicalized: {error}"
        ))
    })?;
    let computed_receipt_hash = value_hash(&verification_value).map_err(|error| {
        publication_receipt_integrity_error(format!(
            "stored ingestion receipt identity cannot be recomputed: {error}"
        ))
    })?;
    let raw_verification_byte_size =
        u64::try_from(stored_raw_verification_byte_size).map_err(|_| {
            publication_receipt_integrity_error(
                "stored raw attestation verification byte size is negative",
            )
        })?;
    let receipt_byte_size = u64::try_from(stored_receipt_byte_size).map_err(|_| {
        publication_receipt_integrity_error("stored ingestion receipt byte size is negative")
    })?;
    let canonical_receipt_byte_size =
        u64::try_from(canonical_verification_json.len()).map_err(|_| {
            publication_receipt_integrity_error(
                "canonical stored ingestion receipt byte size cannot be represented",
            )
        })?;
    validate_publication_input_size(
        raw_verification_byte_size,
        "stored raw attestation verification",
    )
    .map_err(|error| publication_receipt_integrity_error(error.to_string()))?;
    validate_publication_input_size(receipt_byte_size, "stored ingestion receipt")
        .map_err(|error| publication_receipt_integrity_error(error.to_string()))?;
    if !is_lower_hex(receipt_hash, 64)
        || computed_receipt_hash != receipt_hash
        || verification_json != canonical_verification_json
        || receipt_byte_size != canonical_receipt_byte_size
        || stored_schema_version != verification.schema_version
        || stored_report_artifact_hash != verification.report_artifact_hash
        || stored_attestation_bundle_artifact_hash != verification.attestation_bundle_hash
        || stored_raw_verification_hash != verification.raw_verification_hash
        || stored_authoritative != 0
        || verification.authoritative
        || created_by.trim().is_empty()
        || created_by.chars().count() > 256
    {
        return Err(publication_receipt_integrity_error(format!(
            "stored publication ingestion receipt projections disagree for {receipt_hash}"
        )));
    }
    let stage = read_publication_stage(connection, &stage_hash).map_err(|error| {
        publication_receipt_integrity_error(format!(
            "stored ingestion receipt references an invalid stage: {error}"
        ))
    })?;
    validate_publication_receipt_binding(&stage, &verification).map_err(|error| {
        publication_receipt_integrity_error(format!(
            "stored ingestion receipt binding is invalid: {error}"
        ))
    })?;
    let retained_request_subject = publication_request_subject_binding(connection, &stage)
        .map_err(|error| {
            publication_receipt_integrity_error(format!(
                "stored ingestion receipt request binding is invalid: {}",
                error.message
            ))
        })?;
    if read_publication_receipt_subject_binding(connection, receipt_hash)?
        .as_ref()
        .is_some_and(|receipt_subject| receipt_subject != &retained_request_subject)
    {
        return Err(publication_receipt_integrity_error(format!(
            "stored ingestion receipt subject disagrees with its retained publication request for {receipt_hash}"
        )));
    }
    Ok(PublicationIngestionReceiptSnapshot {
        receipt_hash: receipt_hash.to_owned(),
        stage_hash,
        verification,
        raw_verification_byte_size,
        receipt_byte_size,
        created_at,
        created_by,
    })
}

fn publication_stage_not_found(
    report_artifact_hash: &str,
    attestation_bundle_artifact_hash: &str,
) -> AppError {
    AppError::new(
        "MCL_PUBLICATION_STAGE_NOT_FOUND",
        format!(
            "publication report {report_artifact_hash} and bundle {attestation_bundle_artifact_hash} are not staged"
        ),
        false,
        "Stage the exact retained publication candidate before ingestion.",
    )
}

fn publication_receipt_not_found(identity: &str) -> AppError {
    AppError::new(
        "MCL_PUBLICATION_RECEIPT_NOT_FOUND",
        format!("publication ingestion receipt for {identity} is not registered"),
        false,
        "Complete controlled publication ingestion or use its exact receipt identity.",
    )
}

fn publication_stage_integrity_error(message: impl Into<String>) -> AppError {
    AppError::new(
        "MCL_PUBLICATION_STAGE_INTEGRITY_FAILED",
        message,
        false,
        "Quarantine the database and restore a verified backup.",
    )
}

fn publication_receipt_integrity_error(message: impl Into<String>) -> AppError {
    AppError::new(
        "MCL_PUBLICATION_RECEIPT_INTEGRITY_FAILED",
        message,
        false,
        "Quarantine the database and restore a verified backup.",
    )
}

fn publication_authority_error(
    code: &'static str,
    message: impl Into<String>,
    corrective_action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, corrective_action)
}

fn publication_authority_integrity_error(message: impl Into<String>) -> AppError {
    AppError::new(
        "MCL_PUBLICATION_AUTHORITY_INTEGRITY_FAILED",
        message,
        false,
        "Quarantine the database and restore the exact receipt-bound authoritative evidence from a verified backup.",
    )
}

fn read_artifact(
    connection: &Connection,
    artifact_hash: &str,
) -> Result<ArtifactSnapshot, AppError> {
    let row = connection
        .query_row(
            "SELECT media_type, byte_size, creation_source, license_expression, restriction, metadata_json, created_at, created_by FROM artifacts WHERE artifact_hash = ?1",
            [artifact_hash],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| AppError::database("read artifact metadata", error))?;
    let Some((
        stored_media_type,
        byte_size,
        stored_creation_source,
        stored_license_expression,
        stored_restriction,
        metadata_json,
        created_at,
        created_by,
    )) = row
    else {
        return Err(AppError::new(
            "MCL_ARTIFACT_NOT_FOUND",
            format!("artifact {artifact_hash} is not registered"),
            false,
            "Ingest the artifact or use an exact registered artifact hash.",
        ));
    };
    let metadata: ArtifactMetadata = serde_json::from_str(&metadata_json).map_err(|error| {
        AppError::new(
            "MCL_ARTIFACT_INTEGRITY_FAILED",
            format!("stored artifact metadata is invalid: {error}"),
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?;
    let byte_size = u64::try_from(byte_size).map_err(|_| {
        AppError::new(
            "MCL_ARTIFACT_INTEGRITY_FAILED",
            "stored artifact byte size is negative",
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?;
    metadata.validate(byte_size).map_err(|error| {
        AppError::new(
            "MCL_ARTIFACT_INTEGRITY_FAILED",
            format!("stored artifact metadata fails validation: {error}"),
            false,
            "Quarantine the database and restore a verified backup.",
        )
    })?;
    if stored_media_type != metadata.media_type.as_str()
        || stored_creation_source != metadata.creation_source.as_str()
        || stored_license_expression != metadata.license_expression
        || stored_restriction != metadata.restriction.as_str()
    {
        return Err(AppError::new(
            "MCL_ARTIFACT_INTEGRITY_FAILED",
            format!("stored artifact metadata columns disagree for {artifact_hash}"),
            false,
            "Quarantine the database and restore a verified backup.",
        ));
    }
    Ok(ArtifactSnapshot {
        artifact_hash: artifact_hash.to_owned(),
        media_type: metadata.media_type,
        byte_size,
        creation_source: metadata.creation_source,
        license_expression: metadata.license_expression,
        restriction: metadata.restriction,
        semantic_metadata: metadata.semantic_metadata,
        created_at,
        created_by,
    })
}

fn artifact_snapshot_matches_metadata(
    artifact: &ArtifactSnapshot,
    metadata: &ArtifactMetadata,
    byte_size: u64,
) -> bool {
    artifact.byte_size == byte_size
        && artifact.media_type == metadata.media_type
        && artifact.creation_source == metadata.creation_source
        && artifact.license_expression == metadata.license_expression
        && artifact.restriction == metadata.restriction
        && artifact.semantic_metadata == metadata.semantic_metadata
}

fn read_snapshot(
    connection: &Connection,
    object_id: &str,
    version_hash: Option<&str>,
) -> Result<RecordSnapshot, AppError> {
    let version = match version_hash {
        Some(hash) => hash.to_owned(),
        None => connection
            .query_row(
                "SELECT head_version_hash FROM records WHERE object_id = ?1 AND tombstoned = 0",
                [object_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| AppError::database("read record head", error))?
            .ok_or_else(|| {
                AppError::new(
                    "MCL_RECORD_NOT_FOUND",
                    format!("canonical object {object_id} does not exist"),
                    false,
                    "Search for the stable object ID and retry.",
                )
            })?,
    };
    read_snapshot_by_version(connection, &version)
}

fn read_snapshot_by_version(
    connection: &Connection,
    version_hash: &str,
) -> Result<RecordSnapshot, AppError> {
    let raw: Option<RawRecordRow> = connection
        .query_row(
            "SELECT rv.object_id, r.record_type, rv.version_hash, rv.schema_version, rv.payload_json, rv.predecessor_hash, rv.created_at, rv.created_by FROM record_versions rv JOIN records r ON r.object_id = rv.object_id WHERE rv.version_hash = ?1",
            [version_hash],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| AppError::database("read record version", error))?;
    let Some((
        object_id,
        kind,
        version_hash,
        schema_version,
        payload_json,
        predecessor_hash,
        created_at,
        created_by,
    )) = raw
    else {
        return Err(AppError::new(
            "MCL_RECORD_VERSION_NOT_FOUND",
            format!("canonical record version {version_hash} does not exist"),
            false,
            "Search for the exact version hash and retry.",
        ));
    };
    let payload: Value = serde_json::from_str(&payload_json).map_err(|error| {
        AppError::new(
            "MCL_CANONICAL_PAYLOAD_INVALID",
            error.to_string(),
            false,
            "Run `mcl doctor` and restore a verified backup if stored state was altered.",
        )
    })?;
    Ok(RecordSnapshot {
        object_id,
        kind: RecordKind::from_str(&kind)?,
        version_hash,
        schema_version,
        payload,
        predecessor_hash,
        created_at,
        created_by,
    })
}

fn conflict(object_id: &str, expected: &str, actual: &str) -> AppError {
    AppError::new(
        "MCL_VERSION_CONFLICT",
        format!("object {object_id} head changed: expected {expected}, actual {actual}"),
        true,
        "Reload the current head, reconcile the proposal, and retry with a new idempotency key.",
    )
}

fn validate_edge_endpoint(
    connection: &Connection,
    label: &str,
    object_id: &str,
    version_hash: &str,
) -> Result<(), AppError> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM record_versions WHERE object_id = ?1 AND version_hash = ?2)",
            params![object_id, version_hash],
            |row| row.get(0),
        )
        .map_err(|error| AppError::database("validate edge endpoint", error))?;
    if !exists {
        return Err(AppError::new(
            "MCL_EDGE_ENDPOINT_INVALID",
            format!("{label} version {version_hash} is not owned by object {object_id}"),
            false,
            "Use an exact object and version pair returned by canonical lookup.",
        ));
    }
    Ok(())
}

fn hard_prerequisite_would_cycle(
    connection: &Connection,
    source_object_id: &str,
    target_object_id: &str,
) -> Result<bool, AppError> {
    if source_object_id == target_object_id {
        return Ok(true);
    }
    connection
        .query_row(
            "WITH RECURSIVE reachable(node) AS (SELECT target_object_id FROM edges WHERE edge_type = 'pedagogy.hard_prerequisite' AND source_object_id = ?1 UNION SELECT edge.target_object_id FROM edges edge JOIN reachable ON edge.source_object_id = reachable.node WHERE edge.edge_type = 'pedagogy.hard_prerequisite') SELECT EXISTS(SELECT 1 FROM reachable WHERE node = ?2)",
            params![target_object_id, source_object_id],
            |row| row.get(0),
        )
        .map_err(|error| AppError::database("validate hard prerequisite cycle", error))
}

fn read_edge(connection: &Connection, edge_id: &str) -> Result<EdgeSnapshot, AppError> {
    let raw: Option<RawEdgeRow> = connection
        .query_row(
            "SELECT edge_id, edge_type, source_object_id, source_version_hash, target_object_id, target_version_hash, payload_json, created_at, created_by FROM edges WHERE edge_id = ?1",
            [edge_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?)),
        )
        .optional()
        .map_err(|error| AppError::database("read canonical edge", error))?;
    let Some((
        edge_id,
        kind,
        source_object_id,
        source_version_hash,
        target_object_id,
        target_version_hash,
        payload_json,
        created_at,
        created_by,
    )) = raw
    else {
        return Err(AppError::new(
            "MCL_EDGE_NOT_FOUND",
            format!("canonical edge {edge_id} does not exist"),
            false,
            "Use an exact edge ID returned by the canonical store.",
        ));
    };
    Ok(EdgeSnapshot {
        edge_id,
        kind: EdgeKind::from_str(&kind)?,
        source_object_id,
        source_version_hash,
        target_object_id,
        target_version_hash,
        payload: serde_json::from_str(&payload_json).map_err(|error| {
            AppError::new(
                "MCL_CANONICAL_PAYLOAD_INVALID",
                error.to_string(),
                false,
                "Run `mcl doctor` and restore a verified backup if stored state was altered.",
            )
        })?,
        created_at,
        created_by,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};

    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;
    use crate::domain::schemas::ExactVersionReference;
    use crate::domain::{PublicationRetainedArtifactRole, PublicationStageArtifact};

    #[test]
    fn migration_produces_wal_database_with_fts5() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");

        assert_eq!(store.migration_version().expect("migration version"), 11);
        assert_eq!(store.journal_mode().expect("journal mode"), "wal");
        assert_eq!(store.integrity_check().expect("integrity"), "ok");
        store.schema_check().expect("required schema exists");
        store.fts5_check().expect("FTS5 query succeeds");
    }

    #[test]
    fn migration_is_idempotent() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("first migration succeeds");
        store.migrate().expect("second migration succeeds");
        assert_eq!(store.migration_version().expect("migration version"), 11);
    }

    #[test]
    fn migration_advances_v7_database() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        let transaction = store.connection.transaction().expect("legacy transaction");
        for (version, name, sql) in [
            (1_i64, "initial", MIGRATION_0001),
            (2_i64, "idempotency results", MIGRATION_0002),
            (3_i64, "record invariants", MIGRATION_0003),
            (4_i64, "edge invariants", MIGRATION_0004),
            (5_i64, "run event invariants", MIGRATION_0005),
            (6_i64, "environment invariants", MIGRATION_0006),
            (7_i64, "artifact invariants", MIGRATION_0007),
        ] {
            transaction
                .execute_batch(sql)
                .expect("legacy migration applies");
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, unixepoch())",
                    params![version, name],
                )
                .expect("legacy migration recorded");
        }
        transaction.commit().expect("legacy migrations commit");
        assert_eq!(store.migration_version().expect("legacy version"), 7);

        store.migrate().expect("forward migration succeeds");
        assert_eq!(store.migration_version().expect("current version"), 11);
        assert!(
            store
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pragma_table_info('environments') WHERE name = 'created_by')",
                    [],
                    |row| row.get::<_, bool>(0),
                )
                .expect("environment attribution column")
        );
        assert!(
            store
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pragma_table_info('artifacts') WHERE name = 'created_by')",
                    [],
                    |row| row.get::<_, bool>(0),
                )
                .expect("artifact attribution column")
        );
        assert!(
            store
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pragma_table_info('jobs') WHERE name = 'input_json')",
                    [],
                    |row| row.get::<_, bool>(0),
                )
                .expect("verifier job input column")
        );
        for column in ["publication_receipt_hash", "publication_stage_hash"] {
            assert!(
                store
                    .connection
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('evidence') WHERE name = ?1)",
                        [column],
                        |row| row.get::<_, bool>(0),
                    )
                    .expect("publication authority projection column"),
                "missing evidence projection {column}"
            );
        }
        for column in ["subject_object_id", "subject_version_hash"] {
            assert!(
                store
                    .connection
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('publication_ingestion_receipts') WHERE name = ?1)",
                        [column],
                        |row| row.get::<_, bool>(0),
                    )
                    .expect("publication receipt subject projection column"),
                "missing publication receipt projection {column}"
            );
        }
    }

    #[test]
    fn publication_stage_and_receipt_are_immutable_idempotent_cas_roots() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let (current_subject, _) =
            current_publication_subject(&mut store, "publication-receipt-current");
        let canonical_artifact =
            register_lean_artifact(&mut store, b"canonical", "publication-canonical-artifact");
        let stage = publication_stage_for_subject(
            &mut store,
            &current_subject,
            PublicationOutcome::Proof,
            "publication-receipt-current",
        );

        let first_stage = store
            .register_publication_stage(&stage, "publication-test", "publication-stage-first")
            .expect("stage registers");
        let repeated_stage = store
            .register_publication_stage(&stage, "publication-test", "publication-stage-first")
            .expect("stage retry succeeds");
        let alternate_stage_receipt = store
            .register_publication_stage(&stage, "publication-test", "publication-stage-second")
            .expect("matching stage gets another idempotency receipt");
        assert_eq!(first_stage, repeated_stage);
        assert_eq!(first_stage, alternate_stage_receipt);
        assert_eq!(
            first_stage.stage_hash,
            stage.stage_hash().expect("stage hash")
        );
        assert_eq!(
            store
                .get_publication_stage(
                    &stage.report_artifact_hash,
                    &stage.attestation_bundle_artifact_hash,
                )
                .expect("stage reads"),
            first_stage
        );
        let mut conflicting_stage = stage.clone();
        conflicting_stage.report_byte_size += 1;
        let conflict = store
            .register_publication_stage(
                &conflicting_stage,
                "publication-test",
                "publication-stage-conflict",
            )
            .expect_err("same report and bundle cannot bind a different stage");
        assert_eq!(conflict.code, "MCL_PUBLICATION_STAGE_CONFLICT");

        let verification = publication_verification(&stage);
        let receipt_byte_size =
            canonical_json(&serde_json::to_value(&verification).expect("verification serializes"))
                .expect("verification canonicalizes")
                .len() as u64;
        let first_receipt = store
            .register_publication_ingestion_receipt(
                &first_stage.stage_hash,
                &current_subject,
                &verification,
                123,
                receipt_byte_size,
                "publication-test",
                "publication-receipt-first",
            )
            .expect("receipt registers");
        drop(store);
        let mut store = Store::open(&database).expect("database reopens");
        store.migrate().expect("migration remains idempotent");
        let repeated_receipt = store
            .register_publication_ingestion_receipt(
                &first_stage.stage_hash,
                &current_subject,
                &verification,
                123,
                receipt_byte_size,
                "publication-test",
                "publication-receipt-first",
            )
            .expect("receipt retry succeeds");
        assert_eq!(first_receipt, repeated_receipt);
        assert_eq!(
            first_receipt.receipt_hash,
            value_hash(&serde_json::to_value(&verification).expect("verification serializes"))
                .expect("receipt hash")
        );
        assert_eq!(
            store
                .get_publication_ingestion_receipt_for_stage(&first_stage.stage_hash)
                .expect("receipt reads"),
            first_receipt
        );
        let mut conflicting_verification = verification.clone();
        conflicting_verification.raw_verification_hash = test_hash("different-raw-verification");
        let conflicting_receipt_byte_size = canonical_json(
            &serde_json::to_value(&conflicting_verification)
                .expect("conflicting verification serializes"),
        )
        .expect("conflicting verification canonicalizes")
        .len() as u64;
        let conflict = store
            .register_publication_ingestion_receipt(
                &first_stage.stage_hash,
                &current_subject,
                &conflicting_verification,
                124,
                conflicting_receipt_byte_size,
                "publication-test",
                "publication-receipt-conflict",
            )
            .expect_err("one stage cannot bind a different receipt");
        assert_eq!(conflict.code, "MCL_PUBLICATION_RECEIPT_CONFLICT");

        let hashes = store
            .all_registered_cas_hashes()
            .expect("registered CAS roots read");
        for expected in [
            canonical_artifact.artifact_hash.as_str(),
            stage.report_artifact_hash.as_str(),
            stage.retained_closure_artifact_hash.as_str(),
            stage.attestation_bundle_artifact_hash.as_str(),
            verification.raw_verification_hash.as_str(),
            first_receipt.receipt_hash.as_str(),
        ] {
            assert!(
                hashes.binary_search(&expected.to_owned()).is_ok(),
                "missing CAS root {expected}"
            );
        }
        for artifact in &stage.retained_artifacts {
            assert!(
                hashes.binary_search(&artifact.artifact_hash).is_ok(),
                "missing retained CAS root {}",
                artifact.artifact_hash
            );
        }

        assert!(
            store
                .connection
                .execute(
                    "UPDATE publication_stages SET created_by = 'rewritten' WHERE stage_hash = ?1",
                    [&first_stage.stage_hash],
                )
                .is_err(),
            "publication stage must be immutable"
        );
        assert!(
            store
                .connection
                .execute(
                    "DELETE FROM publication_ingestion_receipts WHERE receipt_hash = ?1",
                    [&first_receipt.receipt_hash],
                )
                .is_err(),
            "publication receipt must be immutable"
        );

        store
            .connection
            .execute_batch(
                "DROP TRIGGER publication_stages_reject_update;
                 DROP TRIGGER publication_ingestion_receipts_reject_update;",
            )
            .expect("test bypasses immutable triggers");
        store
            .connection
            .execute(
                "UPDATE publication_stages SET stage_json = ' ' || stage_json WHERE stage_hash = ?1",
                [&first_stage.stage_hash],
            )
            .expect("test injects noncanonical stage JSON");
        store
            .connection
            .execute(
                "UPDATE publication_ingestion_receipts SET verification_json = ' ' || verification_json WHERE receipt_hash = ?1",
                [&first_receipt.receipt_hash],
            )
            .expect("test injects noncanonical receipt JSON");
        assert_eq!(
            store
                .get_publication_stage(
                    &stage.report_artifact_hash,
                    &stage.attestation_bundle_artifact_hash,
                )
                .expect_err("noncanonical stage is rejected")
                .code,
            "MCL_PUBLICATION_STAGE_INTEGRITY_FAILED"
        );
        assert_eq!(
            store
                .get_publication_ingestion_receipt_for_stage(&first_stage.stage_hash)
                .expect_err("noncanonical receipt is rejected")
                .code,
            "MCL_PUBLICATION_RECEIPT_INTEGRITY_FAILED"
        );
    }

    #[test]
    fn publication_receipt_rejects_wrong_size_and_stage_binding() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let (current_subject, _) =
            current_publication_subject(&mut store, "publication-receipt-invalid");
        let stage = publication_stage_for_subject(
            &mut store,
            &current_subject,
            PublicationOutcome::Proof,
            "publication-receipt-invalid",
        );
        let snapshot = store
            .register_publication_stage(&stage, "publication-test", "stage")
            .expect("stage registers");
        let verification = publication_verification(&stage);
        let receipt_byte_size =
            canonical_json(&serde_json::to_value(&verification).expect("verification serializes"))
                .expect("verification canonicalizes")
                .len() as u64;

        let wrong_size = store
            .register_publication_ingestion_receipt(
                &snapshot.stage_hash,
                &current_subject,
                &verification,
                1,
                receipt_byte_size + 1,
                "publication-test",
                "wrong-size",
            )
            .expect_err("wrong canonical size fails");
        assert_eq!(wrong_size.code, "MCL_PUBLICATION_RECEIPT_INVALID");

        let mut wrong_bundle = verification;
        wrong_bundle.attestation_bundle_hash = test_hash("different-bundle");
        let wrong_bundle_size =
            canonical_json(&serde_json::to_value(&wrong_bundle).expect("verification serializes"))
                .expect("verification canonicalizes")
                .len() as u64;
        let wrong_binding = store
            .register_publication_ingestion_receipt(
                &snapshot.stage_hash,
                &current_subject,
                &wrong_bundle,
                1,
                wrong_bundle_size,
                "publication-test",
                "wrong-binding",
            )
            .expect_err("wrong stage binding fails");
        assert_eq!(
            wrong_binding.code,
            "MCL_PUBLICATION_RECEIPT_BINDING_INVALID"
        );
    }

    #[test]
    fn publication_receipt_retry_keys_and_current_subject_fail_closed() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let (current_subject, formalization_draft) =
            current_publication_subject(&mut store, "publication-receipt-guards");

        let first_stage = publication_stage_for_subject(
            &mut store,
            &current_subject,
            PublicationOutcome::Proof,
            "publication-receipt-guards-first",
        );
        let mut second_stage = publication_stage_for_subject(
            &mut store,
            &current_subject,
            PublicationOutcome::Proof,
            "publication-receipt-guards-second",
        );
        second_stage.report_artifact_hash = test_hash("publication-report-second");
        second_stage.attestation_bundle_artifact_hash =
            test_hash("publication-attestation-bundle-second");
        let first_stage = store
            .register_publication_stage(&first_stage, "publication-test", "guard-stage-first")
            .expect("first stage registers");
        let second_stage = store
            .register_publication_stage(&second_stage, "publication-test", "guard-stage-second")
            .expect("second stage registers");
        let first_verification = publication_verification(&first_stage.stage);
        let second_verification = publication_verification(&second_stage.stage);
        let receipt_size = |verification: &PublicationAttestationVerification| {
            canonical_json(&serde_json::to_value(verification).expect("verification serializes"))
                .expect("verification canonicalizes")
                .len() as u64
        };

        let first_receipt = store
            .register_publication_ingestion_receipt(
                &first_stage.stage_hash,
                &current_subject,
                &first_verification,
                123,
                receipt_size(&first_verification),
                "publication-test",
                "shared-publication-receipt-key",
            )
            .expect("first receipt registers");
        store
            .register_publication_ingestion_receipt(
                &second_stage.stage_hash,
                &current_subject,
                &second_verification,
                123,
                receipt_size(&second_verification),
                "publication-test",
                "second-publication-receipt-key",
            )
            .expect("second receipt registers");
        assert_eq!(
            store
                .publication_ingestion_idempotency_result(
                    &first_stage.stage_hash,
                    "publication-test",
                    "shared-publication-receipt-key",
                )
                .expect("exact receipt retry is found"),
            Some(first_receipt)
        );
        assert_eq!(
            store
                .publication_ingestion_idempotency_result(
                    &second_stage.stage_hash,
                    "publication-test",
                    "shared-publication-receipt-key",
                )
                .expect_err("a used key cannot be rebound during retry")
                .code,
            "MCL_IDEMPOTENCY_CONFLICT"
        );
        assert_eq!(
            store
                .register_publication_ingestion_receipt(
                    &second_stage.stage_hash,
                    &current_subject,
                    &second_verification,
                    123,
                    receipt_size(&second_verification),
                    "publication-test",
                    "shared-publication-receipt-key",
                )
                .expect_err("registration also rejects the rebound key")
                .code,
            "MCL_IDEMPOTENCY_CONFLICT"
        );

        let mut successor = formalization_draft;
        successor.payload["formalization_notes"] =
            json!("state changed after external attestation verification");
        store
            .version_record(
                &current_subject.object_id,
                &current_subject.version_hash,
                &successor,
                "publication-test",
                "publication-receipt-currentness-race",
            )
            .expect("formalization head advances");
        assert_eq!(
            store
                .register_publication_ingestion_receipt(
                    &first_stage.stage_hash,
                    &current_subject,
                    &first_verification,
                    123,
                    receipt_size(&first_verification),
                    "publication-test",
                    "publication-receipt-after-state-change",
                )
                .expect_err("receipt finalization rechecks currentness atomically")
                .code,
            "MCL_PUBLICATION_SUBJECT_STALE"
        );
    }

    #[test]
    fn publication_authority_evidence_is_receipt_bound_idempotent_and_immutable() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let fixture = publication_authority_fixture(
            &mut store,
            "publication-authority-proof",
            PublicationOutcome::Proof,
        );
        let expected_payload = fixture
            .commit
            .evidence_payload()
            .expect("authority payload derives");
        assert_eq!(
            expected_payload.evidence_kind,
            EvidenceKind::LeanKernelProof
        );
        assert_eq!(expected_payload.result, EvidenceResult::Accepted);
        assert_eq!(
            expected_payload.authority_class,
            EvidenceAuthorityClass::Authoritative
        );
        assert_eq!(
            expected_payload.verifier_or_reviewer_identity,
            format!(
                "publication-policy:{}",
                fixture.commit.binding.publication_policy_hash
            )
        );

        let first = store
            .create_publication_authority_evidence(
                &fixture.commit,
                "publication-test",
                "publication-authority-create",
            )
            .expect("authority evidence creates");
        assert_eq!(first.payload, expected_payload);
        assert_eq!(
            store
                .get_publication_stage_by_hash(&fixture.stage.stage_hash)
                .expect("stage reads by hash"),
            fixture.stage
        );
        assert_eq!(
            store
                .get_publication_ingestion_receipt(&fixture.receipt.receipt_hash)
                .expect("receipt reads by hash"),
            fixture.receipt
        );
        let projections = store
            .connection
            .query_row(
                "SELECT artifact_hash, run_id, job_id, publication_receipt_hash, publication_stage_hash FROM evidence WHERE evidence_id = ?1",
                [&first.evidence_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .expect("authority projections read");
        assert_eq!(
            projections,
            (
                None,
                None,
                None,
                Some(fixture.receipt.receipt_hash.clone()),
                Some(fixture.stage.stage_hash.clone())
            )
        );
        assert_eq!(
            store
                .create_publication_authority_evidence(
                    &fixture.commit,
                    "publication-test",
                    "publication-authority-create",
                )
                .expect("exact retry succeeds"),
            first
        );
        assert_eq!(
            store
                .create_publication_authority_evidence(
                    &fixture.commit,
                    "publication-test",
                    "publication-authority-another-key",
                )
                .expect_err("one receipt cannot create authority twice")
                .code,
            "MCL_PUBLICATION_AUTHORITY_EXISTS"
        );
        assert!(
            store
                .connection
                .execute(
                    "UPDATE evidence SET created_by = 'rewritten' WHERE evidence_id = ?1",
                    [&first.evidence_id],
                )
                .is_err(),
            "authority evidence must be immutable"
        );
        assert!(
            store
                .connection
                .execute(
                    "DELETE FROM evidence WHERE evidence_id = ?1",
                    [&first.evidence_id],
                )
                .is_err(),
            "authority evidence must be durable"
        );

        drop(store);
        let mut store = Store::open(&database).expect("database reopens");
        store.migrate().expect("migration remains idempotent");
        assert_eq!(
            store
                .create_publication_authority_evidence(
                    &fixture.commit,
                    "publication-test",
                    "publication-authority-create",
                )
                .expect("restart retry succeeds"),
            first
        );
    }

    #[test]
    fn publication_authority_maps_refutation_and_rejects_substitution() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let fixture = publication_authority_fixture(
            &mut store,
            "publication-authority-refutation",
            PublicationOutcome::Refutation,
        );
        assert_eq!(
            fixture
                .commit
                .evidence_payload()
                .expect("refutation payload derives")
                .evidence_kind,
            EvidenceKind::LeanKernelRefutation
        );

        let mut incomplete = fixture.commit.clone();
        incomplete.artifact_hashes.pop();
        assert_eq!(
            store
                .create_publication_authority_evidence(
                    &incomplete,
                    "publication-test",
                    "publication-authority-incomplete",
                )
                .expect_err("incomplete retained closure fails")
                .code,
            "MCL_PUBLICATION_AUTHORITY_ARTIFACTS_INVALID"
        );

        let mut substituted_policy = fixture.commit.clone();
        substituted_policy.binding.publication_policy_hash = test_hash("substituted-policy");
        assert_eq!(
            store
                .create_publication_authority_evidence(
                    &substituted_policy,
                    "publication-test",
                    "publication-authority-substituted-policy",
                )
                .expect_err("substituted policy fails")
                .code,
            "MCL_PUBLICATION_AUTHORITY_BINDING_INVALID"
        );

        let mut wrong_outcome = fixture.commit.clone();
        wrong_outcome.outcome = PublicationOutcome::Proof;
        assert_eq!(
            store
                .create_publication_authority_evidence(
                    &wrong_outcome,
                    "publication-test",
                    "publication-authority-wrong-outcome",
                )
                .expect_err("claim polarity controls the outcome")
                .code,
            "MCL_PUBLICATION_OUTCOME_MISMATCH"
        );

        let created = store
            .create_publication_authority_evidence(
                &fixture.commit,
                "publication-test",
                "publication-authority-refutation-create",
            )
            .expect("refutation authority creates");
        assert_eq!(
            created.payload.evidence_kind,
            EvidenceKind::LeanKernelRefutation
        );
        assert_eq!(
            store
                .create_publication_authority_evidence(
                    &fixture.commit,
                    "different-publication-actor",
                    "publication-authority-refutation-create",
                )
                .expect_err("an idempotency key cannot change actor")
                .code,
            "MCL_IDEMPOTENCY_CONFLICT"
        );
    }

    #[test]
    fn publication_authority_rejects_same_environment_subject_substitution() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let fixture = publication_authority_fixture(
            &mut store,
            "publication-authority-subject-a",
            PublicationOutcome::Proof,
        );
        let shared_environment = fixture.formalization_draft.payload["environment_hash"]
            .as_str()
            .expect("fixture environment")
            .to_owned();
        let (substituted_subject, substituted_draft) =
            current_publication_subject_for_outcome_in_environment(
                &mut store,
                "publication-authority-subject-b",
                PublicationOutcome::Proof,
                &shared_environment,
            );
        assert_ne!(fixture.commit.subject, substituted_subject);
        assert_eq!(
            fixture.formalization_draft.payload["environment_hash"],
            substituted_draft.payload["environment_hash"],
            "the adversarial subjects deliberately share one environment"
        );
        assert_eq!(
            fixture.formalization_draft.payload["claim_polarity"],
            substituted_draft.payload["claim_polarity"],
            "the adversarial subjects deliberately share one polarity"
        );

        assert_eq!(
            store
                .register_publication_ingestion_receipt(
                    &fixture.stage.stage_hash,
                    &substituted_subject,
                    &fixture.receipt.verification,
                    fixture.receipt.raw_verification_byte_size,
                    fixture.receipt.receipt_byte_size,
                    "publication-test",
                    "publication-authority-subject-a-receipt",
                )
                .expect_err("receipt retry cannot substitute its request subject")
                .code,
            "MCL_PUBLICATION_RECEIPT_SUBJECT_MISMATCH"
        );

        let mut substituted_commit = fixture.commit.clone();
        substituted_commit.subject = substituted_subject.clone();
        assert_eq!(
            store
                .create_publication_authority_evidence(
                    &substituted_commit,
                    "publication-test",
                    "publication-authority-substituted-subject",
                )
                .expect_err("same-environment formalization substitution fails")
                .code,
            "MCL_PUBLICATION_AUTHORITY_SUBJECT_MISMATCH"
        );

        let substituted_payload = substituted_commit
            .evidence_payload()
            .expect("substituted payload remains structurally valid");
        assert!(
            raw_insert_publication_authority_evidence(
                &store,
                &substituted_payload,
                Some(&fixture.receipt.receipt_hash),
                Some(&fixture.stage.stage_hash),
            )
            .is_err(),
            "the SQL gate must bind evidence to the receipt and retained request subject"
        );

        let evidence = store
            .create_publication_authority_evidence(
                &fixture.commit,
                "publication-test",
                "publication-authority-exact-subject",
            )
            .expect("the exact retained request subject creates authority");
        store
            .connection
            .execute_batch("DROP TRIGGER publication_ingestion_receipts_reject_update;")
            .expect("test bypasses receipt immutability");
        store
            .connection
            .execute(
                "UPDATE publication_ingestion_receipts SET subject_object_id = ?1, subject_version_hash = ?2 WHERE receipt_hash = ?3",
                params![
                    substituted_subject.object_id,
                    substituted_subject.version_hash,
                    fixture.receipt.receipt_hash,
                ],
            )
            .expect("test corrupts the receipt subject binding");
        assert_eq!(
            store
                .get_evidence(&evidence.evidence_id)
                .expect_err("read-time validation rejects subject rebinding")
                .code,
            "MCL_PUBLICATION_AUTHORITY_INTEGRITY_FAILED"
        );
    }

    #[test]
    fn publication_authority_rechecks_subject_head_inside_the_transaction() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let fixture = publication_authority_fixture(
            &mut store,
            "publication-authority-stale",
            PublicationOutcome::Proof,
        );
        let mut successor = fixture.formalization_draft.clone();
        successor.payload["formalization_notes"] =
            json!("state changed after publication receipt ingestion");
        store
            .version_record(
                &fixture.commit.subject.object_id,
                &fixture.commit.subject.version_hash,
                &successor,
                "publication-test",
                "publication-authority-successor",
            )
            .expect("formalization head advances");
        assert_eq!(
            store
                .create_publication_authority_evidence(
                    &fixture.commit,
                    "publication-test",
                    "publication-authority-after-head-change",
                )
                .expect_err("stale publication cannot grant authority")
                .code,
            "MCL_PUBLICATION_SUBJECT_STALE"
        );
    }

    #[test]
    fn publication_authority_insert_trigger_rejects_forged_gate_inputs() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let fixture = publication_authority_fixture(
            &mut store,
            "publication-authority-trigger",
            PublicationOutcome::Proof,
        );
        let payload = fixture
            .commit
            .evidence_payload()
            .expect("authority payload derives");

        assert!(
            raw_insert_publication_authority_evidence(&store, &payload, None, None).is_err(),
            "authority evidence without receipt projections must fail"
        );

        let metadata = serde_json::to_value(&payload).expect("authority payload serializes");
        assert!(
            raw_insert_publication_authority_metadata_as_actor(
                &store,
                &payload,
                &metadata,
                Some(&fixture.receipt.receipt_hash),
                Some(&fixture.stage.stage_hash),
                "   ",
            )
            .is_err(),
            "authority evidence requires nonblank actor attribution"
        );
        let mut unknown_top_level = metadata.clone();
        unknown_top_level
            .as_object_mut()
            .expect("payload object")
            .insert("caller_override".to_owned(), json!(true));
        assert!(
            raw_insert_publication_authority_metadata(
                &store,
                &payload,
                &unknown_top_level,
                Some(&fixture.receipt.receipt_hash),
                Some(&fixture.stage.stage_hash),
            )
            .is_err(),
            "authority metadata cannot add a top-level key"
        );
        let mut unknown_subject = metadata.clone();
        unknown_subject["subject"]
            .as_object_mut()
            .expect("subject object")
            .insert("caller_override".to_owned(), json!(true));
        assert!(
            raw_insert_publication_authority_metadata(
                &store,
                &payload,
                &unknown_subject,
                Some(&fixture.receipt.receipt_hash),
                Some(&fixture.stage.stage_hash),
            )
            .is_err(),
            "authority subject cannot add an unknown key"
        );
        let mut unknown_binding = metadata.clone();
        unknown_binding["publication_authority"]
            .as_object_mut()
            .expect("binding object")
            .insert("caller_override".to_owned(), json!(true));
        assert!(
            raw_insert_publication_authority_metadata(
                &store,
                &payload,
                &unknown_binding,
                Some(&fixture.receipt.receipt_hash),
                Some(&fixture.stage.stage_hash),
            )
            .is_err(),
            "authority binding cannot add an unknown key"
        );
        let mut mismatched_verifier = metadata.clone();
        mismatched_verifier["verifier_or_reviewer_identity"] = json!("caller:authority");
        assert!(
            raw_insert_publication_authority_metadata(
                &store,
                &payload,
                &mismatched_verifier,
                Some(&fixture.receipt.receipt_hash),
                Some(&fixture.stage.stage_hash),
            )
            .is_err(),
            "authority verifier metadata must match its SQL projection"
        );
        let mut unsorted_artifacts = metadata.clone();
        unsorted_artifacts["artifact_hashes"]
            .as_array_mut()
            .expect("artifact array")
            .swap(0, 1);
        assert!(
            raw_insert_publication_authority_metadata(
                &store,
                &payload,
                &unsorted_artifacts,
                Some(&fixture.receipt.receipt_hash),
                Some(&fixture.stage.stage_hash),
            )
            .is_err(),
            "authority artifact projections must remain strictly sorted"
        );

        let mut substituted_policy = payload.clone();
        let substituted_hash = test_hash("trigger-substituted-policy");
        substituted_policy
            .publication_authority
            .as_mut()
            .expect("authority binding")
            .publication_policy_hash = substituted_hash.clone();
        substituted_policy.verifier_or_reviewer_identity =
            format!("publication-policy:{substituted_hash}");
        assert!(
            raw_insert_publication_authority_evidence(
                &store,
                &substituted_policy,
                Some(&fixture.receipt.receipt_hash),
                Some(&fixture.stage.stage_hash),
            )
            .is_err(),
            "authority evidence cannot substitute the staged policy"
        );

        let mut incomplete = payload;
        incomplete.artifact_hashes.pop();
        assert!(
            raw_insert_publication_authority_evidence(
                &store,
                &incomplete,
                Some(&fixture.receipt.receipt_hash),
                Some(&fixture.stage.stage_hash),
            )
            .is_err(),
            "authority evidence cannot omit a retained closure artifact"
        );

        store
            .create_publication_authority_evidence(
                &fixture.commit,
                "publication-test",
                "publication-authority-trigger-valid",
            )
            .expect("the exact Store path passes the closed insert gate");
    }

    #[test]
    fn publication_authority_reads_fail_closed_on_projection_tampering() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let fixture = publication_authority_fixture(
            &mut store,
            "publication-authority-tamper",
            PublicationOutcome::Proof,
        );
        let evidence = store
            .create_publication_authority_evidence(
                &fixture.commit,
                "publication-test",
                "publication-authority-before-tamper",
            )
            .expect("authority evidence creates");
        store
            .connection
            .execute_batch("DROP TRIGGER evidence_reject_update;")
            .expect("test bypasses evidence immutability");
        store
            .connection
            .execute(
                "UPDATE evidence SET publication_stage_hash = NULL WHERE evidence_id = ?1",
                [&evidence.evidence_id],
            )
            .expect("test corrupts the stage projection");
        assert_eq!(
            store
                .get_evidence(&evidence.evidence_id)
                .expect_err("projection corruption fails closed")
                .code,
            "MCL_EVIDENCE_INTEGRITY_FAILED"
        );
    }

    struct PublicationAuthorityFixture {
        commit: PublicationAuthorityCommit,
        stage: PublicationStageSnapshot,
        receipt: PublicationIngestionReceiptSnapshot,
        formalization_draft: RecordDraft,
    }

    fn publication_authority_fixture(
        store: &mut Store,
        label: &str,
        outcome: PublicationOutcome,
    ) -> PublicationAuthorityFixture {
        let (subject, formalization_draft) =
            current_publication_subject_for_outcome(store, label, outcome);
        let stage = publication_stage_for_subject(store, &subject, outcome, label);
        let stage = store
            .register_publication_stage(&stage, "publication-test", &format!("{label}-stage"))
            .expect("authority stage registers");
        let mut verification = publication_verification(&stage.stage);
        verification.raw_verification_hash = test_hash(&format!("{label}-raw-verification"));
        let receipt_byte_size =
            canonical_json(&serde_json::to_value(&verification).expect("verification serializes"))
                .expect("verification canonicalizes")
                .len() as u64;
        let receipt = store
            .register_publication_ingestion_receipt(
                &stage.stage_hash,
                &subject,
                &verification,
                123,
                receipt_byte_size,
                "publication-test",
                &format!("{label}-receipt"),
            )
            .expect("authority receipt registers");
        let binding = PublicationAuthorityBinding {
            schema_version: crate::domain::evidence::PUBLICATION_AUTHORITY_BINDING_SCHEMA_VERSION
                .to_owned(),
            ingestion_receipt_hash: receipt.receipt_hash.clone(),
            stage_hash: stage.stage_hash.clone(),
            report_artifact_hash: stage.stage.report_artifact_hash.clone(),
            retained_closure_artifact_hash: stage.stage.retained_closure_artifact_hash.clone(),
            attestation_bundle_artifact_hash: stage.stage.attestation_bundle_artifact_hash.clone(),
            raw_verification_hash: receipt.verification.raw_verification_hash.clone(),
            publication_request_hash: publication_stage_role_artifact_hash(
                &stage,
                PublicationRetainedArtifactRole::PublicationRequest,
            )
            .expect("stage has publication request")
            .to_owned(),
            publication_policy_hash: publication_stage_role_artifact_hash(
                &stage,
                PublicationRetainedArtifactRole::PublicationPolicy,
            )
            .expect("stage has publication policy")
            .to_owned(),
        };
        let commit = PublicationAuthorityCommit {
            subject,
            outcome,
            environment_hash: formalization_draft.payload["environment_hash"]
                .as_str()
                .expect("formalization environment")
                .to_owned(),
            binding,
            artifact_hashes: publication_authority_artifact_hashes(&stage, &receipt),
        };
        PublicationAuthorityFixture {
            commit,
            stage,
            receipt,
            formalization_draft,
        }
    }

    fn raw_insert_publication_authority_evidence(
        store: &Store,
        payload: &EvidencePayload,
        publication_receipt_hash: Option<&str>,
        publication_stage_hash: Option<&str>,
    ) -> rusqlite::Result<usize> {
        raw_insert_publication_authority_metadata(
            store,
            payload,
            &serde_json::to_value(payload).expect("authority payload serializes"),
            publication_receipt_hash,
            publication_stage_hash,
        )
    }

    fn raw_insert_publication_authority_metadata(
        store: &Store,
        payload: &EvidencePayload,
        metadata: &Value,
        publication_receipt_hash: Option<&str>,
        publication_stage_hash: Option<&str>,
    ) -> rusqlite::Result<usize> {
        raw_insert_publication_authority_metadata_as_actor(
            store,
            payload,
            metadata,
            publication_receipt_hash,
            publication_stage_hash,
            "publication-test",
        )
    }

    fn raw_insert_publication_authority_metadata_as_actor(
        store: &Store,
        payload: &EvidencePayload,
        metadata: &Value,
        publication_receipt_hash: Option<&str>,
        publication_stage_hash: Option<&str>,
        actor: &str,
    ) -> rusqlite::Result<usize> {
        let payload_json = canonical_string(metadata).expect("authority payload canonicalizes");
        let evidence_hash = test_hash(&payload_json);
        let artifact_hashes_json = serde_json::to_string(
            metadata["artifact_hashes"]
                .as_array()
                .expect("authority artifact array"),
        )
        .expect("artifacts serialize");
        store.connection.execute(
            "INSERT INTO evidence(evidence_id, subject_object_id, subject_version_hash, evidence_kind, result, authority_class, run_id, environment_hash, artifact_hash, metadata_json, created_at, superseded_by, evidence_hash, job_id, artifact_hashes_json, verifier_identity, created_by, stale_reason, publication_receipt_hash, publication_stage_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, NULL, ?8, unixepoch(), NULL, ?9, NULL, ?10, ?11, ?12, NULL, ?13, ?14)",
            params![
                Uuid::now_v7().to_string(),
                payload.subject.object_id,
                payload.subject.version_hash,
                payload.evidence_kind.as_str(),
                payload.result.as_str(),
                payload.authority_class.as_str(),
                payload.environment_hash,
                payload_json,
                evidence_hash,
                artifact_hashes_json,
                payload.verifier_or_reviewer_identity,
                actor,
                publication_receipt_hash,
                publication_stage_hash,
            ],
        )
    }

    fn publication_stage_for_subject(
        store: &mut Store,
        subject: &ExactVersionReference,
        outcome: PublicationOutcome,
        label: &str,
    ) -> PublicationStage {
        let formalization = store
            .get_record_version(&subject.version_hash)
            .expect("publication subject reads");
        let formalization: FormalizationPayload =
            serde_json::from_value(formalization.payload).expect("formalization decodes");
        let source_commit_sha = test_hash(&format!("{label}-source-commit"))[..40].to_owned();
        let source_tree_sha = test_hash(&format!("{label}-source-tree"))[..40].to_owned();
        let request = PublicationRequest {
            schema_version: crate::domain::publication::PUBLICATION_REQUEST_SCHEMA_VERSION
                .to_owned(),
            subject: subject.clone(),
            outcome,
            diagnostic_evidence_id: Uuid::now_v7().to_string(),
            diagnostic_evidence_hash: test_hash(&format!("{label}-diagnostic")),
            proof_closure_evidence_id: Uuid::now_v7().to_string(),
            proof_closure_evidence_hash: test_hash(&format!("{label}-proof-closure")),
            axiom_audit_evidence_id: Uuid::now_v7().to_string(),
            axiom_audit_evidence_hash: test_hash(&format!("{label}-axiom-audit")),
            environment_hash: formalization.environment_hash,
            module_artifact_hash: formalization.module_artifact_hash,
            declaration_name: formalization.declaration_name,
            policy_hash: test_hash(&format!("{label}-policy")),
            source_commit_sha: source_commit_sha.clone(),
            source_tree_sha: source_tree_sha.clone(),
        };
        let request_bytes =
            canonical_json(&serde_json::to_value(&request).expect("request serializes"))
                .expect("request canonicalizes");
        let request_hash = request.request_hash().expect("request hashes");
        assert_eq!(
            request_hash,
            test_hash(&String::from_utf8_lossy(&request_bytes))
        );
        let metadata = ArtifactMetadata {
            schema_version: crate::domain::artifact::ARTIFACT_METADATA_SCHEMA_VERSION.to_owned(),
            media_type: ArtifactMediaType::Json,
            creation_source: ArtifactCreationSource::Generated,
            license_expression: None,
            restriction: ArtifactRestriction::Private,
            semantic_metadata: BTreeMap::from([
                ("artifact_role".to_owned(), "publication_request".to_owned()),
                ("request_hash".to_owned(), request_hash.clone()),
                (
                    "formalization_object_id".to_owned(),
                    subject.object_id.clone(),
                ),
                (
                    "formalization_version_hash".to_owned(),
                    subject.version_hash.clone(),
                ),
                ("source_commit_sha".to_owned(), source_commit_sha),
                ("source_tree_sha".to_owned(), source_tree_sha),
            ]),
        };
        store
            .register_publication_request_artifact(
                &request_hash,
                request_bytes.len() as u64,
                &metadata,
                &request,
                "publication-test",
                &format!("{label}-request-artifact"),
            )
            .expect("publication request artifact registers");

        let mut stage = publication_stage();
        stage.report_artifact_hash = test_hash(&format!("{label}-report"));
        stage.retained_closure_artifact_hash = test_hash(&format!("{label}-closure"));
        stage.attestation_bundle_artifact_hash = test_hash(&format!("{label}-bundle"));
        for (index, artifact) in stage.retained_artifacts.iter_mut().enumerate() {
            artifact.identity_hash = test_hash(&format!("{label}-identity-{index}"));
            artifact.artifact_hash = test_hash(&format!("{label}-artifact-{index}"));
        }
        let retained_request = stage
            .retained_artifacts
            .iter_mut()
            .find(|artifact| artifact.role == PublicationRetainedArtifactRole::PublicationRequest)
            .expect("stage request role");
        retained_request.identity_hash = request_hash.clone();
        retained_request.artifact_hash = request_hash;
        retained_request.byte_size = request_bytes.len() as u64;
        stage
    }

    fn publication_stage() -> PublicationStage {
        PublicationStage {
            schema_version: crate::domain::publication::PUBLICATION_STAGE_SCHEMA_VERSION.to_owned(),
            report_artifact_hash: test_hash("publication-report"),
            report_byte_size: 1_024,
            retained_closure_artifact_hash: test_hash("publication-retained-closure"),
            retained_closure_byte_size: 2_048,
            attestation_bundle_artifact_hash: test_hash("publication-attestation-bundle"),
            attestation_bundle_byte_size: 4_096,
            retained_artifacts: PublicationRetainedArtifactRole::ALL
                .into_iter()
                .enumerate()
                .map(|(index, role)| PublicationStageArtifact {
                    role,
                    path: role.expected_path().to_owned(),
                    identity_hash: test_hash(&format!("identity-{index}")),
                    artifact_hash: test_hash(&format!("artifact-{index}")),
                    byte_size: index as u64,
                })
                .collect(),
            authoritative: false,
        }
    }

    fn publication_verification(stage: &PublicationStage) -> PublicationAttestationVerification {
        let policy = crate::domain::publication::committed_publication_policy()
            .expect("committed publication policy");
        PublicationAttestationVerification {
            schema_version:
                crate::domain::publication::PUBLICATION_ATTESTATION_VERIFICATION_SCHEMA_VERSION
                    .to_owned(),
            report_content_hash: stage.report_artifact_hash.clone(),
            report_artifact_hash: stage.report_artifact_hash.clone(),
            attestation_bundle_hash: stage.attestation_bundle_artifact_hash.clone(),
            raw_verification_hash: test_hash("raw-attestation-verification"),
            verifier_name: "gh".to_owned(),
            verifier_version: policy.attestation_verifier_version,
            verifier_binary_sha256: policy.attestation_verifier_binary_sha256,
            repository: policy.repository.clone(),
            signer_workflow: format!("{}/{}", policy.repository, policy.workflow_path),
            certificate_identity: format!(
                "https://github.com/{}/{}@{}",
                policy.repository, policy.workflow_path, policy.required_source_ref
            ),
            source_ref: policy.required_source_ref,
            source_commit_sha: "a".repeat(40),
            predicate_type: policy.attestation_predicate_type,
            self_hosted_runners_denied: true,
            verified_attestation_count: 1,
            verified_timestamp_count: 1,
            authoritative: false,
        }
    }

    fn current_publication_subject(
        store: &mut Store,
        label: &str,
    ) -> (ExactVersionReference, RecordDraft) {
        current_publication_subject_for_outcome(store, label, PublicationOutcome::Proof)
    }

    fn current_publication_subject_for_outcome(
        store: &mut Store,
        label: &str,
        outcome: PublicationOutcome,
    ) -> (ExactVersionReference, RecordDraft) {
        let environment = store
            .register_environment(
                &environment_manifest(),
                "publication-test",
                &format!("{label}-environment"),
            )
            .expect("publication receipt environment registers");
        current_publication_subject_for_outcome_in_environment(
            store,
            label,
            outcome,
            &environment.environment_hash,
        )
    }

    fn current_publication_subject_for_outcome_in_environment(
        store: &mut Store,
        label: &str,
        outcome: PublicationOutcome,
        environment_hash: &str,
    ) -> (ExactVersionReference, RecordDraft) {
        let source_snapshot = store
            .create_record(
                &source(&format!("Publication receipt source fixture {label}")),
                "publication-test",
                &format!("{label}-source"),
            )
            .expect("publication receipt source creates");
        let mut claim_draft = claim(&format!("Publication receipt fixture {label}"));
        claim_draft.payload["source_reference"] = json!({
            "object_id": source_snapshot.object_id,
            "version_hash": source_snapshot.version_hash,
        });
        let claim_snapshot = store
            .create_record(&claim_draft, "publication-test", &format!("{label}-claim"))
            .expect("publication receipt claim creates");
        let module_source =
            format!("theorem publicationReceiptFixture : True := by trivial\n-- {label}\n");
        let module =
            register_lean_artifact(store, module_source.as_bytes(), &format!("{label}-module"));
        let mut draft = formalization(
            &claim_snapshot,
            "True",
            environment_hash,
            &module.artifact_hash,
            &[],
        );
        draft.payload["claim_polarity"] = json!(match outcome {
            PublicationOutcome::Proof => "claim",
            PublicationOutcome::Refutation => "negation",
        });
        draft.payload["declaration_name"] = json!("MathOS.publicationReceiptFixture");
        let snapshot = store
            .create_record(
                &draft,
                "publication-test",
                &format!("{label}-formalization"),
            )
            .expect("publication receipt formalization creates");
        (
            ExactVersionReference {
                object_id: snapshot.object_id,
                version_hash: snapshot.version_hash,
            },
            draft,
        )
    }

    fn test_hash(value: &str) -> String {
        format!("{:x}", Sha256::digest(value.as_bytes()))
    }

    fn environment_manifest() -> EnvironmentManifest {
        serde_json::from_str(include_str!(
            "../../fixtures/environment/lean-4.32-local.json"
        ))
        .expect("environment fixture")
    }

    fn lean_artifact_metadata() -> ArtifactMetadata {
        ArtifactMetadata {
            schema_version: "artifact_metadata/1".to_owned(),
            media_type: crate::domain::ArtifactMediaType::LeanSource,
            creation_source: crate::domain::ArtifactCreationSource::UserIngest,
            license_expression: Some("PolyForm-Noncommercial-1.0.0".to_owned()),
            restriction: crate::domain::ArtifactRestriction::Restricted,
            semantic_metadata: BTreeMap::new(),
        }
    }

    fn register_lean_artifact(
        store: &mut Store,
        bytes: &[u8],
        idempotency_key: &str,
    ) -> ArtifactSnapshot {
        let hash = format!("{:x}", Sha256::digest(bytes));
        store
            .register_artifact(
                &hash,
                bytes.len() as u64,
                &lean_artifact_metadata(),
                "artifact-author",
                idempotency_key,
            )
            .expect("Lean artifact registers")
    }

    #[test]
    fn artifact_metadata_is_immutable_idempotent_and_exact_after_restart() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let bytes = b"theorem truth : True := by trivial\n";
        let first = register_lean_artifact(&mut store, bytes, "artifact-register");
        assert_eq!(first.created_by, "artifact-author");
        assert_eq!(first.byte_size, bytes.len() as u64);
        assert_eq!(store.artifact_count().expect("artifact count"), 1);
        assert_eq!(
            store
                .register_artifact(
                    &first.artifact_hash,
                    bytes.len() as u64,
                    &lean_artifact_metadata(),
                    "artifact-author",
                    "artifact-register",
                )
                .expect("exact retry"),
            first
        );
        assert_eq!(
            store
                .register_artifact(
                    &first.artifact_hash,
                    bytes.len() as u64,
                    &lean_artifact_metadata(),
                    "artifact-author",
                    "artifact-duplicate",
                )
                .expect_err("duplicate artifact")
                .code,
            "MCL_ARTIFACT_EXISTS"
        );
        assert!(
            store
                .connection
                .execute(
                    "UPDATE artifacts SET byte_size = byte_size + 1 WHERE artifact_hash = ?1",
                    [&first.artifact_hash],
                )
                .is_err()
        );
        assert!(
            store
                .connection
                .execute(
                    "DELETE FROM artifacts WHERE artifact_hash = ?1",
                    [&first.artifact_hash],
                )
                .is_err()
        );
        drop(store);
        let restarted = Store::open(&database).expect("database reopens");
        assert_eq!(
            restarted
                .get_artifact(&first.artifact_hash)
                .expect("artifact metadata survives restart"),
            first
        );
    }

    #[test]
    fn artifact_metadata_corruption_is_detected() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let artifact = register_lean_artifact(&mut store, b"def one := 1\n", "artifact-corrupt");
        store
            .connection
            .execute("DROP TRIGGER artifacts_reject_update", [])
            .expect("test removes artifact mutation guard");
        store
            .connection
            .execute(
                "UPDATE artifacts SET media_type = 'application/json' WHERE artifact_hash = ?1",
                [&artifact.artifact_hash],
            )
            .expect("test simulates metadata corruption");
        assert_eq!(
            store
                .get_artifact(&artifact.artifact_hash)
                .expect_err("corruption detected")
                .code,
            "MCL_ARTIFACT_INTEGRITY_FAILED"
        );
    }

    fn verifier_request(environment_hash: &str, artifact_hash: &str) -> VerifierJobRequest {
        VerifierJobRequest {
            schema_version: "verifier_request/1".to_owned(),
            environment_hash: environment_hash.to_owned(),
            module_artifact_hash: artifact_hash.to_owned(),
            declaration_name: "MathOS.Verifier.fixture".to_owned(),
        }
    }

    #[test]
    fn verifier_jobs_are_idempotent_leased_and_recovered_after_expiry() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let environment = store
            .register_environment(
                &environment_manifest(),
                "environment-author",
                "job-environment",
            )
            .expect("environment registers");
        let artifact = register_lean_artifact(
            &mut store,
            b"theorem fixture : True := by trivial\n",
            "job-artifact",
        );
        let request = verifier_request(&environment.environment_hash, &artifact.artifact_hash);
        let queued = store
            .enqueue_verifier_job(&request, 10, "job-author", "job-enqueue")
            .expect("job enqueues");
        assert_eq!(queued.state, VerifierJobState::Queued);
        assert_eq!(queued.attempt_count, 0);
        assert_eq!(
            store
                .enqueue_verifier_job(&request, 10, "job-author", "job-enqueue")
                .expect("exact retry"),
            queued
        );
        let mut changed_request = request.clone();
        changed_request.declaration_name = "MathOS.Verifier.changed".to_owned();
        assert_eq!(
            store
                .enqueue_verifier_job(&changed_request, 10, "job-author", "job-enqueue")
                .expect_err("changed idempotent retry rejected")
                .code,
            "MCL_IDEMPOTENCY_CONFLICT"
        );
        assert_eq!(store.list_verifier_jobs(10).expect("job list").len(), 1);

        let leased = store
            .lease_next_verifier_job("worker-1", 60)
            .expect("lease succeeds")
            .expect("queued job exists");
        assert_eq!(leased.job_id, queued.job_id);
        assert_eq!(leased.state, VerifierJobState::Leased);
        assert_eq!(leased.attempt_count, 1);
        assert!(
            store
                .lease_next_verifier_job("worker-2", 60)
                .expect("empty lease succeeds")
                .is_none()
        );
        let running = store
            .mark_verifier_job_running(&queued.job_id, "worker-1")
            .expect("leased job starts");
        assert_eq!(running.state, VerifierJobState::Running);
        assert_eq!(
            store
                .mark_verifier_job_running(&queued.job_id, "worker-2")
                .expect_err("wrong worker rejected")
                .code,
            "MCL_VERIFIER_JOB_CONFLICT"
        );

        store
            .connection
            .execute(
                "UPDATE jobs SET lease_expires_at = unixepoch() - 1 WHERE job_id = ?1",
                [&queued.job_id],
            )
            .expect("test expires lease");
        assert_eq!(store.requeue_expired_verifier_jobs().expect("requeue"), 1);
        let requeued = store
            .get_verifier_job(&queued.job_id)
            .expect("requeued job");
        assert_eq!(requeued.state, VerifierJobState::Queued);
        assert_eq!(requeued.lease_owner, None);
        assert_eq!(requeued.attempt_count, 1);

        drop(store);
        let restarted = Store::open(&database).expect("database reopens");
        assert_eq!(
            restarted
                .get_verifier_job(&queued.job_id)
                .expect("job survives restart"),
            requeued
        );
    }

    #[test]
    fn verifier_job_references_state_and_stored_input_fail_closed() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let environment = store
            .register_environment(
                &environment_manifest(),
                "environment-author",
                "job-guard-environment",
            )
            .expect("environment registers");
        let artifact = register_lean_artifact(
            &mut store,
            b"theorem guarded : True := by trivial\n",
            "job-guard-artifact",
        );
        assert_eq!(
            store
                .enqueue_verifier_job(
                    &verifier_request(&"f".repeat(64), &artifact.artifact_hash),
                    0,
                    "job-author",
                    "job-missing-environment",
                )
                .expect_err("missing environment")
                .code,
            "MCL_VERIFIER_ENVIRONMENT_INVALID"
        );
        let request = verifier_request(&environment.environment_hash, &artifact.artifact_hash);
        let job = store
            .enqueue_verifier_job(&request, 0, "job-author", "job-guard")
            .expect("job enqueues");
        assert!(
            store
                .connection
                .execute(
                    "UPDATE jobs SET state = 'succeeded' WHERE job_id = ?1",
                    [&job.job_id],
                )
                .is_err()
        );
        assert!(
            store
                .connection
                .execute("DELETE FROM jobs WHERE job_id = ?1", [&job.job_id])
                .is_err()
        );
        let terminal = store
            .enqueue_verifier_job(&request, 10, "job-author", "job-terminal")
            .expect("terminal test job enqueues");
        let terminal_id = terminal.job_id;
        let leased_terminal = store
            .lease_next_verifier_job("terminal-worker", 60)
            .expect("terminal test lease")
            .expect("terminal job selected");
        assert_eq!(leased_terminal.job_id, terminal_id);
        store
            .mark_verifier_job_running(&terminal_id, "terminal-worker")
            .expect("terminal job starts");
        store
            .connection
            .execute(
                "UPDATE jobs SET state = 'succeeded', lease_owner = NULL, lease_expires_at = NULL, result_artifact_hash = ?2 WHERE job_id = ?1",
                params![terminal_id, artifact.artifact_hash],
            )
            .expect("valid terminal transition");
        assert!(
            store
                .connection
                .execute(
                    "UPDATE jobs SET progress_json = '{\"forged\":true}' WHERE job_id = ?1",
                    [&terminal_id],
                )
                .is_err()
        );

        store
            .connection
            .execute("DROP TRIGGER jobs_reject_identity_rewrite", [])
            .expect("test removes identity trigger");
        store
            .connection
            .execute(
                "UPDATE jobs SET input_json = '{}' WHERE job_id = ?1",
                [&job.job_id],
            )
            .expect("test corrupts input");
        assert_eq!(
            store
                .get_verifier_job(&job.job_id)
                .expect_err("corrupt job rejected")
                .code,
            "MCL_VERIFIER_JOB_INTEGRITY_FAILED"
        );
    }

    #[test]
    fn environments_are_immutable_idempotent_and_exact_after_restart() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let manifest = environment_manifest();

        let first = store
            .register_environment(&manifest, "environment-author", "environment-register")
            .expect("environment registers");
        assert_eq!(
            first.environment_hash,
            include_str!("../../fixtures/environment/lean-4.32-local.sha256").trim()
        );
        assert_eq!(first.created_by, "environment-author");
        assert_eq!(store.environment_count().expect("environment count"), 1);
        assert_eq!(
            store
                .register_environment(&manifest, "environment-author", "environment-register")
                .expect("exact retry"),
            first
        );
        assert_eq!(
            store
                .register_environment(&manifest, "environment-author", "different-key")
                .expect_err("duplicate environment")
                .code,
            "MCL_ENVIRONMENT_EXISTS"
        );

        let mut changed = manifest.clone();
        changed.resource_limits.timeout_seconds += 1;
        assert_eq!(
            store
                .register_environment(&changed, "environment-author", "environment-register")
                .expect_err("idempotency key reuse")
                .code,
            "MCL_IDEMPOTENCY_CONFLICT"
        );
        let second = store
            .register_environment(&changed, "environment-author", "environment-second")
            .expect("changed environment registers");
        assert_ne!(first.environment_hash, second.environment_hash);
        assert_eq!(
            store.list_environments(10).expect("environment list").len(),
            2
        );

        drop(store);
        let restarted = Store::open(&database).expect("database reopens");
        assert_eq!(
            restarted
                .get_environment(&first.environment_hash)
                .expect("environment survives restart"),
            first
        );
    }

    #[test]
    fn database_rejects_environment_rewrite_and_deletion() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let environment = store
            .register_environment(
                &environment_manifest(),
                "environment-author",
                "environment-protected",
            )
            .expect("environment registers");

        assert!(
            store
                .connection
                .execute(
                    "UPDATE environments SET trust_profile = 'publication' WHERE environment_hash = ?1",
                    [&environment.environment_hash],
                )
                .is_err()
        );
        assert!(
            store
                .connection
                .execute(
                    "DELETE FROM environments WHERE environment_hash = ?1",
                    [&environment.environment_hash],
                )
                .is_err()
        );
        assert_eq!(
            store
                .get_environment(&environment.environment_hash)
                .expect("protected environment"),
            environment
        );
    }

    #[test]
    fn detects_environment_manifest_corruption() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let environment = store
            .register_environment(
                &environment_manifest(),
                "environment-author",
                "environment-corruption",
            )
            .expect("environment registers");

        store
            .connection
            .execute("DROP TRIGGER environments_reject_update", [])
            .expect("test removes mutation guard");
        store
            .connection
            .execute(
                "UPDATE environments SET manifest_json = '{}' WHERE environment_hash = ?1",
                [&environment.environment_hash],
            )
            .expect("test simulates corruption");
        assert_eq!(
            store
                .get_environment(&environment.environment_hash)
                .expect_err("corruption detected")
                .code,
            "MCL_ENVIRONMENT_INTEGRITY_FAILED"
        );
    }

    fn claim(statement: &str) -> RecordDraft {
        RecordDraft {
            kind: RecordKind::Claim,
            schema_version: "claim/1".to_owned(),
            payload: json!({
                "source_reference": {
                    "object_id": "fixture-source",
                    "version_hash": "a".repeat(64)
                },
                "normalized_informal_statement": statement,
                "claim_kind": "universal",
                "logical_shape": "forall",
                "assumptions": [],
                "variables": [],
                "concept_links": [],
                "source_citations": [],
                "ambiguity_notes": []
            }),
            searchable_text: statement.to_owned(),
        }
    }

    #[test]
    fn versions_are_immutable_and_exactly_addressable() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");

        let first = store
            .create_record(
                &claim("Every prime number is odd"),
                "author",
                "create-claim",
            )
            .expect("record created");
        assert_eq!(
            Uuid::parse_str(&first.object_id)
                .expect("UUID")
                .get_version_num(),
            7
        );
        let second = store
            .version_record(
                &first.object_id,
                &first.version_hash,
                &claim("Every prime number other than 2 is odd"),
                "reviewer",
                "repair-claim",
            )
            .expect("record versioned");

        assert_eq!(second.predecessor_hash, Some(first.version_hash.clone()));
        assert_eq!(
            store
                .get_record(&first.object_id)
                .expect("current head")
                .version_hash,
            second.version_hash
        );
        assert_eq!(
            store
                .get_record_version(&first.version_hash)
                .expect("old version remains"),
            first
        );
    }

    #[test]
    fn database_rejects_version_rewrite_and_foreign_head() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let first = store
            .create_record(&claim("A"), "author", "first")
            .expect("first record");
        let second = store
            .create_record(&claim("B"), "author", "second")
            .expect("second record");

        assert!(
            store
                .connection
                .execute(
                    "UPDATE record_versions SET payload_json = '{\"statement\":\"forged\"}' WHERE version_hash = ?1",
                    [&first.version_hash],
                )
                .is_err()
        );
        assert!(
            store
                .connection
                .execute(
                    "UPDATE records SET head_version_hash = ?1 WHERE object_id = ?2",
                    params![second.version_hash, first.object_id],
                )
                .is_err()
        );
        assert!(
            store
                .connection
                .execute(
                    "UPDATE records SET head_version_hash = NULL WHERE object_id = ?1",
                    [&first.object_id],
                )
                .is_err()
        );
        assert!(
            store
                .connection
                .execute(
                    "UPDATE idempotency_results SET result_json = '{}' WHERE operation = 'record.create' AND idempotency_key = 'first'",
                    [],
                )
                .is_err()
        );
        assert_eq!(
            store
                .get_record(&first.object_id)
                .expect("first remains intact"),
            first
        );
    }

    #[test]
    fn idempotent_retry_returns_original_result_and_reuse_conflicts() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let original = store
            .create_record(&claim("A"), "author", "same-key")
            .expect("created");
        let retry = store
            .create_record(&claim("A"), "author", "same-key")
            .expect("retried");
        assert_eq!(retry, original);
        assert_eq!(
            store
                .create_record(&claim("B"), "author", "same-key")
                .expect_err("different input conflicts")
                .code,
            "MCL_IDEMPOTENCY_CONFLICT"
        );
    }

    #[test]
    fn database_rejects_evidence_rewrite_deletion_and_subject_mismatch() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let first = store
            .create_record(&claim("evidence subject"), "author", "evidence-subject")
            .expect("first subject");
        let second = store
            .create_record(&claim("other subject"), "author", "evidence-other")
            .expect("second subject");
        let evidence_id = Uuid::now_v7().to_string();
        store
            .connection
            .execute(
                "INSERT INTO evidence(evidence_id, subject_object_id, subject_version_hash, evidence_kind, result, authority_class, run_id, environment_hash, artifact_hash, metadata_json, created_at, superseded_by, evidence_hash, job_id, artifact_hashes_json, verifier_identity, created_by, stale_reason) VALUES (?1, ?2, ?3, 'lean_elaboration', 'accepted', 'diagnostic', NULL, NULL, NULL, '{}', unixepoch(), NULL, ?4, NULL, '[]', 'test-worker', 'test-author', NULL)",
                params![evidence_id, first.object_id, first.version_hash, "d".repeat(64)],
            )
            .expect("evidence fixture inserts");
        assert!(
            store
                .connection
                .execute(
                    "UPDATE evidence SET result = 'failed' WHERE evidence_id = ?1",
                    [&evidence_id],
                )
                .is_err()
        );
        assert!(
            store
                .connection
                .execute(
                    "DELETE FROM evidence WHERE evidence_id = ?1",
                    [&evidence_id]
                )
                .is_err()
        );
        assert!(
            store
                .connection
                .execute(
                    "INSERT INTO evidence(evidence_id, subject_object_id, subject_version_hash, evidence_kind, result, authority_class, run_id, environment_hash, artifact_hash, metadata_json, created_at, superseded_by, evidence_hash, job_id, artifact_hashes_json, verifier_identity, created_by, stale_reason) VALUES (?1, ?2, ?3, 'lean_elaboration', 'accepted', 'diagnostic', NULL, NULL, NULL, '{}', unixepoch(), NULL, ?4, NULL, '[]', 'test-worker', 'test-author', NULL)",
                    params![Uuid::now_v7().to_string(), first.object_id, second.version_hash, "e".repeat(64)],
                )
                .is_err()
        );
    }

    #[test]
    fn stale_compare_and_swap_rolls_back_proposed_version() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let first = store
            .create_record(&claim("A"), "author", "create")
            .expect("created");
        let winner = store
            .version_record(
                &first.object_id,
                &first.version_hash,
                &claim("B"),
                "one",
                "winner",
            )
            .expect("winner");
        let losing_draft = claim("C");
        let losing_hash =
            record_version_hash(&losing_draft.schema_version, &losing_draft.payload).expect("hash");
        assert_eq!(
            store
                .version_record(
                    &first.object_id,
                    &first.version_hash,
                    &losing_draft,
                    "two",
                    "loser",
                )
                .expect_err("stale head conflicts")
                .code,
            "MCL_VERSION_CONFLICT"
        );
        assert_eq!(
            store
                .get_record(&first.object_id)
                .expect("current")
                .version_hash,
            winner.version_hash
        );
        assert_eq!(
            store
                .get_record_version(&losing_hash)
                .expect_err("losing insert rolled back")
                .code,
            "MCL_RECORD_VERSION_NOT_FOUND"
        );
    }

    #[test]
    fn committed_records_survive_restart_and_search_tracks_head() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let first = {
            let mut store = Store::open(&database).expect("database opens");
            store.migrate().expect("migration succeeds");
            store
                .create_record(&claim("prime odd counterexample"), "author", "create")
                .expect("created")
        };
        let mut reopened = Store::open(&database).expect("database reopens");
        reopened.migrate().expect("migration remains idempotent");
        assert_eq!(
            reopened.get_record(&first.object_id).expect("persisted"),
            first
        );
        assert_eq!(
            reopened
                .search_records("counterexample", 10)
                .expect("search")
                .len(),
            1
        );
        reopened
            .version_record(
                &first.object_id,
                &first.version_hash,
                &claim("repaired theorem"),
                "reviewer",
                "repair",
            )
            .expect("new head");
        assert!(
            reopened
                .search_records("counterexample", 10)
                .expect("old search")
                .is_empty()
        );
        assert_eq!(
            reopened
                .search_records("repaired", 10)
                .expect("new search")
                .len(),
            1
        );
    }

    #[test]
    fn concurrent_writers_produce_one_winner_and_one_conflict() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let first = {
            let mut store = Store::open(&database).expect("database opens");
            store.migrate().expect("migration succeeds");
            store
                .create_record(&claim("A"), "author", "create")
                .expect("created")
        };
        let barrier = Arc::new(Barrier::new(2));
        let stores = [
            Store::open(&database).expect("first concurrent database opens"),
            Store::open(&database).expect("second concurrent database opens"),
        ];
        let handles: Vec<_> = ["B", "C"]
            .into_iter()
            .zip(stores)
            .map(|(statement, mut store)| {
                let barrier = Arc::clone(&barrier);
                let object_id = first.object_id.clone();
                let head = first.version_hash.clone();
                thread::spawn(move || {
                    barrier.wait();
                    store.version_record(
                        &object_id,
                        &head,
                        &claim(statement),
                        statement,
                        &format!("writer-{statement}"),
                    )
                })
            })
            .collect();
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().expect("writer joins"))
            .collect();
        assert_eq!(
            results.iter().filter(|result| result.is_ok()).count(),
            1,
            "{results:?}"
        );
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .filter(|error| error.code == "MCL_VERSION_CONFLICT")
                .count(),
            1,
            "{results:?}"
        );
    }

    fn edge(kind: EdgeKind, source: &RecordSnapshot, target: &RecordSnapshot) -> EdgeDraft {
        EdgeDraft {
            kind,
            source_object_id: source.object_id.clone(),
            source_version_hash: source.version_hash.clone(),
            target_object_id: target.object_id.clone(),
            target_version_hash: target.version_hash.clone(),
            payload: json!({}),
        }
    }

    fn three_records(store: &mut Store) -> (RecordSnapshot, RecordSnapshot, RecordSnapshot) {
        (
            store
                .create_record(&claim("A"), "author", "record-a")
                .expect("A"),
            store
                .create_record(&claim("B"), "author", "record-b")
                .expect("B"),
            store
                .create_record(&claim("C"), "author", "record-c")
                .expect("C"),
        )
    }

    #[test]
    fn edge_is_version_bound_immutable_and_idempotent() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let (a, b, _) = three_records(&mut store);
        let draft = edge(EdgeKind::LogicDependsOn, &a, &b);
        let created = store
            .create_edge(&draft, "author", "edge-create")
            .expect("edge created");
        assert_eq!(
            store.get_edge(&created.edge_id).expect("edge lookup"),
            created
        );
        assert_eq!(
            store
                .create_edge(&draft, "author", "edge-create")
                .expect("edge retry"),
            created
        );
        assert!(
            store
                .connection
                .execute(
                    "UPDATE edges SET edge_type = 'logic.implies' WHERE edge_id = ?1",
                    [&created.edge_id],
                )
                .is_err()
        );

        let mut invalid = draft;
        invalid.source_version_hash = b.version_hash.clone();
        assert_eq!(
            store
                .create_edge(&invalid, "author", "edge-invalid")
                .expect_err("version belongs to another object")
                .code,
            "MCL_EDGE_ENDPOINT_INVALID"
        );
    }

    #[test]
    fn hard_prerequisite_cycle_fails_but_logical_equivalence_cycle_is_allowed() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let (a, b, c) = three_records(&mut store);
        store
            .create_edge(
                &edge(EdgeKind::PedagogyHardPrerequisite, &a, &b),
                "teacher",
                "a-b",
            )
            .expect("A needs B");
        store
            .create_edge(
                &edge(EdgeKind::PedagogyHardPrerequisite, &b, &c),
                "teacher",
                "b-c",
            )
            .expect("B needs C");
        assert_eq!(
            store
                .create_edge(
                    &edge(EdgeKind::PedagogyHardPrerequisite, &c, &a),
                    "teacher",
                    "c-a",
                )
                .expect_err("cycle rejected")
                .code,
            "MCL_PEDAGOGY_CYCLE"
        );

        let direct_cycle = store.connection.execute(
            "INSERT INTO edges(edge_id, edge_type, source_object_id, source_version_hash, target_object_id, target_version_hash, payload_json, created_at, created_by) VALUES (?1, 'pedagogy.hard_prerequisite', ?2, ?3, ?4, ?5, '{}', unixepoch(), 'forger')",
            params![Uuid::now_v7().to_string(), c.object_id, c.version_hash, a.object_id, a.version_hash],
        );
        assert!(direct_cycle.is_err());

        store
            .create_edge(
                &edge(EdgeKind::LogicEquivalentTo, &a, &b),
                "mathematician",
                "equiv-a-b",
            )
            .expect("equivalence forward");
        store
            .create_edge(
                &edge(EdgeKind::LogicEquivalentTo, &b, &a),
                "mathematician",
                "equiv-b-a",
            )
            .expect("equivalence reverse");
    }

    fn source(label: &str) -> RecordDraft {
        RecordDraft {
            kind: RecordKind::Source,
            schema_version: "source/1".to_owned(),
            payload: json!({
                "source_type": "user_statement",
                "title_or_label": label,
                "authors_or_origin": ["fixture author"],
                "canonical_locator": format!("local:{label}"),
                "acquisition_date": "2026-07-19",
                "license_expression": null,
                "redistribution_status": "unknown",
                "content_hash": null,
                "citation_metadata": {},
                "redaction_class": "private",
                "provenance_notes": "test fixture",
                "original_text": label
            }),
            searchable_text: label.to_owned(),
        }
    }

    fn formalization(
        claim: &RecordSnapshot,
        theorem_type: &str,
        environment_hash: &str,
        artifact_hash: &str,
        imports: &[&str],
    ) -> RecordDraft {
        RecordDraft {
            kind: RecordKind::Formalization,
            schema_version: "formalization/1".to_owned(),
            payload: json!({
                "claim_version": {
                    "object_id": claim.object_id,
                    "version_hash": claim.version_hash
                },
                "formal_system": "lean4",
                "environment_hash": environment_hash,
                "module_artifact_hash": artifact_hash,
                "declaration_name": "MathOS.Pilot.prime_claim",
                "exact_theorem_type": theorem_type,
                "declaration_hash": "d".repeat(64),
                "import_manifest": imports,
                "formalization_notes": "test formalization variant",
                "fidelity_evidence_references": [],
                "verification_evidence_references": []
            }),
            searchable_text: theorem_type.to_owned(),
        }
    }

    #[test]
    fn publication_request_registration_is_head_bound_and_idempotent_for_existing_cas_identity() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let claim = store
            .create_record(
                &claim("Publication registration"),
                "author",
                "publication-claim",
            )
            .expect("claim created");
        let environment = store
            .register_environment(
                &environment_manifest(),
                "environment-author",
                "publication-environment",
            )
            .expect("environment registers");
        let module = register_lean_artifact(
            &mut store,
            b"theorem publicationFixture : True := by trivial\n",
            "publication-module",
        );
        let mut formalization_draft = formalization(
            &claim,
            "True",
            &environment.environment_hash,
            &module.artifact_hash,
            &[],
        );
        formalization_draft.payload["claim_polarity"] = json!("claim");
        formalization_draft.payload["declaration_name"] = json!("MathOS.publicationFixture");
        let formalization = store
            .create_record(
                &formalization_draft,
                "formalizer",
                "publication-formalization",
            )
            .expect("formalization created");
        let request = PublicationRequest {
            schema_version: crate::domain::publication::PUBLICATION_REQUEST_SCHEMA_VERSION
                .to_owned(),
            subject: ExactVersionReference {
                object_id: formalization.object_id.clone(),
                version_hash: formalization.version_hash.clone(),
            },
            outcome: crate::domain::PublicationOutcome::Proof,
            diagnostic_evidence_id: Uuid::now_v7().to_string(),
            diagnostic_evidence_hash: "1".repeat(64),
            proof_closure_evidence_id: Uuid::now_v7().to_string(),
            proof_closure_evidence_hash: "2".repeat(64),
            axiom_audit_evidence_id: Uuid::now_v7().to_string(),
            axiom_audit_evidence_hash: "3".repeat(64),
            environment_hash: environment.environment_hash,
            module_artifact_hash: module.artifact_hash,
            declaration_name: "MathOS.publicationFixture".to_owned(),
            policy_hash: "4".repeat(64),
            source_commit_sha: "5".repeat(40),
            source_tree_sha: "6".repeat(40),
        };
        let materialize = |request: &PublicationRequest| {
            let bytes = canonical_json(
                &serde_json::to_value(request).expect("request serializes for test"),
            )
            .expect("request canonicalizes");
            let hash = format!("{:x}", Sha256::digest(&bytes));
            assert_eq!(hash, request.request_hash().expect("request hash"));
            let metadata = ArtifactMetadata {
                schema_version: crate::domain::artifact::ARTIFACT_METADATA_SCHEMA_VERSION
                    .to_owned(),
                media_type: ArtifactMediaType::Json,
                creation_source: ArtifactCreationSource::Generated,
                license_expression: None,
                restriction: ArtifactRestriction::Private,
                semantic_metadata: BTreeMap::from([
                    ("artifact_role".to_owned(), "publication_request".to_owned()),
                    ("request_hash".to_owned(), hash.clone()),
                    (
                        "formalization_object_id".to_owned(),
                        request.subject.object_id.clone(),
                    ),
                    (
                        "formalization_version_hash".to_owned(),
                        request.subject.version_hash.clone(),
                    ),
                    (
                        "source_commit_sha".to_owned(),
                        request.source_commit_sha.clone(),
                    ),
                    (
                        "source_tree_sha".to_owned(),
                        request.source_tree_sha.clone(),
                    ),
                ]),
            };
            (bytes, hash, metadata)
        };

        let (bytes, hash, metadata) = materialize(&request);
        let first = store
            .register_publication_request_artifact(
                &hash,
                bytes.len() as u64,
                &metadata,
                &request,
                "publisher",
                "publication-request-a",
            )
            .expect("publication request registers");
        assert_eq!(
            store
                .register_publication_request_artifact(
                    &hash,
                    bytes.len() as u64,
                    &metadata,
                    &request,
                    "publisher",
                    "publication-request-a",
                )
                .expect("exact publication retry"),
            first
        );
        assert_eq!(
            store
                .register_publication_request_artifact(
                    &hash,
                    bytes.len() as u64,
                    &metadata,
                    &request,
                    "publisher",
                    "publication-request-a-second-receipt",
                )
                .expect("matching existing request records a second exact receipt"),
            first
        );
        let mut mismatched_metadata = metadata.clone();
        mismatched_metadata
            .semantic_metadata
            .insert("request_hash".to_owned(), "8".repeat(64));
        assert_eq!(
            store
                .register_publication_request_artifact(
                    &hash,
                    bytes.len() as u64,
                    &mismatched_metadata,
                    &request,
                    "publisher",
                    "publication-request-bad-metadata",
                )
                .expect_err("request metadata mismatch fails closed")
                .code,
            "MCL_PUBLICATION_REQUEST_ARTIFACT_INVALID"
        );

        let mut other_request = request.clone();
        other_request.source_tree_sha = "7".repeat(40);
        let (other_bytes, other_hash, other_metadata) = materialize(&other_request);
        store
            .register_publication_request_artifact(
                &other_hash,
                other_bytes.len() as u64,
                &other_metadata,
                &other_request,
                "publisher",
                "publication-request-b",
            )
            .expect("second publication request registers");
        assert_eq!(
            store
                .register_publication_request_artifact(
                    &other_hash,
                    other_bytes.len() as u64,
                    &other_metadata,
                    &other_request,
                    "publisher",
                    "publication-request-a",
                )
                .expect_err("used key cannot target an already-existing second request")
                .code,
            "MCL_IDEMPOTENCY_CONFLICT"
        );

        let mut successor_draft = formalization_draft;
        successor_draft.payload["formalization_notes"] = json!("superseding exact version");
        store
            .version_record(
                &formalization.object_id,
                &formalization.version_hash,
                &successor_draft,
                "formalizer",
                "publication-formalization-successor",
            )
            .expect("formalization is superseded");
        assert_eq!(
            store
                .register_publication_request_artifact(
                    &hash,
                    bytes.len() as u64,
                    &metadata,
                    &request,
                    "publisher",
                    "publication-request-stale",
                )
                .expect_err("stale request cannot register")
                .code,
            "MCL_PUBLICATION_SUBJECT_STALE"
        );
    }

    #[test]
    fn diagnostic_evidence_is_exact_idempotent_restart_safe_and_corruption_detecting() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let claim = store
            .create_record(&claim("True is inhabited"), "author", "evidence-claim")
            .expect("claim created");
        let environment = store
            .register_environment(
                &environment_manifest(),
                "environment-author",
                "evidence-environment",
            )
            .expect("environment registers");
        let module = register_lean_artifact(
            &mut store,
            b"theorem fixture : True := by trivial\n",
            "evidence-module",
        );
        let mut formalization_draft = formalization(
            &claim,
            "True",
            &environment.environment_hash,
            &module.artifact_hash,
            &[],
        );
        formalization_draft.payload["declaration_name"] = json!("MathOS.Verifier.fixture");
        let formalization_record = store
            .create_record(&formalization_draft, "formalizer", "evidence-formalization")
            .expect("formalization created");
        let request = verifier_request(&environment.environment_hash, &module.artifact_hash);
        let queued = store
            .enqueue_verifier_job(&request, 0, "verifier", "evidence-job")
            .expect("job enqueued");
        store
            .lease_next_verifier_job("evidence-worker", 60)
            .expect("lease succeeds")
            .expect("job leased");
        store
            .mark_verifier_job_running(&queued.job_id, "evidence-worker")
            .expect("job starts");
        let report_bytes = b"{}";
        let report_hash = format!("{:x}", Sha256::digest(report_bytes));
        let report_metadata = ArtifactMetadata {
            schema_version: "artifact_metadata/1".to_owned(),
            media_type: crate::domain::ArtifactMediaType::Json,
            creation_source: crate::domain::ArtifactCreationSource::Verifier,
            license_expression: None,
            restriction: crate::domain::ArtifactRestriction::Private,
            semantic_metadata: BTreeMap::new(),
        };
        store
            .register_artifact(
                &report_hash,
                report_bytes.len() as u64,
                &report_metadata,
                "evidence-worker",
                "evidence-report",
            )
            .expect("report artifact registers");
        store
            .finish_verifier_job(&queued.job_id, "evidence-worker", &report_hash, true, None)
            .expect("job finishes");
        let mut artifact_hashes = vec![module.artifact_hash.clone(), report_hash];
        artifact_hashes.sort();
        let payload = EvidencePayload {
            schema_version: crate::domain::evidence::EVIDENCE_SCHEMA_VERSION.to_owned(),
            subject: ExactVersionReference {
                object_id: formalization_record.object_id,
                version_hash: formalization_record.version_hash,
            },
            evidence_kind: EvidenceKind::LeanElaboration,
            result: crate::domain::EvidenceResult::Accepted,
            authority_class: EvidenceAuthorityClass::Diagnostic,
            producing_run_id: None,
            producing_job_id: Some(queued.job_id),
            artifact_hashes,
            verifier_or_reviewer_identity: "lean:test".to_owned(),
            environment_hash: Some(environment.environment_hash.clone()),
            supersedes_evidence_id: None,
            stale: false,
            stale_reason: None,
            publication_authority: None,
        };
        let mismatched_formalization = store
            .create_record(
                &formalization(
                    &claim,
                    "True",
                    &environment.environment_hash,
                    &module.artifact_hash,
                    &[],
                ),
                "formalizer",
                "evidence-mismatched-formalization",
            )
            .expect("mismatched formalization created");
        let mut mismatched_payload = payload.clone();
        mismatched_payload.subject = ExactVersionReference {
            object_id: mismatched_formalization.object_id,
            version_hash: mismatched_formalization.version_hash,
        };
        assert_eq!(
            store
                .create_diagnostic_evidence(
                    &mismatched_payload,
                    "reviewer",
                    "evidence-mismatched-create",
                )
                .expect_err("mismatched verifier target rejected")
                .code,
            "MCL_EVIDENCE_FORMALIZATION_MISMATCH"
        );
        let evidence = store
            .create_diagnostic_evidence(&payload, "reviewer", "evidence-create")
            .expect("diagnostic evidence created");
        assert_eq!(
            store
                .create_diagnostic_evidence(&payload, "reviewer", "evidence-create")
                .expect("exact retry"),
            evidence
        );
        let policy = crate::domain::audit::committed_audit_policy().expect("audit policy");
        let audit_request = LeanAuditRequest {
            schema_version: crate::domain::audit::AUDIT_REQUEST_SCHEMA_VERSION.to_owned(),
            subject: evidence.payload.subject.clone(),
            diagnostic_evidence_id: evidence.evidence_id.clone(),
            diagnostic_evidence_hash: evidence.evidence_hash.clone(),
            environment_hash: environment.environment_hash.clone(),
            module_artifact_hash: module.artifact_hash,
            declaration_name: "MathOS.Verifier.fixture".to_owned(),
            policy_hash: policy.policy_hash().expect("policy hash"),
        };
        assert_eq!(
            store
                .validate_audit_job_enqueue(&audit_request, 5)
                .expect("audit validates"),
            audit_request.request_hash().expect("audit request hash")
        );
        let mut wrong_declaration = audit_request.clone();
        wrong_declaration.declaration_name = "MathOS.Verifier.other".to_owned();
        assert_eq!(
            store
                .validate_audit_job_enqueue(&wrong_declaration, 5)
                .expect_err("wrong declaration rejected")
                .code,
            "MCL_AUDIT_FORMALIZATION_MISMATCH"
        );
        let mut changed_environment = environment_manifest();
        changed_environment.resource_limits.timeout_seconds += 1;
        let other_environment = store
            .register_environment(
                &changed_environment,
                "environment-author",
                "audit-other-environment",
            )
            .expect("other environment registers");
        let mut wrong_environment = audit_request.clone();
        wrong_environment.environment_hash = other_environment.environment_hash;
        assert_eq!(
            store
                .validate_audit_job_enqueue(&wrong_environment, 5)
                .expect_err("wrong environment rejected")
                .code,
            "MCL_AUDIT_FORMALIZATION_MISMATCH"
        );
        let audit = store
            .enqueue_audit_job(&audit_request, 5, "auditor", "audit-enqueue")
            .expect("audit enqueues");
        assert_eq!(
            store
                .enqueue_audit_job(&audit_request, 5, "auditor", "audit-enqueue")
                .expect("audit exact retry"),
            audit
        );
        let mut forged_policy = audit_request.clone();
        forged_policy.policy_hash = "f".repeat(64);
        assert_eq!(
            store
                .enqueue_audit_job(&forged_policy, 5, "auditor", "audit-forged-policy")
                .expect_err("forged audit policy rejected")
                .code,
            "MCL_AUDIT_POLICY_MISMATCH"
        );
        let leased_audit = store
            .lease_next_audit_job("audit-worker", 60)
            .expect("audit lease succeeds")
            .expect("audit leased");
        assert_eq!(leased_audit.job_id, audit.job_id);
        store
            .mark_audit_job_running(&audit.job_id, "audit-worker")
            .expect("audit starts");
        let audit_report_bytes = b"{\"audit\":true}";
        let audit_report_hash = format!("{:x}", Sha256::digest(audit_report_bytes));
        store
            .register_artifact(
                &audit_report_hash,
                audit_report_bytes.len() as u64,
                &report_metadata,
                "audit-worker",
                "audit-report",
            )
            .expect("audit report registers");
        let finished_audit = store
            .finish_audit_job(
                &audit.job_id,
                "audit-worker",
                &audit_report_hash,
                true,
                None,
            )
            .expect("audit finishes");
        assert_eq!(finished_audit.state, VerifierJobState::Succeeded);
        let mut audit_artifacts = vec![
            audit_request.module_artifact_hash.clone(),
            audit_report_hash,
        ];
        audit_artifacts.sort();
        let audit_payload = |kind| EvidencePayload {
            schema_version: crate::domain::evidence::EVIDENCE_SCHEMA_VERSION.to_owned(),
            subject: audit_request.subject.clone(),
            evidence_kind: kind,
            result: EvidenceResult::Accepted,
            authority_class: EvidenceAuthorityClass::Diagnostic,
            producing_run_id: None,
            producing_job_id: Some(audit.job_id.clone()),
            artifact_hashes: audit_artifacts.clone(),
            verifier_or_reviewer_identity: "lean-audit:test".to_owned(),
            environment_hash: Some(audit_request.environment_hash.clone()),
            supersedes_evidence_id: None,
            stale: false,
            stale_reason: None,
            publication_authority: None,
        };
        let audit_payloads = [
            audit_payload(EvidenceKind::ProofClosureScan),
            audit_payload(EvidenceKind::AxiomAudit),
        ];
        let audit_evidence = store
            .create_audit_evidence_pair(&audit_payloads, "audit-reviewer", "audit-evidence-create")
            .expect("audit evidence pair created");
        assert_eq!(audit_evidence.len(), 2);
        assert_eq!(
            store
                .create_audit_evidence_pair(
                    &audit_payloads,
                    "audit-reviewer",
                    "audit-evidence-create",
                )
                .expect("audit evidence exact retry"),
            audit_evidence
        );
        drop(store);

        let reopened = Store::open(&database).expect("database reopens");
        assert_eq!(
            reopened
                .get_evidence(&evidence.evidence_id)
                .expect("evidence survives restart"),
            evidence
        );
        assert_eq!(
            reopened
                .get_audit_job(&audit.job_id)
                .expect("audit survives restart"),
            finished_audit
        );
        assert_eq!(
            reopened.list_audit_jobs(10).expect("audit listing"),
            [finished_audit]
        );
        for audit_record in &audit_evidence {
            assert_eq!(
                reopened
                    .get_evidence(&audit_record.evidence_id)
                    .expect("audit evidence survives restart"),
                *audit_record
            );
        }
        reopened
            .connection
            .execute("DROP TRIGGER evidence_reject_update", [])
            .expect("test removes evidence mutation guard");
        reopened
            .connection
            .execute(
                "UPDATE evidence SET result = 'failed' WHERE evidence_id = ?1",
                [&evidence.evidence_id],
            )
            .expect("test simulates projection corruption");
        assert_eq!(
            reopened
                .get_evidence(&evidence.evidence_id)
                .expect_err("projection corruption detected")
                .code,
            "MCL_EVIDENCE_INTEGRITY_FAILED"
        );
    }

    #[test]
    fn one_claim_retains_multiple_exact_formalizations_and_sensitive_changes_rehash() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let claim = store
            .create_record(
                &claim("Every prime number is odd"),
                "author",
                "claim-formalized",
            )
            .expect("claim created");
        let first_environment = store
            .register_environment(
                &environment_manifest(),
                "environment-author",
                "formalization-environment-a",
            )
            .expect("first environment");
        let mut changed_environment = environment_manifest();
        changed_environment.resource_limits.timeout_seconds += 1;
        let second_environment = store
            .register_environment(
                &changed_environment,
                "environment-author",
                "formalization-environment-b",
            )
            .expect("second environment");
        let first_artifact = register_lean_artifact(
            &mut store,
            b"theorem prime_claim_a : True := by trivial\n",
            "formalization-artifact-a",
        );
        let second_artifact = register_lean_artifact(
            &mut store,
            b"theorem prime_claim_b : True := by trivial\n",
            "formalization-artifact-b",
        );
        let variants = [
            formalization(
                &claim,
                "∀ p, Nat.Prime p → Odd p",
                &first_environment.environment_hash,
                &first_artifact.artifact_hash,
                &["Mathlib"],
            ),
            formalization(
                &claim,
                "∀ p, Nat.Prime p → p % 2 = 1",
                &first_environment.environment_hash,
                &first_artifact.artifact_hash,
                &["Mathlib"],
            ),
            formalization(
                &claim,
                "∀ p, Nat.Prime p → Odd p",
                &second_environment.environment_hash,
                &first_artifact.artifact_hash,
                &["Mathlib"],
            ),
            formalization(
                &claim,
                "∀ p, Nat.Prime p → Odd p",
                &first_environment.environment_hash,
                &second_artifact.artifact_hash,
                &["Mathlib"],
            ),
            formalization(
                &claim,
                "∀ p, Nat.Prime p → Odd p",
                &first_environment.environment_hash,
                &first_artifact.artifact_hash,
                &["Mathlib", "MathOS.Foundation"],
            ),
        ];
        let created = variants
            .iter()
            .enumerate()
            .map(|(index, draft)| {
                store
                    .create_record(draft, "formalizer", &format!("formalization-{index}"))
                    .expect("formalization created")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            created
                .iter()
                .map(|item| item.object_id.as_str())
                .collect::<HashSet<_>>()
                .len(),
            created.len()
        );
        assert_eq!(
            created
                .iter()
                .map(|item| item.version_hash.as_str())
                .collect::<HashSet<_>>()
                .len(),
            created.len()
        );
        drop(store);
        let reopened = Store::open(&database).expect("database reopens");
        for formalization in created {
            assert_eq!(
                reopened
                    .get_record_version(&formalization.version_hash)
                    .expect("formalization remains addressable"),
                formalization
            );
        }
    }

    #[test]
    fn claim_status_formalization_reads_are_exact_current_bounded_and_fail_closed() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let claim_snapshot = store
            .create_record(
                &claim("Derived status exact claim"),
                "status-test",
                "status-claim",
            )
            .expect("claim creates");
        let other_claim = store
            .create_record(
                &claim("Unrelated exact claim"),
                "status-test",
                "status-other-claim",
            )
            .expect("other claim creates");
        let environment = store
            .register_environment(&environment_manifest(), "status-test", "status-environment")
            .expect("environment registers");
        let module = register_lean_artifact(
            &mut store,
            b"theorem statusFixture : True := by trivial\n",
            "status-module",
        );
        let mut first_draft = formalization(
            &claim_snapshot,
            "True",
            &environment.environment_hash,
            &module.artifact_hash,
            &[],
        );
        first_draft.payload["claim_polarity"] = json!("claim");
        first_draft.payload["declaration_name"] = json!("MathOS.statusFixtureOne");
        let first = store
            .create_record(&first_draft, "status-test", "status-formalization-one")
            .expect("first formalization creates");
        let mut second_draft = first_draft.clone();
        second_draft.payload["declaration_name"] = json!("MathOS.statusFixtureTwo");
        second_draft.payload["declaration_hash"] = json!("e".repeat(64));
        let second = store
            .create_record(&second_draft, "status-test", "status-formalization-two")
            .expect("second formalization creates");
        let claim_reference = ExactVersionReference {
            object_id: claim_snapshot.object_id.clone(),
            version_hash: claim_snapshot.version_hash.clone(),
        };

        let listed = store
            .list_current_formalizations_for_claim(&claim_reference)
            .expect("current formalizations list");
        let mut expected = vec![first.clone(), second.clone()];
        expected.sort_by(|left, right| left.object_id.cmp(&right.object_id));
        assert_eq!(
            listed, expected,
            "all exact current heads are deterministic"
        );
        assert_eq!(
            store
                .list_current_formalizations_for_claim_bounded(&claim_reference, 1)
                .expect_err("bounded read cannot truncate a second head")
                .code,
            "MCL_CLAIM_STATUS_LIMIT_EXCEEDED"
        );

        let mut successor_draft = first_draft.clone();
        successor_draft.payload["formalization_notes"] = json!("current successor");
        let successor = store
            .version_record(
                &first.object_id,
                &first.version_hash,
                &successor_draft,
                "status-test",
                "status-formalization-successor",
            )
            .expect("formalization successor creates");
        let mut moved_draft = second_draft;
        moved_draft.payload["claim_version"] = json!({
            "object_id": other_claim.object_id,
            "version_hash": other_claim.version_hash,
        });
        store
            .version_record(
                &second.object_id,
                &second.version_hash,
                &moved_draft,
                "status-test",
                "status-formalization-moved",
            )
            .expect("formalization can move only through an exact successor");
        assert_eq!(
            store
                .list_current_formalizations_for_claim(&claim_reference)
                .expect("successor list"),
            vec![successor.clone()],
            "historical heads and heads now bound to another claim are excluded"
        );
        assert_eq!(
            store
                .list_current_formalizations_for_claim(&ExactVersionReference {
                    object_id: successor.object_id.clone(),
                    version_hash: successor.version_hash.clone(),
                })
                .expect_err("formalization cannot substitute for claim")
                .code,
            "MCL_CLAIM_STATUS_SUBJECT_INVALID"
        );

        store
            .connection
            .execute("DROP TRIGGER record_versions_reject_update", [])
            .expect("test removes immutable-record guard");
        store
            .connection
            .execute(
                "UPDATE record_versions SET payload_json = json_set(payload_json, '$.formalization_notes', 'corrupted current head') WHERE version_hash = ?1",
                [&successor.version_hash],
            )
            .expect("test simulates current-head corruption");
        assert_eq!(
            store
                .list_current_formalizations_for_claim(&claim_reference)
                .expect_err("corrupt current head fails closed")
                .code,
            "MCL_CLAIM_STATUS_INTEGRITY_FAILED"
        );
    }

    #[test]
    fn claim_status_basis_detects_authority_and_formalization_head_changes() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let fixture = publication_authority_fixture(
            &mut store,
            "claim-status-basis",
            PublicationOutcome::Proof,
        );
        let formalization_payload: FormalizationPayload =
            serde_json::from_value(fixture.formalization_draft.payload.clone())
                .expect("formalization fixture decodes");
        let claim_reference = formalization_payload.claim_version;
        let claim_snapshot = store
            .get_record_version(&claim_reference.version_hash)
            .expect("claim reads");
        let claim_payload: ClaimPayload =
            serde_json::from_value(claim_snapshot.payload).expect("claim decodes");
        let source_reference = claim_payload.source_reference;

        let before_authority = store
            .capture_claim_status_read_basis(&claim_reference)
            .expect("basis before authority");
        assert_eq!(
            before_authority.current_claim_head_version_hash,
            Some(claim_reference.version_hash.clone())
        );
        assert_eq!(before_authority.source, source_reference);
        assert_eq!(
            before_authority.current_source_head_version_hash,
            Some(source_reference.version_hash.clone())
        );
        assert_eq!(before_authority.formalizations.len(), 1);
        assert!(
            before_authority.formalizations[0]
                .authoritative_evidence
                .is_empty()
        );
        assert!(
            store
                .list_authoritative_evidence_for_subject(&fixture.commit.subject)
                .expect("empty authority list")
                .is_empty()
        );

        let authority = store
            .create_publication_authority_evidence(
                &fixture.commit,
                "status-test",
                "claim-status-authority",
            )
            .expect("authority creates");
        assert_eq!(
            store
                .list_authoritative_evidence_for_subject(&fixture.commit.subject)
                .expect("authority list"),
            vec![authority.clone()]
        );
        assert_eq!(
            store
                .recheck_claim_status_read_basis(&before_authority)
                .expect_err("new authority changes the basis")
                .code,
            "MCL_CLAIM_STATUS_READ_CONFLICT"
        );
        let after_authority = store
            .capture_claim_status_read_basis(&claim_reference)
            .expect("basis after authority");
        store
            .recheck_claim_status_read_basis(&after_authority)
            .expect("unchanged basis rechecks");
        assert_eq!(
            after_authority.formalizations[0].authoritative_evidence,
            vec![ClaimStatusEvidenceReadBasis {
                evidence_id: authority.evidence_id.clone(),
                evidence_hash: authority.evidence_hash.clone(),
            }]
        );
        assert_eq!(
            store
                .list_authoritative_evidence_for_subject(&claim_reference)
                .expect_err("claim cannot substitute for formalization")
                .code,
            "MCL_CLAIM_STATUS_SUBJECT_INVALID"
        );

        let mut successor_draft = fixture.formalization_draft;
        successor_draft.payload["formalization_notes"] =
            json!("successor invalidates old authority currentness");
        let successor = store
            .version_record(
                &fixture.commit.subject.object_id,
                &fixture.commit.subject.version_hash,
                &successor_draft,
                "status-test",
                "claim-status-formalization-successor",
            )
            .expect("formalization successor creates");
        assert_eq!(
            store
                .recheck_claim_status_read_basis(&after_authority)
                .expect_err("new formalization head changes the basis")
                .code,
            "MCL_CLAIM_STATUS_READ_CONFLICT"
        );
        let successor_basis = store
            .capture_claim_status_read_basis(&claim_reference)
            .expect("successor basis");
        assert_eq!(
            successor_basis.formalizations[0].formalization,
            ExactVersionReference {
                object_id: successor.object_id,
                version_hash: successor.version_hash,
            }
        );
        assert!(
            successor_basis.formalizations[0]
                .authoritative_evidence
                .is_empty()
        );
        assert_eq!(
            store
                .list_authoritative_evidence_for_subject(&fixture.commit.subject)
                .expect("historical exact authority remains readable"),
            vec![authority]
        );

        let source_snapshot = store
            .get_record_version(&source_reference.version_hash)
            .expect("source reads");
        let mut source_payload = source_snapshot.payload.clone();
        source_payload["provenance_notes"] =
            json!("source successor changes the status read basis");
        let source_successor = store
            .version_record(
                &source_reference.object_id,
                &source_reference.version_hash,
                &RecordDraft {
                    kind: RecordKind::Source,
                    schema_version: source_snapshot.schema_version,
                    payload: source_payload,
                    searchable_text: "claim status source successor".to_owned(),
                },
                "status-test",
                "claim-status-source-successor",
            )
            .expect("source successor creates");
        assert_eq!(
            store
                .recheck_claim_status_read_basis(&successor_basis)
                .expect_err("new source head changes the basis")
                .code,
            "MCL_CLAIM_STATUS_READ_CONFLICT"
        );
        let after_source_basis = store
            .capture_claim_status_read_basis(&claim_reference)
            .expect("post-source basis");
        assert_eq!(
            after_source_basis.current_source_head_version_hash,
            Some(source_successor.version_hash)
        );

        let claim_successor = store
            .version_record(
                &claim_reference.object_id,
                &claim_reference.version_hash,
                &claim("Revised source claim cannot inherit the old exact status"),
                "status-test",
                "claim-status-claim-successor",
            )
            .expect("claim successor creates");
        assert_eq!(
            store
                .recheck_claim_status_read_basis(&after_source_basis)
                .expect_err("new claim head changes the basis")
                .code,
            "MCL_CLAIM_STATUS_READ_CONFLICT"
        );
        let superseded_claim_basis = store
            .capture_claim_status_read_basis(&claim_reference)
            .expect("superseded exact claim basis");
        assert_eq!(
            superseded_claim_basis.current_claim_head_version_hash,
            Some(claim_successor.version_hash)
        );
        assert!(superseded_claim_basis.formalizations.is_empty());

        drop(store);
        let reopened = Store::open(&database).expect("database reopens");
        reopened
            .recheck_claim_status_read_basis(&superseded_claim_basis)
            .expect("basis survives restart deterministically");
    }

    #[test]
    fn claim_status_basis_recheck_detects_a_second_connection_head_change() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut reader = Store::open(&database).expect("reader opens");
        reader.migrate().expect("migration succeeds");
        let fixture = publication_authority_fixture(
            &mut reader,
            "claim-status-second-connection",
            PublicationOutcome::Proof,
        );
        let formalization_payload: FormalizationPayload =
            serde_json::from_value(fixture.formalization_draft.payload)
                .expect("formalization decodes");
        let claim = formalization_payload.claim_version;
        let basis = reader
            .capture_claim_status_read_basis(&claim)
            .expect("reader captures one atomic basis");

        let mut writer = Store::open(&database).expect("writer opens");
        let current_claim = writer
            .get_record_version(&claim.version_hash)
            .expect("writer reads claim");
        let mut successor_payload = current_claim.payload;
        successor_payload["normalized_informal_statement"] =
            json!("A concurrent writer revised the exact claim.");
        writer
            .version_record(
                &claim.object_id,
                &claim.version_hash,
                &RecordDraft {
                    kind: RecordKind::Claim,
                    schema_version: current_claim.schema_version,
                    payload: successor_payload,
                    searchable_text: "concurrent revised claim".to_owned(),
                },
                "concurrent-writer",
                "claim-status-concurrent-successor",
            )
            .expect("second connection advances the claim head");

        let conflict = reader
            .recheck_claim_status_read_basis(&basis)
            .expect_err("fresh basis comparison detects the concurrent head change");
        assert_eq!(conflict.code, "MCL_CLAIM_STATUS_READ_CONFLICT");
        assert!(conflict.retryable);
    }

    #[test]
    fn formalization_rejects_missing_or_non_claim_exact_references() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let source = store
            .create_record(&source("not a claim"), "author", "reference-source")
            .expect("source created");
        let environment = store
            .register_environment(
                &environment_manifest(),
                "environment-author",
                "reference-environment",
            )
            .expect("environment registered");
        let lean_artifact = register_lean_artifact(
            &mut store,
            b"theorem reference_fixture : True := by trivial\n",
            "reference-artifact",
        );

        let missing_claim = RecordSnapshot {
            object_id: "missing-claim".to_owned(),
            version_hash: "a".repeat(64),
            kind: RecordKind::Claim,
            schema_version: "claim/1".to_owned(),
            payload: json!({}),
            predecessor_hash: None,
            created_at: 0,
            created_by: "test".to_owned(),
        };
        assert_eq!(
            store
                .create_record(
                    &formalization(
                        &missing_claim,
                        "True",
                        &environment.environment_hash,
                        &lean_artifact.artifact_hash,
                        &["Mathlib"],
                    ),
                    "formalizer",
                    "missing-claim",
                )
                .expect_err("missing claim rejected")
                .code,
            "MCL_FORMALIZATION_CLAIM_INVALID"
        );

        assert_eq!(
            store
                .create_record(
                    &formalization(
                        &source,
                        "True",
                        &environment.environment_hash,
                        &lean_artifact.artifact_hash,
                        &["Mathlib"],
                    ),
                    "formalizer",
                    "source-is-not-claim",
                )
                .expect_err("wrong referenced kind rejected")
                .code,
            "MCL_FORMALIZATION_CLAIM_INVALID"
        );

        let claim = store
            .create_record(&claim("A valid claim"), "author", "valid-claim")
            .expect("claim created");
        assert_eq!(
            store
                .create_record(
                    &formalization(
                        &claim,
                        "True",
                        &"f".repeat(64),
                        &lean_artifact.artifact_hash,
                        &["Mathlib"],
                    ),
                    "formalizer",
                    "missing-environment",
                )
                .expect_err("missing environment rejected")
                .code,
            "MCL_FORMALIZATION_ENVIRONMENT_INVALID"
        );
        assert_eq!(
            store
                .create_record(
                    &formalization(
                        &claim,
                        "True",
                        &environment.environment_hash,
                        &"f".repeat(64),
                        &["Mathlib"],
                    ),
                    "formalizer",
                    "missing-artifact",
                )
                .expect_err("missing module artifact rejected")
                .code,
            "MCL_FORMALIZATION_ARTIFACT_INVALID"
        );
        let json_bytes = br#"{"not":"lean source"}"#;
        let json_hash = format!("{:x}", Sha256::digest(json_bytes));
        let json_metadata = ArtifactMetadata {
            schema_version: "artifact_metadata/1".to_owned(),
            media_type: crate::domain::ArtifactMediaType::Json,
            creation_source: crate::domain::ArtifactCreationSource::UserIngest,
            license_expression: Some("PolyForm-Noncommercial-1.0.0".to_owned()),
            restriction: crate::domain::ArtifactRestriction::Restricted,
            semantic_metadata: BTreeMap::new(),
        };
        store
            .register_artifact(
                &json_hash,
                json_bytes.len() as u64,
                &json_metadata,
                "artifact-author",
                "reference-json-artifact",
            )
            .expect("JSON artifact registers");
        assert_eq!(
            store
                .create_record(
                    &formalization(
                        &claim,
                        "True",
                        &environment.environment_hash,
                        &json_hash,
                        &["Mathlib"],
                    ),
                    "formalizer",
                    "wrong-media-artifact",
                )
                .expect_err("non-Lean module artifact rejected")
                .code,
            "MCL_FORMALIZATION_ARTIFACT_INVALID"
        );
        let created = store
            .create_record(
                &formalization(
                    &claim,
                    "True",
                    &environment.environment_hash,
                    &lean_artifact.artifact_hash,
                    &["Mathlib"],
                ),
                "formalizer",
                "valid-formalization",
            )
            .expect("valid formalization created");
        let mut invalid_version = formalization(
            &claim,
            "True ∧ True",
            &environment.environment_hash,
            &lean_artifact.artifact_hash,
            &["Mathlib"],
        );
        invalid_version.payload["claim_version"] = json!({
            "object_id": "missing-claim",
            "version_hash": "a".repeat(64)
        });
        assert_eq!(
            store
                .version_record(
                    &created.object_id,
                    &created.version_hash,
                    &invalid_version,
                    "formalizer",
                    "invalid-formalization-version",
                )
                .expect_err("invalid exact reference rejected on version")
                .code,
            "MCL_FORMALIZATION_CLAIM_INVALID"
        );
        assert_eq!(
            store
                .get_record(&created.object_id)
                .expect("formalization remains readable")
                .version_hash,
            created.version_hash
        );
    }

    #[test]
    fn typed_source_survives_restart_without_normalizing_original_text() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let original = "  Every prime number is odd.  ";
        let created = {
            let mut store = Store::open(&database).expect("database opens");
            store.migrate().expect("migration succeeds");
            store
                .create_record(&source(original), "intake", "source-valid")
                .expect("source created")
        };
        let reopened = Store::open(&database).expect("database reopens");
        let loaded = reopened
            .get_record_version(&created.version_hash)
            .expect("source version loads");
        assert_eq!(loaded, created);
        assert_eq!(loaded.payload["original_text"], original);
    }

    #[test]
    fn rejected_schema_payload_leaves_no_record_or_idempotency_receipt() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");
        let mut invalid = source("invalid source");
        invalid.payload["content_hash"] = json!("bad-hash");
        assert_eq!(
            store
                .create_record(&invalid, "intake", "source-invalid")
                .expect_err("invalid source rejected")
                .code,
            "MCL_SCHEMA_HASH_INVALID"
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM records", [], |row| row
                    .get::<_, i64>(0))
                .expect("record count"),
            0
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM idempotency_results", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("idempotency count"),
            0
        );

        let mut unsupported = source("future source");
        unsupported.schema_version = "source/2".to_owned();
        assert_eq!(
            store
                .create_record(&unsupported, "intake", "source-future")
                .expect_err("unsupported schema rejected")
                .code,
            "MCL_SCHEMA_VERSION_UNSUPPORTED"
        );
    }
}
