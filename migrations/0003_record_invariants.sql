CREATE TRIGGER record_versions_reject_update
BEFORE UPDATE ON record_versions
BEGIN
    SELECT RAISE(ABORT, 'record versions are immutable');
END;

CREATE TRIGGER record_versions_reject_delete
BEFORE DELETE ON record_versions
BEGIN
    SELECT RAISE(ABORT, 'record versions are immutable');
END;

CREATE TRIGGER records_reject_identity_update
BEFORE UPDATE OF object_id, record_type, created_at, created_by ON records
BEGIN
    SELECT RAISE(ABORT, 'record identity fields are immutable');
END;

CREATE TRIGGER records_require_owned_head
BEFORE UPDATE OF head_version_hash ON records
WHEN NEW.head_version_hash IS NOT NULL
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1
        FROM record_versions
        WHERE version_hash = NEW.head_version_hash
          AND object_id = NEW.object_id
    ) THEN RAISE(ABORT, 'record head must reference a version owned by the object') END;
END;

CREATE TRIGGER records_reject_head_clear
BEFORE UPDATE OF head_version_hash ON records
WHEN OLD.head_version_hash IS NOT NULL AND NEW.head_version_hash IS NULL
BEGIN
    SELECT RAISE(ABORT, 'record head cannot be cleared');
END;

CREATE TRIGGER records_require_successor_head
BEFORE UPDATE OF head_version_hash ON records
WHEN OLD.head_version_hash IS NOT NULL
 AND NEW.head_version_hash IS NOT OLD.head_version_hash
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1
        FROM record_versions
        WHERE version_hash = NEW.head_version_hash
          AND object_id = NEW.object_id
          AND predecessor_hash = OLD.head_version_hash
    ) THEN RAISE(ABORT, 'record head must advance to a direct successor') END;
END;

CREATE TRIGGER idempotency_results_reject_update
BEFORE UPDATE ON idempotency_results
BEGIN
    SELECT RAISE(ABORT, 'idempotency results are immutable');
END;

CREATE TRIGGER idempotency_results_reject_delete
BEFORE DELETE ON idempotency_results
BEGIN
    SELECT RAISE(ABORT, 'idempotency results are immutable');
END;
