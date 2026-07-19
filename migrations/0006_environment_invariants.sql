ALTER TABLE environments
ADD COLUMN created_by TEXT NOT NULL DEFAULT 'legacy-unattributed';

CREATE TRIGGER environments_reject_update
BEFORE UPDATE ON environments
BEGIN
    SELECT RAISE(ABORT, 'environments are immutable');
END;

CREATE TRIGGER environments_reject_delete
BEFORE DELETE ON environments
BEGIN
    SELECT RAISE(ABORT, 'environments are immutable');
END;
