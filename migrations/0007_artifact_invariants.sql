ALTER TABLE artifacts
    ADD COLUMN created_by TEXT NOT NULL DEFAULT 'migration:unknown';

CREATE TRIGGER artifacts_reject_update
BEFORE UPDATE ON artifacts
BEGIN
    SELECT RAISE(ABORT, 'artifacts are immutable');
END;

CREATE TRIGGER artifacts_reject_delete
BEFORE DELETE ON artifacts
BEGIN
    SELECT RAISE(ABORT, 'artifacts are immutable');
END;
