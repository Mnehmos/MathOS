use std::fs;
use std::path::Path;
use std::str::FromStr;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::canonical::{canonical_json, record_version_hash, value_hash};
use crate::domain::schemas::{FormalizationPayload, validate_record_payload};
use crate::domain::{EdgeDraft, EdgeKind, EdgeSnapshot, RecordDraft, RecordKind, RecordSnapshot};
use crate::error::AppError;

mod graph;
mod runs;

const MIGRATION_0001: &str = include_str!("../../migrations/0001_initial.sql");
const MIGRATION_0002: &str = include_str!("../../migrations/0002_idempotency.sql");
const MIGRATION_0003: &str = include_str!("../../migrations/0003_record_invariants.sql");
const MIGRATION_0004: &str = include_str!("../../migrations/0004_edge_invariants.sql");
const MIGRATION_0005: &str = include_str!("../../migrations/0005_run_event_invariants.sql");
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

    pub fn get_edge(&self, edge_id: &str) -> Result<EdgeSnapshot, AppError> {
        read_edge(&self.connection, edge_id)
    }
}

fn validate_record_references(
    transaction: &rusqlite::Transaction<'_>,
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
    let actual_kind = transaction
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
        Some("claim") => Ok(()),
        Some(kind) => Err(AppError::new(
            "MCL_FORMALIZATION_CLAIM_INVALID",
            format!("formalization claim reference resolves to `{kind}`, not `claim`"),
            false,
            "Reference an exact existing claim object and version.",
        )),
        None => Err(AppError::new(
            "MCL_FORMALIZATION_CLAIM_INVALID",
            "formalization claim reference does not resolve to an exact existing version",
            false,
            "Reference an exact existing claim object and version.",
        )),
    }
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
    use std::collections::HashSet;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn migration_produces_wal_database_with_fts5() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");

        assert_eq!(store.migration_version().expect("migration version"), 5);
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
        assert_eq!(store.migration_version().expect("migration version"), 5);
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
        environment_marker: char,
        artifact_marker: char,
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
                "environment_hash": environment_marker.to_string().repeat(64),
                "module_artifact_hash": artifact_marker.to_string().repeat(64),
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
        let variants = [
            formalization(&claim, "∀ p, Nat.Prime p → Odd p", 'a', 'b', &["Mathlib"]),
            formalization(
                &claim,
                "∀ p, Nat.Prime p → p % 2 = 1",
                'a',
                'b',
                &["Mathlib"],
            ),
            formalization(&claim, "∀ p, Nat.Prime p → Odd p", 'c', 'b', &["Mathlib"]),
            formalization(&claim, "∀ p, Nat.Prime p → Odd p", 'a', 'c', &["Mathlib"]),
            formalization(
                &claim,
                "∀ p, Nat.Prime p → Odd p",
                'a',
                'b',
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
                    &formalization(&missing_claim, "True", 'b', 'c', &["Mathlib"]),
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
                    &formalization(&source, "True", 'b', 'c', &["Mathlib"]),
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
        let created = store
            .create_record(
                &formalization(&claim, "True", 'b', 'c', &["Mathlib"]),
                "formalizer",
                "valid-formalization",
            )
            .expect("valid formalization created");
        let mut invalid_version = formalization(&claim, "True ∧ True", 'b', 'c', &["Mathlib"]);
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
