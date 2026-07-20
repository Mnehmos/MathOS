CREATE TABLE publication_stages (
    stage_hash TEXT PRIMARY KEY
        CHECK (length(stage_hash) = 64 AND stage_hash NOT GLOB '*[^0-9a-f]*'),
    schema_version TEXT NOT NULL
        CHECK (schema_version = 'publication_stage/1'),
    report_artifact_hash TEXT NOT NULL
        CHECK (length(report_artifact_hash) = 64 AND report_artifact_hash NOT GLOB '*[^0-9a-f]*'),
    report_byte_size INTEGER NOT NULL
        CHECK (report_byte_size BETWEEN 1 AND 16777216),
    retained_closure_artifact_hash TEXT NOT NULL
        CHECK (length(retained_closure_artifact_hash) = 64 AND retained_closure_artifact_hash NOT GLOB '*[^0-9a-f]*'),
    retained_closure_byte_size INTEGER NOT NULL
        CHECK (retained_closure_byte_size BETWEEN 1 AND 16777216),
    attestation_bundle_artifact_hash TEXT NOT NULL
        CHECK (length(attestation_bundle_artifact_hash) = 64 AND attestation_bundle_artifact_hash NOT GLOB '*[^0-9a-f]*'),
    attestation_bundle_byte_size INTEGER NOT NULL
        CHECK (attestation_bundle_byte_size BETWEEN 1 AND 16777216),
    retained_artifact_count INTEGER NOT NULL
        CHECK (retained_artifact_count = 25),
    stage_json TEXT NOT NULL
        CHECK (
            json_valid(stage_json)
            AND json_type(stage_json) = 'object'
            AND json_extract(stage_json, '$.schema_version') IS schema_version
            AND json_extract(stage_json, '$.report_artifact_hash') IS report_artifact_hash
            AND json_extract(stage_json, '$.report_byte_size') IS report_byte_size
            AND json_extract(stage_json, '$.retained_closure_artifact_hash') IS retained_closure_artifact_hash
            AND json_extract(stage_json, '$.retained_closure_byte_size') IS retained_closure_byte_size
            AND json_extract(stage_json, '$.attestation_bundle_artifact_hash') IS attestation_bundle_artifact_hash
            AND json_extract(stage_json, '$.attestation_bundle_byte_size') IS attestation_bundle_byte_size
            AND json_type(stage_json, '$.retained_artifacts') = 'array'
            AND json_array_length(stage_json, '$.retained_artifacts') IS retained_artifact_count
            AND json_type(stage_json, '$.authoritative') = 'false'
        ),
    authoritative INTEGER NOT NULL
        CHECK (authoritative = 0),
    created_at INTEGER NOT NULL
        CHECK (created_at >= 0),
    created_by TEXT NOT NULL
        CHECK (length(trim(created_by)) BETWEEN 1 AND 256),
    UNIQUE (report_artifact_hash, attestation_bundle_artifact_hash),
    UNIQUE (stage_hash, report_artifact_hash, attestation_bundle_artifact_hash)
) STRICT;

CREATE INDEX publication_stages_created
ON publication_stages(created_at, stage_hash);

CREATE TABLE publication_ingestion_receipts (
    receipt_hash TEXT PRIMARY KEY
        CHECK (length(receipt_hash) = 64 AND receipt_hash NOT GLOB '*[^0-9a-f]*'),
    schema_version TEXT NOT NULL
        CHECK (schema_version = 'publication_attestation_verification/1'),
    stage_hash TEXT NOT NULL
        CHECK (length(stage_hash) = 64 AND stage_hash NOT GLOB '*[^0-9a-f]*'),
    report_artifact_hash TEXT NOT NULL
        CHECK (length(report_artifact_hash) = 64 AND report_artifact_hash NOT GLOB '*[^0-9a-f]*'),
    attestation_bundle_artifact_hash TEXT NOT NULL
        CHECK (length(attestation_bundle_artifact_hash) = 64 AND attestation_bundle_artifact_hash NOT GLOB '*[^0-9a-f]*'),
    raw_verification_hash TEXT NOT NULL
        CHECK (length(raw_verification_hash) = 64 AND raw_verification_hash NOT GLOB '*[^0-9a-f]*'),
    raw_verification_byte_size INTEGER NOT NULL
        CHECK (raw_verification_byte_size BETWEEN 1 AND 16777216),
    receipt_byte_size INTEGER NOT NULL
        CHECK (receipt_byte_size BETWEEN 1 AND 16777216),
    verification_json TEXT NOT NULL
        CHECK (
            json_valid(verification_json)
            AND json_type(verification_json) = 'object'
            AND json_extract(verification_json, '$.schema_version') IS schema_version
            AND json_extract(verification_json, '$.report_content_hash') IS report_artifact_hash
            AND json_extract(verification_json, '$.report_artifact_hash') IS report_artifact_hash
            AND json_extract(verification_json, '$.attestation_bundle_hash') IS attestation_bundle_artifact_hash
            AND json_extract(verification_json, '$.raw_verification_hash') IS raw_verification_hash
            AND json_type(verification_json, '$.authoritative') = 'false'
        ),
    authoritative INTEGER NOT NULL
        CHECK (authoritative = 0),
    created_at INTEGER NOT NULL
        CHECK (created_at >= 0),
    created_by TEXT NOT NULL
        CHECK (length(trim(created_by)) BETWEEN 1 AND 256),
    UNIQUE (stage_hash),
    UNIQUE (report_artifact_hash, attestation_bundle_artifact_hash),
    FOREIGN KEY (stage_hash)
        REFERENCES publication_stages(stage_hash),
    FOREIGN KEY (stage_hash, report_artifact_hash, attestation_bundle_artifact_hash)
        REFERENCES publication_stages(stage_hash, report_artifact_hash, attestation_bundle_artifact_hash)
) STRICT;

CREATE INDEX publication_ingestion_receipts_created
ON publication_ingestion_receipts(created_at, receipt_hash);

CREATE TRIGGER publication_stages_reject_update
BEFORE UPDATE ON publication_stages
BEGIN
    SELECT RAISE(ABORT, 'publication stages are immutable');
END;

CREATE TRIGGER publication_stages_reject_delete
BEFORE DELETE ON publication_stages
BEGIN
    SELECT RAISE(ABORT, 'publication stages are durable history');
END;

CREATE TRIGGER publication_ingestion_receipts_reject_update
BEFORE UPDATE ON publication_ingestion_receipts
BEGIN
    SELECT RAISE(ABORT, 'publication ingestion receipts are immutable');
END;

CREATE TRIGGER publication_ingestion_receipts_reject_delete
BEFORE DELETE ON publication_ingestion_receipts
BEGIN
    SELECT RAISE(ABORT, 'publication ingestion receipts are durable history');
END;
