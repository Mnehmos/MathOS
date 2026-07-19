use std::str::FromStr;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    Store, read_idempotent_result, validate_hash, validate_mutation_inputs, write_idempotent_result,
};
use crate::canonical::{canonical_json, value_hash};
use crate::domain::{
    RunChainReport, RunEventDraft, RunEventKind, RunEventSnapshot, RunKind, RunSnapshot, RunState,
};
use crate::error::AppError;

type RawRunRow = (
    String,
    String,
    String,
    String,
    String,
    i64,
    Option<i64>,
    i64,
    Option<String>,
);

type RawRunEventRow = (
    String,
    String,
    i64,
    String,
    String,
    Option<String>,
    String,
    String,
    i64,
);

impl Store {
    pub fn validate_run_create(&self, budget: &Value) -> Result<(), AppError> {
        canonical_json(budget)?;
        Ok(())
    }

    pub fn create_run(
        &mut self,
        kind: RunKind,
        budget: &Value,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<RunSnapshot, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        let budget_json = canonical_string(budget)?;
        let input_hash = value_hash(&json!({
            "operation": "run.create",
            "kind": kind,
            "budget": budget,
            "actor": actor,
        }))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start run creation", error))?;
        if let Some(existing) =
            read_idempotent_result(&transaction, "run.create", idempotency_key, &input_hash)?
        {
            return Ok(existing);
        }

        let run_id = Uuid::now_v7().to_string();
        transaction
            .execute(
                "INSERT INTO runs(run_id, run_kind, state, actor, budget_json, started_at) VALUES (?1, ?2, 'active', ?3, ?4, unixepoch())",
                params![run_id, kind.as_str(), actor, budget_json],
            )
            .map_err(|error| AppError::database("insert run", error))?;
        insert_event(
            &transaction,
            &run_id,
            1,
            RunEventKind::RunStarted,
            &json!({"run_kind": kind, "budget": budget}),
            None,
            actor,
        )?;
        let snapshot = read_run(&transaction, &run_id)?;
        write_idempotent_result(
            &transaction,
            "run.create",
            idempotency_key,
            &input_hash,
            &snapshot,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit run creation", error))?;
        Ok(snapshot)
    }

    pub fn append_run_event(
        &mut self,
        run_id: &str,
        expected_head_hash: &str,
        draft: &RunEventDraft,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<RunEventSnapshot, AppError> {
        validate_mutation_inputs(actor, idempotency_key)?;
        validate_hash(expected_head_hash, "expected run event head")?;
        validate_append_event_kind(draft.kind)?;
        canonical_json(&draft.payload)?;
        let input_hash = value_hash(&json!({
            "operation": "run.event.append",
            "run_id": run_id,
            "expected_head_hash": expected_head_hash,
            "kind": draft.kind,
            "payload": draft.payload,
            "actor": actor,
        }))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::database("start run event append", error))?;
        if let Some(existing) = read_idempotent_result(
            &transaction,
            "run.event.append",
            idempotency_key,
            &input_hash,
        )? {
            return Ok(existing);
        }

        let run = read_run(&transaction, run_id)?;
        if run.state != RunState::Active {
            return Err(AppError::new(
                "MCL_RUN_NOT_ACTIVE",
                format!("run {run_id} is {}", run.state.as_str()),
                false,
                "Start a new run or use the explicit lifecycle operation allowed for this state.",
            ));
        }
        let actual_head = run.event_head_hash.as_deref().ok_or_else(|| {
            AppError::new(
                "MCL_RUN_CHAIN_EMPTY",
                format!("run {run_id} has no origin event"),
                false,
                "Run integrity verification and restore a verified backup.",
            )
        })?;
        if actual_head != expected_head_hash {
            return Err(run_event_conflict(run_id, expected_head_hash, actual_head));
        }

        let event = insert_event(
            &transaction,
            run_id,
            run.event_count + 1,
            draft.kind,
            &draft.payload,
            Some(actual_head),
            actor,
        )?;
        write_idempotent_result(
            &transaction,
            "run.event.append",
            idempotency_key,
            &input_hash,
            &event,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::database("commit run event append", error))?;
        Ok(event)
    }

    pub fn validate_run_event_append(
        &self,
        run_id: &str,
        expected_head_hash: &str,
        draft: &RunEventDraft,
    ) -> Result<(), AppError> {
        validate_hash(expected_head_hash, "expected run event head")?;
        validate_append_event_kind(draft.kind)?;
        canonical_json(&draft.payload)?;
        let run = self.get_run(run_id)?;
        if run.state != RunState::Active {
            return Err(AppError::new(
                "MCL_RUN_NOT_ACTIVE",
                format!("run {run_id} is {}", run.state.as_str()),
                false,
                "Start a new run or use the explicit lifecycle operation allowed for this state.",
            ));
        }
        let actual_head = run.event_head_hash.as_deref().ok_or_else(|| {
            AppError::new(
                "MCL_RUN_CHAIN_EMPTY",
                format!("run {run_id} has no origin event"),
                false,
                "Run integrity verification and restore a verified backup.",
            )
        })?;
        if actual_head != expected_head_hash {
            return Err(run_event_conflict(run_id, expected_head_hash, actual_head));
        }
        Ok(())
    }

    pub fn get_run(&self, run_id: &str) -> Result<RunSnapshot, AppError> {
        read_run(&self.connection, run_id)
    }

    pub fn list_run_events(&self, run_id: &str) -> Result<Vec<RunEventSnapshot>, AppError> {
        read_run(&self.connection, run_id)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT event_id, run_id, sequence, event_type, payload_json, previous_event_hash, event_hash, actor, created_at FROM run_events WHERE run_id = ?1 ORDER BY sequence",
            )
            .map_err(|error| AppError::database("prepare run event list", error))?;
        let raw = statement
            .query_map([run_id], raw_run_event)
            .map_err(|error| AppError::database("list run events", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::database("read run event list", error))?;
        raw.into_iter().map(decode_run_event).collect()
    }

    pub fn verify_run_chain(&self, run_id: &str) -> Result<RunChainReport, AppError> {
        let run = self.get_run(run_id)?;
        let events = self.list_run_events(run_id)?;
        let mut expected_previous: Option<String> = None;
        let mut first_invalid_sequence = None;

        for (index, event) in events.iter().enumerate() {
            let expected_sequence = index as i64 + 1;
            let recomputed = run_event_hash(
                &event.run_id,
                event.sequence,
                event.kind,
                &event.payload,
                event.previous_event_hash.as_deref(),
                &event.actor,
            )?;
            if event.sequence != expected_sequence
                || event.previous_event_hash != expected_previous
                || event.event_hash != recomputed
            {
                first_invalid_sequence = Some(event.sequence);
                break;
            }
            expected_previous = Some(event.event_hash.clone());
        }

        if first_invalid_sequence.is_none()
            && (run.event_count != events.len() as i64 || run.event_head_hash != expected_previous)
        {
            first_invalid_sequence = Some(if run.event_count > events.len() as i64 {
                events.len() as i64 + 1
            } else {
                run.event_count.max(1)
            });
        }

        Ok(RunChainReport {
            run_id: run_id.to_owned(),
            valid: first_invalid_sequence.is_none(),
            event_count: events.len() as i64,
            head_hash: events.last().map(|event| event.event_hash.clone()),
            first_invalid_sequence,
        })
    }
}

fn validate_append_event_kind(kind: RunEventKind) -> Result<(), AppError> {
    match kind {
        RunEventKind::Observation
        | RunEventKind::ActionSubmitted
        | RunEventKind::OutputObserved
        | RunEventKind::Diagnostic
        | RunEventKind::EvidenceLinked
        | RunEventKind::LeaseChanged => Ok(()),
        RunEventKind::RunStarted
        | RunEventKind::RunFrozen
        | RunEventKind::RunClosed
        | RunEventKind::RunFailed => Err(AppError::new(
            "MCL_RUN_LIFECYCLE_EVENT_RESTRICTED",
            format!(
                "{} is emitted only by its controlled lifecycle operation",
                kind.as_str()
            ),
            false,
            "Use run creation, freeze, close, or failure handling instead of generic append.",
        )),
    }
}

fn insert_event(
    connection: &Connection,
    run_id: &str,
    sequence: i64,
    kind: RunEventKind,
    payload: &Value,
    previous_event_hash: Option<&str>,
    actor: &str,
) -> Result<RunEventSnapshot, AppError> {
    let payload_json = canonical_string(payload)?;
    let event_hash = run_event_hash(run_id, sequence, kind, payload, previous_event_hash, actor)?;
    let event_id = Uuid::now_v7().to_string();
    connection
        .execute(
            "INSERT INTO run_events(event_id, run_id, sequence, event_type, payload_json, previous_event_hash, event_hash, actor, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, unixepoch())",
            params![event_id, run_id, sequence, kind.as_str(), payload_json, previous_event_hash, event_hash, actor],
        )
        .map_err(|error| AppError::database("insert run event", error))?;
    read_run_event(connection, &event_id)
}

fn run_event_hash(
    run_id: &str,
    sequence: i64,
    kind: RunEventKind,
    payload: &Value,
    previous_event_hash: Option<&str>,
    actor: &str,
) -> Result<String, AppError> {
    let envelope = canonical_json(&json!({
        "run_id": run_id,
        "sequence": sequence,
        "event_type": kind,
        "payload": payload,
        "actor": actor,
    }))?;
    let mut digest = Sha256::new();
    if let Some(previous) = previous_event_hash {
        digest.update(previous.as_bytes());
    }
    digest.update(envelope);
    Ok(format!("{:x}", digest.finalize()))
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

fn read_run(connection: &Connection, run_id: &str) -> Result<RunSnapshot, AppError> {
    let raw: Option<RawRunRow> = connection
        .query_row(
            "SELECT run_id, run_kind, state, actor, budget_json, started_at, ended_at, event_count, event_head_hash FROM runs WHERE run_id = ?1",
            [run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?)),
        )
        .optional()
        .map_err(|error| AppError::database("read run", error))?;
    let Some((
        run_id,
        kind,
        state,
        actor,
        budget_json,
        started_at,
        ended_at,
        event_count,
        event_head_hash,
    )) = raw
    else {
        return Err(AppError::new(
            "MCL_RUN_NOT_FOUND",
            format!("run {run_id} does not exist"),
            false,
            "Use an exact run ID returned by the canonical store.",
        ));
    };
    Ok(RunSnapshot {
        run_id,
        kind: RunKind::from_str(&kind)?,
        state: RunState::from_str(&state)?,
        actor,
        budget: decode_json(&budget_json)?,
        started_at,
        ended_at,
        event_count,
        event_head_hash,
    })
}

fn read_run_event(connection: &Connection, event_id: &str) -> Result<RunEventSnapshot, AppError> {
    let raw: Option<RawRunEventRow> = connection
        .query_row(
            "SELECT event_id, run_id, sequence, event_type, payload_json, previous_event_hash, event_hash, actor, created_at FROM run_events WHERE event_id = ?1",
            [event_id],
            raw_run_event,
        )
        .optional()
        .map_err(|error| AppError::database("read run event", error))?;
    raw.map(decode_run_event).transpose()?.ok_or_else(|| {
        AppError::new(
            "MCL_RUN_EVENT_NOT_FOUND",
            format!("run event {event_id} does not exist"),
            false,
            "Use an exact event ID returned by the run history.",
        )
    })
}

fn raw_run_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRunEventRow> {
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
    ))
}

