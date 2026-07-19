ALTER TABLE runs ADD COLUMN event_count INTEGER NOT NULL DEFAULT 0 CHECK (event_count >= 0);
ALTER TABLE runs ADD COLUMN event_head_hash TEXT CHECK (
    event_head_hash IS NULL OR length(event_head_hash) = 64
);

UPDATE runs
SET event_count = (
        SELECT COUNT(*) FROM run_events WHERE run_id = runs.run_id
    ),
    event_head_hash = (
        SELECT event_hash
        FROM run_events
        WHERE run_id = runs.run_id
        ORDER BY sequence DESC
        LIMIT 1
    );

CREATE TRIGGER runs_reject_identity_update
BEFORE UPDATE OF run_id, run_kind, actor, budget_json, started_at ON runs
BEGIN
    SELECT RAISE(ABORT, 'run identity and origin are immutable');
END;

CREATE TRIGGER runs_reject_delete
BEFORE DELETE ON runs
BEGIN
    SELECT RAISE(ABORT, 'runs are durable history');
END;

CREATE TRIGGER run_events_reject_update
BEFORE UPDATE ON run_events
BEGIN
    SELECT RAISE(ABORT, 'run events are immutable');
END;

CREATE TRIGGER run_events_reject_delete
BEFORE DELETE ON run_events
BEGIN
    SELECT RAISE(ABORT, 'run events are durable history');
END;

CREATE TRIGGER run_events_require_known_type
BEFORE INSERT ON run_events
WHEN NEW.event_type NOT IN (
    'run_started',
    'observation',
    'action_submitted',
    'output_observed',
    'diagnostic',
    'evidence_linked',
    'lease_changed',
    'run_frozen',
    'run_closed',
    'run_failed'
)
BEGIN
    SELECT RAISE(ABORT, 'unknown run event type');
END;

CREATE TRIGGER run_events_require_contiguous_chain
BEFORE INSERT ON run_events
BEGIN
    SELECT CASE WHEN NEW.sequence != (
        SELECT event_count + 1 FROM runs WHERE run_id = NEW.run_id
    ) THEN RAISE(ABORT, 'run event sequence is not contiguous') END;

    SELECT CASE WHEN (
        SELECT event_count FROM runs WHERE run_id = NEW.run_id
    ) = 0 AND NEW.previous_event_hash IS NOT NULL
    THEN RAISE(ABORT, 'first run event must not have a predecessor') END;

    SELECT CASE WHEN (
        SELECT event_count FROM runs WHERE run_id = NEW.run_id
    ) > 0 AND NEW.previous_event_hash IS NOT (
        SELECT event_head_hash FROM runs WHERE run_id = NEW.run_id
    ) THEN RAISE(ABORT, 'run event predecessor does not match the current head') END;
END;

CREATE TRIGGER run_events_advance_anchor
AFTER INSERT ON run_events
BEGIN
    UPDATE runs
    SET event_count = NEW.sequence,
        event_head_hash = NEW.event_hash
    WHERE run_id = NEW.run_id;
END;
