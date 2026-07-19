ALTER TABLE jobs
    ADD COLUMN input_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(input_json));

ALTER TABLE jobs
    ADD COLUMN actor TEXT NOT NULL DEFAULT 'migration:unknown';

CREATE TRIGGER jobs_reject_identity_rewrite
BEFORE UPDATE ON jobs
WHEN NEW.job_id <> OLD.job_id
  OR NEW.job_type <> OLD.job_type
  OR NEW.canonical_input_hash <> OLD.canonical_input_hash
  OR NEW.idempotency_key <> OLD.idempotency_key
  OR NEW.input_json <> OLD.input_json
  OR NEW.priority <> OLD.priority
  OR NEW.actor <> OLD.actor
  OR NEW.created_at <> OLD.created_at
BEGIN
    SELECT RAISE(ABORT, 'job identity is immutable');
END;

CREATE TRIGGER jobs_reject_invalid_state_transition
BEFORE UPDATE OF state ON jobs
WHEN NOT (
    NEW.state = OLD.state
    OR (OLD.state = 'queued' AND NEW.state IN ('leased', 'cancelled', 'blocked'))
    OR (OLD.state = 'leased' AND NEW.state IN ('running', 'queued', 'cancelled', 'blocked'))
    OR (OLD.state = 'running' AND NEW.state IN ('succeeded', 'failed', 'queued', 'cancelled', 'blocked'))
    OR (OLD.state = 'blocked' AND NEW.state IN ('queued', 'cancelled'))
)
BEGIN
    SELECT RAISE(ABORT, 'invalid job state transition');
END;

CREATE TRIGGER jobs_reject_terminal_rewrite
BEFORE UPDATE ON jobs
WHEN OLD.state IN ('succeeded', 'failed', 'cancelled')
BEGIN
    SELECT RAISE(ABORT, 'terminal jobs are immutable');
END;

CREATE TRIGGER jobs_reject_invalid_attempt_change
BEFORE UPDATE OF attempt_count ON jobs
WHEN NEW.attempt_count <> OLD.attempt_count
 AND NOT (
    OLD.state = 'queued'
    AND NEW.state = 'leased'
    AND NEW.attempt_count = OLD.attempt_count + 1
 )
BEGIN
    SELECT RAISE(ABORT, 'job attempt count may advance only during lease');
END;

CREATE TRIGGER jobs_reject_invalid_lease_shape
BEFORE UPDATE ON jobs
WHEN (
    NEW.state IN ('leased', 'running')
    AND (NEW.lease_owner IS NULL OR NEW.lease_expires_at IS NULL)
) OR (
    NEW.state NOT IN ('leased', 'running')
    AND (NEW.lease_owner IS NOT NULL OR NEW.lease_expires_at IS NOT NULL)
)
BEGIN
    SELECT RAISE(ABORT, 'job lease fields do not match state');
END;

CREATE TRIGGER jobs_reject_invalid_result_shape
BEFORE UPDATE ON jobs
WHEN (NEW.state IN ('succeeded', 'failed') AND NEW.result_artifact_hash IS NULL)
  OR (NEW.state NOT IN ('succeeded', 'failed') AND NEW.result_artifact_hash IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'job result artifact does not match state');
END;

CREATE TRIGGER jobs_reject_delete
BEFORE DELETE ON jobs
BEGIN
    SELECT RAISE(ABORT, 'jobs are durable history');
END;
