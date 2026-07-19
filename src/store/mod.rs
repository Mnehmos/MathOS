use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::canonical::{canonical_json, record_version_hash, value_hash};
use crate::domain::schemas::{
    ExactVersionReference, FormalizationPayload, validate_record_payload,
};
use crate::domain::{
    ArtifactMetadata, ArtifactSnapshot, EdgeDraft, EdgeKind, EdgeSnapshot, EnvironmentManifest,
    EnvironmentSnapshot, EvidenceAuthorityClass, EvidenceKind, EvidencePayload, EvidenceResult,
    EvidenceSnapshot, LeanAuditJobSnapshot, LeanAuditRequest, RecordDraft, RecordKind,
    RecordSnapshot, VerifierJobRequest, VerifierJobSnapshot, VerifierJobState,
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

pub struct Store {
    connection: Connection,
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
        Ok(Self { connection })
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
        validate_mutation_inputs(actor, idempotency_key)?;
        self.validate_artifact_registration(artifact_hash, byte_size, metadata)?;
        let input_hash = value_hash(&json!({
            "operation": "artifact.register",
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
        if let Some(existing) = read_idempotent_result(
            &transaction,
            "artifact.register",
            idempotency_key,
            &input_hash,
        )? {
            return Ok(existing);
        }
        if transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM artifacts WHERE artifact_hash = ?1)",
                [artifact_hash],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| AppError::database("search registered artifact", error))?
        {
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
            "artifact.register",
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
    fn requeue_expired_verifier_jobs(&mut self) -> Result<usize, AppError> {
        requeue_expired_jobs(&self.connection)
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
        validate_hash(&draft.source_version_hash, "source version")?;
        validate_hash(&draft.target_version_hash, "target version")?;
        let input_hash = value_hash(&json!({
            "operation": "edge.create",
            "kind": draft.kind,
            "source_object_id": draft.source_object_id,
            "source_version_hash": draft.source_version_hash,
            "target_object_id": draft.target_object_id,
            "target_version_hash": draft.target_version_hash,
            "payload": draft.payload,
            "actor": actor,
        }))?;
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

    pub fn get_edge(&self, edge_id: &str) -> Result<EdgeSnapshot, AppError> {
        read_edge(&self.connection, edge_id)
    }
}

fn validate_record_references(
    connection: &Connection,
    draft: &RecordDraft,
) -> Result<(), AppError> {
    if draft.kind != RecordKind::Formalization {
        return Ok(());
    }
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
            "SELECT evidence_hash, metadata_json, created_at, created_by, subject_object_id, subject_version_hash, evidence_kind, result, authority_class, run_id, job_id, environment_hash, artifact_hashes_json, verifier_identity, stale_reason FROM evidence WHERE evidence_id = ?1 AND evidence_hash IS NOT NULL",
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
    {
        return Err(AppError::new(
            "MCL_EVIDENCE_INTEGRITY_FAILED",
            format!("stored evidence projections disagree with {evidence_id}"),
            false,
            "Quarantine the database and restore a verified backup.",
        ));
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

    #[test]
    fn migration_produces_wal_database_with_fts5() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");

        assert_eq!(store.migration_version().expect("migration version"), 9);
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
        assert_eq!(store.migration_version().expect("migration version"), 9);
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
        assert_eq!(store.migration_version().expect("current version"), 9);
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
