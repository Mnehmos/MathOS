CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    applied_at INTEGER NOT NULL
) STRICT;

CREATE TABLE records (
    object_id TEXT PRIMARY KEY,
    record_type TEXT NOT NULL,
    head_version_hash TEXT,
    tombstoned INTEGER NOT NULL DEFAULT 0 CHECK (tombstoned IN (0, 1)),
    created_at INTEGER NOT NULL,
    created_by TEXT NOT NULL
) STRICT;

CREATE TABLE record_versions (
    version_hash TEXT PRIMARY KEY CHECK (length(version_hash) = 64),
    object_id TEXT NOT NULL REFERENCES records(object_id),
    schema_version TEXT NOT NULL,
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    predecessor_hash TEXT REFERENCES record_versions(version_hash),
    created_at INTEGER NOT NULL,
    created_by TEXT NOT NULL,
    UNIQUE(object_id, predecessor_hash)
) STRICT;

CREATE INDEX record_versions_object ON record_versions(object_id, created_at);

CREATE TABLE edges (
    edge_id TEXT PRIMARY KEY,
    edge_type TEXT NOT NULL,
    source_object_id TEXT NOT NULL REFERENCES records(object_id),
    source_version_hash TEXT NOT NULL REFERENCES record_versions(version_hash),
    target_object_id TEXT NOT NULL REFERENCES records(object_id),
    target_version_hash TEXT NOT NULL REFERENCES record_versions(version_hash),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    created_at INTEGER NOT NULL,
    created_by TEXT NOT NULL
) STRICT;

CREATE TABLE artifacts (
    artifact_hash TEXT PRIMARY KEY CHECK (length(artifact_hash) = 64),
    media_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
    creation_source TEXT NOT NULL,
    license_expression TEXT,
    restriction TEXT,
    metadata_json TEXT NOT NULL CHECK (json_valid(metadata_json)),
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE runs (
    run_id TEXT PRIMARY KEY,
    run_kind TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('active', 'frozen', 'closed', 'failed')),
    actor TEXT NOT NULL,
    budget_json TEXT NOT NULL CHECK (json_valid(budget_json)),
    started_at INTEGER NOT NULL,
    ended_at INTEGER
) STRICT;

CREATE TABLE run_events (
    event_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(run_id),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    previous_event_hash TEXT,
    event_hash TEXT NOT NULL UNIQUE CHECK (length(event_hash) = 64),
    actor TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(run_id, sequence)
) STRICT;

CREATE TABLE environments (
    environment_hash TEXT PRIMARY KEY CHECK (length(environment_hash) = 64),
    manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
    trust_profile TEXT NOT NULL CHECK (trust_profile IN ('local', 'publication')),
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE evidence (
    evidence_id TEXT PRIMARY KEY,
    subject_object_id TEXT NOT NULL REFERENCES records(object_id),
    subject_version_hash TEXT NOT NULL REFERENCES record_versions(version_hash),
    evidence_kind TEXT NOT NULL,
    result TEXT NOT NULL,
    authority_class TEXT NOT NULL,
    run_id TEXT REFERENCES runs(run_id),
    environment_hash TEXT REFERENCES environments(environment_hash),
    artifact_hash TEXT REFERENCES artifacts(artifact_hash),
    metadata_json TEXT NOT NULL CHECK (json_valid(metadata_json)),
    created_at INTEGER NOT NULL,
    superseded_by TEXT REFERENCES evidence(evidence_id)
) STRICT;

CREATE TABLE jobs (
    job_id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL,
    canonical_input_hash TEXT NOT NULL CHECK (length(canonical_input_hash) = 64),
    idempotency_key TEXT NOT NULL UNIQUE,
    state TEXT NOT NULL CHECK (state IN ('queued', 'leased', 'running', 'succeeded', 'failed', 'cancelled', 'blocked')),
    priority INTEGER NOT NULL DEFAULT 0,
    lease_owner TEXT,
    lease_expires_at INTEGER,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    progress_json TEXT NOT NULL CHECK (json_valid(progress_json)),
    result_artifact_hash TEXT REFERENCES artifacts(artifact_hash),
    last_error_json TEXT CHECK (last_error_json IS NULL OR json_valid(last_error_json)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE INDEX jobs_dispatch ON jobs(state, priority DESC, created_at);

CREATE TABLE releases (
    release_id TEXT PRIMARY KEY,
    manifest_hash TEXT NOT NULL UNIQUE CHECK (length(manifest_hash) = 64),
    state TEXT NOT NULL CHECK (state IN ('preview', 'built', 'verified', 'released', 'retracted')),
    manifest_artifact_hash TEXT NOT NULL REFERENCES artifacts(artifact_hash),
    created_at INTEGER NOT NULL,
    verified_at INTEGER
) STRICT;

CREATE VIRTUAL TABLE record_search USING fts5(
    object_id UNINDEXED,
    record_type,
    searchable_text,
    tokenize = 'unicode61'
);
