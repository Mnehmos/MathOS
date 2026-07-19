use std::fs;
use std::path::Path;
use std::str::FromStr;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::canonical::{canonical_json, record_version_hash, value_hash};
use crate::domain::{RecordDraft, RecordKind, RecordSnapshot};
use crate::error::AppError;

const MIGRATION_0001: &str = include_str!("../../migrations/0001_initial.sql");
const MIGRATION_0002: &str = include_str!("../../migrations/0002_idempotency.sql");
const MIGRATION_0003: &str = include_str!("../../migrations/0003_record_invariants.sql");
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

fn read_idempotent_result(
    connection: &Connection,
    operation: &str,
    key: &str,
    input_hash: &str,
) -> Result<Option<RecordSnapshot>, AppError> {
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

fn write_idempotent_result(
    connection: &Connection,
    operation: &str,
    key: &str,
    input_hash: &str,
    result: &RecordSnapshot,
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

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn migration_produces_wal_database_with_fts5() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = Store::open(&database).expect("database opens");
        store.migrate().expect("migration succeeds");

        assert_eq!(store.migration_version().expect("migration version"), 3);
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
        assert_eq!(store.migration_version().expect("migration version"), 3);
    }

    fn claim(statement: &str) -> RecordDraft {
        RecordDraft {
            kind: RecordKind::Claim,
            schema_version: "claim/1".to_owned(),
            payload: json!({"statement": statement}),
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
}
