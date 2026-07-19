ALTER TABLE evidence
    ADD COLUMN evidence_hash TEXT CHECK (evidence_hash IS NULL OR length(evidence_hash) = 64);

ALTER TABLE evidence
    ADD COLUMN job_id TEXT REFERENCES jobs(job_id);

ALTER TABLE evidence
    ADD COLUMN artifact_hashes_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(artifact_hashes_json) AND json_type(artifact_hashes_json) = 'array');

ALTER TABLE evidence
    ADD COLUMN verifier_identity TEXT NOT NULL DEFAULT 'migration:unknown';

ALTER TABLE evidence
    ADD COLUMN created_by TEXT NOT NULL DEFAULT 'migration:unknown';

ALTER TABLE evidence
    ADD COLUMN stale_reason TEXT;

CREATE UNIQUE INDEX evidence_content_identity
ON evidence(evidence_hash)
WHERE evidence_hash IS NOT NULL;

CREATE TRIGGER evidence_reject_subject_mismatch_insert
BEFORE INSERT ON evidence
WHEN NOT EXISTS (
    SELECT 1 FROM record_versions
    WHERE version_hash = NEW.subject_version_hash
      AND object_id = NEW.subject_object_id
)
BEGIN
    SELECT RAISE(ABORT, 'evidence subject version does not belong to object');
END;

CREATE TRIGGER evidence_reject_update
BEFORE UPDATE ON evidence
BEGIN
    SELECT RAISE(ABORT, 'evidence is immutable');
END;

CREATE TRIGGER evidence_reject_delete
BEFORE DELETE ON evidence
BEGIN
    SELECT RAISE(ABORT, 'evidence is durable history');
END;