fn decode_run_event(raw: RawRunEventRow) -> Result<RunEventSnapshot, AppError> {
    let (
        event_id,
        run_id,
        sequence,
        kind,
        payload_json,
        previous_event_hash,
        event_hash,
        actor,
        created_at,
    ) = raw;
    Ok(RunEventSnapshot {
        event_id,
        run_id,
        sequence,
        kind: RunEventKind::from_str(&kind)?,
        payload: decode_json(&payload_json)?,
        previous_event_hash,
        event_hash,
        actor,
        created_at,
    })
}

fn decode_json(encoded: &str) -> Result<Value, AppError> {
    serde_json::from_str(encoded).map_err(|error| {
        AppError::new(
            "MCL_CANONICAL_PAYLOAD_INVALID",
            error.to_string(),
            false,
            "Run `mcl doctor` and restore a verified backup if stored state was altered.",
        )
    })
}

fn run_event_conflict(run_id: &str, expected: &str, actual: &str) -> AppError {
    AppError::new(
        "MCL_RUN_EVENT_CONFLICT",
        format!("run {run_id} event head changed: expected {expected}, actual {actual}"),
        true,
        "Reload the event head, reconcile the new history, and retry with a new idempotency key.",
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use tempfile::TempDir;

    use super::*;

    fn open_store(path: &std::path::Path) -> Store {
        let mut store = Store::open(path).expect("database opens");
        store.migrate().expect("migrations apply");
        store
    }

    #[test]
    fn run_starts_with_a_valid_origin_event_and_survives_restart() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let run = {
            let mut store = open_store(&database);
            let created = store
                .create_run(
                    RunKind::Prove,
                    &json!({"max_actions": 12}),
                    "agent-a",
                    "run-create-1",
                )
                .expect("run created");
            let retry = store
                .create_run(
                    RunKind::Prove,
                    &json!({"max_actions": 12}),
                    "agent-a",
                    "run-create-1",
                )
                .expect("run creation retry");
            assert_eq!(retry, created);
            let stored_budget: String = store
                .connection
                .query_row(
                    "SELECT budget_json FROM runs WHERE run_id = ?1",
                    [&created.run_id],
                    |row| row.get(0),
                )
                .expect("stored canonical budget");
            assert_eq!(stored_budget, "{\"max_actions\":12}");
            created
        };

        assert_eq!(run.event_count, 1);
        let reopened = open_store(&database);
        let report = reopened
            .verify_run_chain(&run.run_id)
            .expect("chain verifies after restart");
        assert!(report.valid);
        assert_eq!(report.event_count, 1);
        assert_eq!(report.head_hash, run.event_head_hash);
    }

    #[test]
    fn append_is_idempotent_and_expected_head_is_compare_and_swap() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = open_store(&database);
        let run = store
            .create_run(
                RunKind::CounterexampleSearch,
                &json!({"max_candidates": 100}),
                "agent-a",
                "run-create-2",
            )
            .expect("run created");
        let head = run.event_head_hash.expect("origin head");
        let draft = RunEventDraft {
            kind: RunEventKind::Observation,
            payload: json!({"candidate": 2}),
        };
        let appended = store
            .append_run_event(&run.run_id, &head, &draft, "agent-a", "append-1")
            .expect("event appended");
        let retry = store
            .append_run_event(&run.run_id, &head, &draft, "agent-a", "append-1")
            .expect("idempotent retry");
        assert_eq!(retry, appended);
        assert_eq!(store.list_run_events(&run.run_id).expect("events").len(), 2);

        let conflict = store
            .append_run_event(&run.run_id, &head, &draft, "agent-b", "append-2")
            .expect_err("stale head conflicts");
        assert_eq!(conflict.code, "MCL_RUN_EVENT_CONFLICT");
        assert!(conflict.retryable);
        assert_eq!(store.list_run_events(&run.run_id).expect("events").len(), 2);
        assert_eq!(
            store.get_run(&run.run_id).expect("run").event_head_hash,
            Some(appended.event_hash)
        );
    }

    #[test]
    fn concurrent_appends_from_one_head_have_one_winner() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let run = {
            let mut store = open_store(&database);
            store
                .create_run(
                    RunKind::Formalize,
                    &json!({"max_variants": 2}),
                    "coordinator",
                    "run-create-3",
                )
                .expect("run created")
        };
        let run_id = run.run_id;
        let head = run.event_head_hash.expect("origin head");
        let barrier = Arc::new(Barrier::new(2));
        let handles = (0..2)
            .map(|index| {
                let database = database.clone();
                let run_id = run_id.clone();
                let head = head.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    let mut store = open_store(&database);
                    barrier.wait();
                    store.append_run_event(
                        &run_id,
                        &head,
                        &RunEventDraft {
                            kind: RunEventKind::Observation,
                            payload: json!({"worker": index}),
                        },
                        &format!("agent-{index}"),
                        &format!("concurrent-{index}"),
                    )
                })
            })
            .collect::<Vec<_>>();
        let results = handles
            .into_iter()
            .map(|handle| handle.join().expect("worker joined"))
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .filter(|error| error.code == "MCL_RUN_EVENT_CONFLICT")
                .count(),
            1
        );
        let store = open_store(&database);
        assert_eq!(store.list_run_events(&run_id).expect("events").len(), 2);
        assert!(store.verify_run_chain(&run_id).expect("chain report").valid);
    }

    #[test]
    fn database_rejects_run_event_rewrite_and_deletion() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = open_store(&database);
        let run = store
            .create_run(
                RunKind::Audit,
                &json!({"max_findings": 10}),
                "auditor",
                "run-create-4",
            )
            .expect("run created");
        let event = store
            .list_run_events(&run.run_id)
            .expect("events")
            .remove(0);
        assert!(
            store
                .connection
                .execute(
                    "UPDATE run_events SET payload_json = '{}' WHERE event_id = ?1",
                    [&event.event_id],
                )
                .is_err()
        );
        assert!(
            store
                .connection
                .execute(
                    "DELETE FROM run_events WHERE event_id = ?1",
                    [&event.event_id]
                )
                .is_err()
        );
    }

    #[test]
    fn verification_detects_a_forged_payload() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = open_store(&database);
        let run = store
            .create_run(
                RunKind::LiteratureReview,
                &json!({"max_sources": 5}),
                "reviewer",
                "run-create-5",
            )
            .expect("run created");
        store
            .connection
            .execute_batch("DROP TRIGGER run_events_reject_update;")
            .expect("test removes guard");
        store
            .connection
            .execute(
                "UPDATE run_events SET payload_json = '{\"forged\":true}' WHERE run_id = ?1",
                [&run.run_id],
            )
            .expect("test corrupts payload");

        let report = store
            .verify_run_chain(&run.run_id)
            .expect("verification returns report");
        assert!(!report.valid);
        assert_eq!(report.first_invalid_sequence, Some(1));
    }

    #[test]
    fn verification_detects_a_missing_final_event_against_the_run_anchor() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = open_store(&database);
        let run = store
            .create_run(
                RunKind::Audit,
                &json!({"max_findings": 3}),
                "auditor",
                "run-create-missing",
            )
            .expect("run created");
        let second = store
            .append_run_event(
                &run.run_id,
                run.event_head_hash.as_deref().expect("head"),
                &RunEventDraft {
                    kind: RunEventKind::Diagnostic,
                    payload: json!({"finding": "example"}),
                },
                "auditor",
                "append-missing",
            )
            .expect("event appended");
        store
            .connection
            .execute_batch("DROP TRIGGER run_events_reject_delete;")
            .expect("test removes guard");
        store
            .connection
            .execute(
                "DELETE FROM run_events WHERE event_id = ?1",
                [&second.event_id],
            )
            .expect("test removes final event");

        let report = store
            .verify_run_chain(&run.run_id)
            .expect("verification returns report");
        assert!(!report.valid);
        assert_eq!(report.first_invalid_sequence, Some(2));
    }

    #[test]
    fn verification_detects_reordered_events() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = open_store(&database);
        let run = store
            .create_run(
                RunKind::Generalize,
                &json!({"max_actions": 2}),
                "agent-a",
                "run-create-reorder",
            )
            .expect("run created");
        store
            .append_run_event(
                &run.run_id,
                run.event_head_hash.as_deref().expect("head"),
                &RunEventDraft {
                    kind: RunEventKind::Observation,
                    payload: json!({"candidate": "general theorem"}),
                },
                "agent-a",
                "append-reorder",
            )
            .expect("event appended");
        store
            .connection
            .execute_batch("DROP TRIGGER run_events_reject_update;")
            .expect("test removes guard");
        store
            .connection
            .execute(
                "UPDATE run_events SET sequence = 3 WHERE run_id = ?1 AND sequence = 2",
                [&run.run_id],
            )
            .expect("move second event aside");
        store
            .connection
            .execute(
                "UPDATE run_events SET sequence = 2 WHERE run_id = ?1 AND sequence = 1",
                [&run.run_id],
            )
            .expect("move origin event");
        store
            .connection
            .execute(
                "UPDATE run_events SET sequence = 1 WHERE run_id = ?1 AND sequence = 3",
                [&run.run_id],
            )
            .expect("move second event first");

        let report = store
            .verify_run_chain(&run.run_id)
            .expect("verification returns report");
        assert!(!report.valid);
        assert_eq!(report.first_invalid_sequence, Some(1));
    }

    #[test]
    fn generic_append_cannot_forge_lifecycle_boundaries() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = open_store(&database);
        let run = store
            .create_run(RunKind::Prove, &json!({}), "agent-a", "run-create-6")
            .expect("run created");
        let error = store
            .append_run_event(
                &run.run_id,
                run.event_head_hash.as_deref().expect("head"),
                &RunEventDraft {
                    kind: RunEventKind::RunClosed,
                    payload: json!({}),
                },
                "agent-a",
                "fake-close",
            )
            .expect_err("lifecycle event rejected");
        assert_eq!(error.code, "MCL_RUN_LIFECYCLE_EVENT_RESTRICTED");
    }
}
