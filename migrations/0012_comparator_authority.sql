CREATE TABLE comparator_authority_stages (
    stage_hash TEXT PRIMARY KEY
        CHECK (length(stage_hash) = 64 AND stage_hash NOT GLOB '*[^0-9a-f]*'),
    schema_version TEXT NOT NULL
        CHECK (schema_version = 'comparator_authority_stage/1'),
    report_artifact_hash TEXT NOT NULL
        CHECK (length(report_artifact_hash) = 64 AND report_artifact_hash NOT GLOB '*[^0-9a-f]*'),
    package_verification_hash TEXT NOT NULL
        CHECK (length(package_verification_hash) = 64 AND package_verification_hash NOT GLOB '*[^0-9a-f]*'),
    package_input_fingerprint TEXT NOT NULL
        CHECK (length(package_input_fingerprint) = 64 AND package_input_fingerprint NOT GLOB '*[^0-9a-f]*'),
    plan_artifact_hash TEXT NOT NULL
        CHECK (length(plan_artifact_hash) = 64 AND plan_artifact_hash NOT GLOB '*[^0-9a-f]*'),
    source_release_manifest_hash TEXT NOT NULL
        CHECK (length(source_release_manifest_hash) = 64 AND source_release_manifest_hash NOT GLOB '*[^0-9a-f]*'),
    attestation_bundle_artifact_hash TEXT NOT NULL
        CHECK (length(attestation_bundle_artifact_hash) = 64 AND attestation_bundle_artifact_hash NOT GLOB '*[^0-9a-f]*'),
    policy_hash TEXT NOT NULL
        CHECK (length(policy_hash) = 64 AND policy_hash NOT GLOB '*[^0-9a-f]*'),
    subject_object_id TEXT NOT NULL
        REFERENCES records(object_id),
    subject_version_hash TEXT NOT NULL
        REFERENCES record_versions(version_hash)
        CHECK (length(subject_version_hash) = 64 AND subject_version_hash NOT GLOB '*[^0-9a-f]*'),
    source_commit_sha TEXT NOT NULL
        CHECK (length(source_commit_sha) = 40 AND source_commit_sha NOT GLOB '*[^0-9a-f]*'),
    workflow_run_id TEXT NOT NULL
        CHECK (length(workflow_run_id) BETWEEN 1 AND 32 AND workflow_run_id NOT GLOB '*[^0-9]*'),
    workflow_run_attempt INTEGER NOT NULL
        CHECK (workflow_run_attempt BETWEEN 1 AND 4294967295),
    artifact_count INTEGER NOT NULL
        CHECK (artifact_count BETWEEN 21 AND 256),
    stage_json TEXT NOT NULL
        CHECK (
            json_valid(stage_json)
            AND json_type(stage_json) = 'object'
            AND json_extract(stage_json, '$.schema_version') IS schema_version
            AND json_extract(stage_json, '$.report_artifact_hash') IS report_artifact_hash
            AND json_extract(stage_json, '$.package_verification_hash') IS package_verification_hash
            AND json_extract(stage_json, '$.package_input_fingerprint') IS package_input_fingerprint
            AND json_extract(stage_json, '$.plan_artifact_hash') IS plan_artifact_hash
            AND json_extract(stage_json, '$.source_release_manifest_hash') IS source_release_manifest_hash
            AND json_extract(stage_json, '$.attestation_bundle_artifact_hash') IS attestation_bundle_artifact_hash
            AND json_extract(stage_json, '$.policy_hash') IS policy_hash
            AND json_extract(stage_json, '$.source_formalization.object_id') IS subject_object_id
            AND json_extract(stage_json, '$.source_formalization.version_hash') IS subject_version_hash
            AND json_extract(stage_json, '$.source_commit_sha') IS source_commit_sha
            AND json_extract(stage_json, '$.workflow_run_id') IS workflow_run_id
            AND json_extract(stage_json, '$.workflow_run_attempt') IS workflow_run_attempt
            AND json_type(stage_json, '$.artifacts') = 'array'
            AND json_array_length(stage_json, '$.artifacts') IS artifact_count
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

CREATE INDEX comparator_authority_stages_created
ON comparator_authority_stages(created_at, stage_hash);

CREATE TABLE comparator_ingestion_receipts (
    receipt_hash TEXT PRIMARY KEY
        CHECK (length(receipt_hash) = 64 AND receipt_hash NOT GLOB '*[^0-9a-f]*'),
    schema_version TEXT NOT NULL
        CHECK (schema_version = 'comparator_attestation_verification/1'),
    stage_hash TEXT NOT NULL
        REFERENCES comparator_authority_stages(stage_hash),
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
            AND json_extract(verification_json, '$.stage_hash') IS stage_hash
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
    FOREIGN KEY (stage_hash, report_artifact_hash, attestation_bundle_artifact_hash)
        REFERENCES comparator_authority_stages(stage_hash, report_artifact_hash, attestation_bundle_artifact_hash)
) STRICT;

CREATE INDEX comparator_ingestion_receipts_created
ON comparator_ingestion_receipts(created_at, receipt_hash);

ALTER TABLE evidence
    ADD COLUMN comparator_receipt_hash TEXT
        REFERENCES comparator_ingestion_receipts(receipt_hash)
        CHECK (
            comparator_receipt_hash IS NULL
            OR (
                length(comparator_receipt_hash) = 64
                AND comparator_receipt_hash NOT GLOB '*[^0-9a-f]*'
            )
        );

ALTER TABLE evidence
    ADD COLUMN comparator_stage_hash TEXT
        REFERENCES comparator_authority_stages(stage_hash)
        CHECK (
            comparator_stage_hash IS NULL
            OR (
                length(comparator_stage_hash) = 64
                AND comparator_stage_hash NOT GLOB '*[^0-9a-f]*'
            )
        );

CREATE UNIQUE INDEX evidence_comparator_receipt_identity
ON evidence(comparator_receipt_hash)
WHERE comparator_receipt_hash IS NOT NULL;

CREATE TRIGGER comparator_authority_stages_require_current_subject
BEFORE INSERT ON comparator_authority_stages
WHEN NOT EXISTS (
    SELECT 1
    FROM records AS record
    JOIN record_versions AS version
      ON version.object_id = record.object_id
     AND version.version_hash = NEW.subject_version_hash
    WHERE record.object_id = NEW.subject_object_id
      AND record.record_type = 'formalization'
      AND record.tombstoned = 0
      AND record.head_version_hash = NEW.subject_version_hash
)
BEGIN
    SELECT RAISE(ABORT, 'Comparator stage subject must be the current exact formalization');
END;

CREATE TRIGGER comparator_authority_stages_reject_update
BEFORE UPDATE ON comparator_authority_stages
BEGIN
    SELECT RAISE(ABORT, 'Comparator authority stages are immutable');
END;

CREATE TRIGGER comparator_authority_stages_reject_delete
BEFORE DELETE ON comparator_authority_stages
BEGIN
    SELECT RAISE(ABORT, 'Comparator authority stages are durable history');
END;

CREATE TRIGGER comparator_ingestion_receipts_reject_update
BEFORE UPDATE ON comparator_ingestion_receipts
BEGIN
    SELECT RAISE(ABORT, 'Comparator ingestion receipts are immutable');
END;

CREATE TRIGGER comparator_ingestion_receipts_reject_delete
BEFORE DELETE ON comparator_ingestion_receipts
BEGIN
    SELECT RAISE(ABORT, 'Comparator ingestion receipts are durable history');
END;


DROP TRIGGER evidence_reject_invalid_publication_authority_insert;

CREATE TRIGGER evidence_reject_invalid_publication_authority_insert
BEFORE INSERT ON evidence
WHEN (
    NEW.evidence_kind IN ('lean_kernel_proof', 'lean_kernel_refutation')
    OR NEW.authority_class = 'authoritative'
    OR NEW.publication_receipt_hash IS NOT NULL
    OR NEW.publication_stage_hash IS NOT NULL
    OR json_extract(NEW.metadata_json, '$.schema_version') = 'evidence/2'
    OR json_type(NEW.metadata_json, '$.publication_authority') IS NOT NULL
)
AND NOT (
    (
        NEW.evidence_kind = 'comparator_run'
        AND NEW.result = 'accepted'
        AND NEW.authority_class = 'authoritative'
        AND NEW.comparator_receipt_hash IS NOT NULL
        AND NEW.comparator_stage_hash IS NOT NULL
        AND NEW.publication_receipt_hash IS NULL
        AND NEW.publication_stage_hash IS NULL
        AND json_extract(NEW.metadata_json, '$.schema_version') = 'evidence/3'
        AND json_type(NEW.metadata_json, '$.comparator_authority') = 'object'
    )
    OR (
        NEW.evidence_kind IN ('lean_kernel_proof', 'lean_kernel_refutation')
    AND NEW.result = 'accepted'
    AND NEW.authority_class = 'authoritative'
    AND NEW.run_id IS NULL
    AND NEW.job_id IS NULL
    AND NEW.artifact_hash IS NULL
    AND NEW.environment_hash IS NOT NULL
    AND NEW.superseded_by IS NULL
    AND NEW.stale_reason IS NULL
    AND NEW.publication_receipt_hash IS NOT NULL
    AND NEW.publication_stage_hash IS NOT NULL
    AND NEW.evidence_hash IS NOT NULL
    AND length(NEW.evidence_hash) = 64
    AND NEW.evidence_hash NOT GLOB '*[^0-9a-f]*'
    AND NEW.created_at >= 0
    AND length(trim(NEW.created_by)) BETWEEN 1 AND 256
    AND json_valid(NEW.metadata_json)
    AND json_type(NEW.metadata_json) = 'object'
    AND 14 = (SELECT count(*) FROM json_each(NEW.metadata_json))
    AND json_extract(NEW.metadata_json, '$.schema_version') = 'evidence/2'
    AND json_type(NEW.metadata_json, '$.subject') = 'object'
    AND 2 = (
        SELECT count(*)
        FROM json_each(NEW.metadata_json, '$.subject')
    )
    AND json_extract(NEW.metadata_json, '$.subject.object_id') IS NEW.subject_object_id
    AND json_extract(NEW.metadata_json, '$.subject.version_hash') IS NEW.subject_version_hash
    AND json_extract(NEW.metadata_json, '$.evidence_kind') IS NEW.evidence_kind
    AND json_extract(NEW.metadata_json, '$.result') IS NEW.result
    AND json_extract(NEW.metadata_json, '$.authority_class') IS NEW.authority_class
    AND json_type(NEW.metadata_json, '$.producing_run_id') = 'null'
    AND json_type(NEW.metadata_json, '$.producing_job_id') = 'null'
    AND json_extract(NEW.metadata_json, '$.environment_hash') IS NEW.environment_hash
    AND json_type(NEW.metadata_json, '$.supersedes_evidence_id') = 'null'
    AND json_type(NEW.metadata_json, '$.stale') = 'false'
    AND json_type(NEW.metadata_json, '$.stale_reason') = 'null'
    AND json_type(NEW.metadata_json, '$.artifact_hashes') = 'array'
    AND json_extract(NEW.metadata_json, '$.artifact_hashes')
        = json(NEW.artifact_hashes_json)
    AND NOT EXISTS (
        SELECT 1
        FROM json_each(NEW.artifact_hashes_json) AS artifact
        WHERE artifact.type != 'text'
           OR length(artifact.value) != 64
           OR artifact.value GLOB '*[^0-9a-f]*'
    )
    AND NOT EXISTS (
        SELECT 1
        FROM json_each(NEW.artifact_hashes_json) AS later
        JOIN json_each(NEW.artifact_hashes_json) AS earlier
          ON CAST(earlier.key AS INTEGER) < CAST(later.key AS INTEGER)
        WHERE earlier.value >= later.value
    )
    AND json_type(NEW.metadata_json, '$.publication_authority') = 'object'
    AND 9 = (
        SELECT count(*)
        FROM json_each(NEW.metadata_json, '$.publication_authority')
    )
    AND json_extract(
        NEW.metadata_json,
        '$.publication_authority.schema_version'
    ) = 'publication_authority_binding/1'
    AND json_extract(
        NEW.metadata_json,
        '$.publication_authority.ingestion_receipt_hash'
    ) IS NEW.publication_receipt_hash
    AND json_extract(
        NEW.metadata_json,
        '$.publication_authority.stage_hash'
    ) IS NEW.publication_stage_hash
    AND NEW.verifier_identity = (
        'publication-policy:' || json_extract(
            NEW.metadata_json,
            '$.publication_authority.publication_policy_hash'
        )
    )
    AND json_extract(
        NEW.metadata_json,
        '$.verifier_or_reviewer_identity'
    ) IS NEW.verifier_identity
    AND EXISTS (
        SELECT 1
        FROM publication_ingestion_receipts AS receipt
        JOIN publication_stages AS stage
          ON stage.stage_hash = receipt.stage_hash
        WHERE receipt.receipt_hash = NEW.publication_receipt_hash
          AND receipt.stage_hash = NEW.publication_stage_hash
          AND receipt.subject_object_id IS NEW.subject_object_id
          AND receipt.subject_version_hash IS NEW.subject_version_hash
          AND receipt.authoritative = 0
          AND stage.authoritative = 0
          AND json_extract(
              NEW.metadata_json,
              '$.publication_authority.report_artifact_hash'
          ) IS stage.report_artifact_hash
          AND json_extract(
              NEW.metadata_json,
              '$.publication_authority.retained_closure_artifact_hash'
          ) IS stage.retained_closure_artifact_hash
          AND json_extract(
              NEW.metadata_json,
              '$.publication_authority.attestation_bundle_artifact_hash'
          ) IS stage.attestation_bundle_artifact_hash
          AND json_extract(
              NEW.metadata_json,
              '$.publication_authority.raw_verification_hash'
          ) IS receipt.raw_verification_hash
          AND 1 = (
              SELECT count(*)
              FROM json_each(stage.stage_json, '$.retained_artifacts') AS retained
              JOIN artifacts AS request_artifact
                ON request_artifact.artifact_hash = json_extract(
                    retained.value,
                    '$.artifact_hash'
                )
              WHERE json_extract(retained.value, '$.role') = 'publication_request'
                AND json_extract(retained.value, '$.artifact_hash') IS json_extract(
                    NEW.metadata_json,
                    '$.publication_authority.publication_request_hash'
                )
                AND request_artifact.media_type = 'application/json'
                AND request_artifact.creation_source = 'generated'
                AND request_artifact.restriction = 'private'
                AND json_extract(
                    request_artifact.metadata_json,
                    '$.semantic_metadata.artifact_role'
                ) = 'publication_request'
                AND json_extract(
                    request_artifact.metadata_json,
                    '$.semantic_metadata.request_hash'
                ) IS request_artifact.artifact_hash
                AND json_extract(
                    request_artifact.metadata_json,
                    '$.semantic_metadata.formalization_object_id'
                ) IS NEW.subject_object_id
                AND json_extract(
                    request_artifact.metadata_json,
                    '$.semantic_metadata.formalization_version_hash'
                ) IS NEW.subject_version_hash
          )
          AND 1 = (
              SELECT count(*)
              FROM json_each(stage.stage_json, '$.retained_artifacts') AS retained
              WHERE json_extract(retained.value, '$.role') = 'publication_policy'
                AND json_extract(retained.value, '$.artifact_hash') IS json_extract(
                    NEW.metadata_json,
                    '$.publication_authority.publication_policy_hash'
                )
          )
          AND NOT EXISTS (
              SELECT 1
              FROM json_each(NEW.artifact_hashes_json) AS artifact
              WHERE NOT (
                  artifact.value IS stage.report_artifact_hash
                  OR artifact.value IS stage.retained_closure_artifact_hash
                  OR artifact.value IS stage.attestation_bundle_artifact_hash
                  OR artifact.value IS receipt.raw_verification_hash
                  OR artifact.value IS receipt.receipt_hash
                  OR EXISTS (
                      SELECT 1
                      FROM json_each(
                          stage.stage_json,
                          '$.retained_artifacts'
                      ) AS retained
                      WHERE json_extract(retained.value, '$.artifact_hash')
                          IS artifact.value
                  )
              )
          )
          AND NOT EXISTS (
              SELECT 1
              FROM json_each(stage.stage_json, '$.retained_artifacts') AS retained
              WHERE NOT EXISTS (
                  SELECT 1
                  FROM json_each(NEW.artifact_hashes_json) AS artifact
                  WHERE artifact.value IS json_extract(
                      retained.value,
                      '$.artifact_hash'
                  )
              )
          )
          AND EXISTS (
              SELECT 1 FROM json_each(NEW.artifact_hashes_json)
              WHERE value IS stage.report_artifact_hash
          )
          AND EXISTS (
              SELECT 1 FROM json_each(NEW.artifact_hashes_json)
              WHERE value IS stage.retained_closure_artifact_hash
          )
          AND EXISTS (
              SELECT 1 FROM json_each(NEW.artifact_hashes_json)
              WHERE value IS stage.attestation_bundle_artifact_hash
          )
          AND EXISTS (
              SELECT 1 FROM json_each(NEW.artifact_hashes_json)
              WHERE value IS receipt.raw_verification_hash
          )
          AND EXISTS (
              SELECT 1 FROM json_each(NEW.artifact_hashes_json)
              WHERE value IS receipt.receipt_hash
          )
          AND receipt.report_artifact_hash IS stage.report_artifact_hash
          AND receipt.attestation_bundle_artifact_hash
              IS stage.attestation_bundle_artifact_hash
    )
    AND EXISTS (
        SELECT 1
        FROM records AS record
        JOIN record_versions AS version
          ON version.object_id = record.object_id
         AND version.version_hash = record.head_version_hash
        WHERE record.object_id = NEW.subject_object_id
          AND record.record_type = 'formalization'
          AND record.tombstoned = 0
          AND record.head_version_hash = NEW.subject_version_hash
          AND json_extract(version.payload_json, '$.environment_hash')
              IS NEW.environment_hash
          AND (
              (
                  NEW.evidence_kind = 'lean_kernel_proof'
                  AND json_extract(version.payload_json, '$.claim_polarity') = 'claim'
              )
              OR (
                  NEW.evidence_kind = 'lean_kernel_refutation'
                  AND json_extract(version.payload_json, '$.claim_polarity') = 'negation'
              )
          )
    )
    )
)
BEGIN
    SELECT RAISE(ABORT, 'publication authority evidence violates the closed gate');
END;


CREATE TRIGGER evidence_reject_invalid_comparator_authority_insert
BEFORE INSERT ON evidence
WHEN (
    NEW.evidence_kind = 'comparator_run'
    OR NEW.comparator_receipt_hash IS NOT NULL
    OR NEW.comparator_stage_hash IS NOT NULL
    OR json_extract(NEW.metadata_json, '$.schema_version') = 'evidence/3'
    OR json_type(NEW.metadata_json, '$.comparator_authority') IS NOT NULL
    OR (
        NEW.authority_class = 'authoritative'
        AND NEW.evidence_kind NOT IN ('lean_kernel_proof', 'lean_kernel_refutation')
    )
)
AND NOT (
    NEW.evidence_kind = 'comparator_run'
    AND NEW.result = 'accepted'
    AND NEW.authority_class = 'authoritative'
    AND NEW.run_id IS NULL
    AND NEW.job_id IS NULL
    AND NEW.artifact_hash IS NULL
    AND NEW.environment_hash IS NOT NULL
    AND NEW.superseded_by IS NULL
    AND NEW.stale_reason IS NULL
    AND NEW.publication_receipt_hash IS NULL
    AND NEW.publication_stage_hash IS NULL
    AND NEW.comparator_receipt_hash IS NOT NULL
    AND NEW.comparator_stage_hash IS NOT NULL
    AND NEW.evidence_hash IS NOT NULL
    AND length(NEW.evidence_hash) = 64
    AND NEW.evidence_hash NOT GLOB '*[^0-9a-f]*'
    AND NEW.created_at >= 0
    AND length(trim(NEW.created_by)) BETWEEN 1 AND 256
    AND json_valid(NEW.metadata_json)
    AND json_type(NEW.metadata_json) = 'object'
    AND 14 = (SELECT count(*) FROM json_each(NEW.metadata_json))
    AND json_extract(NEW.metadata_json, '$.schema_version') = 'evidence/3'
    AND json_type(NEW.metadata_json, '$.subject') = 'object'
    AND 2 = (SELECT count(*) FROM json_each(NEW.metadata_json, '$.subject'))
    AND json_extract(NEW.metadata_json, '$.subject.object_id') IS NEW.subject_object_id
    AND json_extract(NEW.metadata_json, '$.subject.version_hash') IS NEW.subject_version_hash
    AND json_extract(NEW.metadata_json, '$.evidence_kind') IS NEW.evidence_kind
    AND json_extract(NEW.metadata_json, '$.result') IS NEW.result
    AND json_extract(NEW.metadata_json, '$.authority_class') IS NEW.authority_class
    AND json_type(NEW.metadata_json, '$.producing_run_id') = 'null'
    AND json_type(NEW.metadata_json, '$.producing_job_id') = 'null'
    AND json_extract(NEW.metadata_json, '$.environment_hash') IS NEW.environment_hash
    AND json_type(NEW.metadata_json, '$.supersedes_evidence_id') = 'null'
    AND json_type(NEW.metadata_json, '$.stale') = 'false'
    AND json_type(NEW.metadata_json, '$.stale_reason') = 'null'
    AND json_type(NEW.metadata_json, '$.publication_authority') IS NULL
    AND json_type(NEW.metadata_json, '$.artifact_hashes') = 'array'
    AND json_extract(NEW.metadata_json, '$.artifact_hashes') = json(NEW.artifact_hashes_json)
    AND NOT EXISTS (
        SELECT 1
        FROM json_each(NEW.artifact_hashes_json) AS artifact
        WHERE artifact.type != 'text'
           OR length(artifact.value) != 64
           OR artifact.value GLOB '*[^0-9a-f]*'
    )
    AND NOT EXISTS (
        SELECT 1
        FROM json_each(NEW.artifact_hashes_json) AS later
        JOIN json_each(NEW.artifact_hashes_json) AS earlier
          ON CAST(earlier.key AS INTEGER) < CAST(later.key AS INTEGER)
        WHERE earlier.value >= later.value
    )
    AND json_type(NEW.metadata_json, '$.comparator_authority') = 'object'
    AND 14 = (
        SELECT count(*)
        FROM json_each(NEW.metadata_json, '$.comparator_authority')
    )
    AND json_extract(
        NEW.metadata_json,
        '$.comparator_authority.schema_version'
    ) = 'comparator_authority_binding/1'
    AND json_extract(
        NEW.metadata_json,
        '$.comparator_authority.ingestion_receipt_hash'
    ) IS NEW.comparator_receipt_hash
    AND json_extract(
        NEW.metadata_json,
        '$.comparator_authority.stage_hash'
    ) IS NEW.comparator_stage_hash
    AND NEW.verifier_identity = (
        'comparator-authority-policy:' || json_extract(
            NEW.metadata_json,
            '$.comparator_authority.policy_hash'
        )
    )
    AND json_extract(
        NEW.metadata_json,
        '$.verifier_or_reviewer_identity'
    ) IS NEW.verifier_identity
    AND EXISTS (
        SELECT 1
        FROM comparator_ingestion_receipts AS receipt
        JOIN comparator_authority_stages AS stage
          ON stage.stage_hash = receipt.stage_hash
        WHERE receipt.receipt_hash = NEW.comparator_receipt_hash
          AND receipt.stage_hash = NEW.comparator_stage_hash
          AND stage.subject_object_id IS NEW.subject_object_id
          AND stage.subject_version_hash IS NEW.subject_version_hash
          AND receipt.authoritative = 0
          AND stage.authoritative = 0
          AND receipt.report_artifact_hash IS stage.report_artifact_hash
          AND receipt.attestation_bundle_artifact_hash
              IS stage.attestation_bundle_artifact_hash
          AND json_extract(
              NEW.metadata_json,
              '$.comparator_authority.report_artifact_hash'
          ) IS stage.report_artifact_hash
          AND json_extract(
              NEW.metadata_json,
              '$.comparator_authority.attestation_bundle_artifact_hash'
          ) IS stage.attestation_bundle_artifact_hash
          AND json_extract(
              NEW.metadata_json,
              '$.comparator_authority.raw_verification_hash'
          ) IS receipt.raw_verification_hash
          AND json_extract(
              NEW.metadata_json,
              '$.comparator_authority.policy_hash'
          ) IS stage.policy_hash
          AND stage.policy_hash =
              '3d0bf9b5bf1aba8ba9e1461f6c1105ff6e40f5cb3e34552fc26c24baede779b7'
          AND json_extract(
              NEW.metadata_json,
              '$.comparator_authority.plan_artifact_hash'
          ) IS stage.plan_artifact_hash
          AND json_extract(
              NEW.metadata_json,
              '$.comparator_authority.source_release_manifest_hash'
          ) IS stage.source_release_manifest_hash
          AND json_extract(
              NEW.metadata_json,
              '$.comparator_authority.package_verification_hash'
          ) IS stage.package_verification_hash
          AND json_extract(
              NEW.metadata_json,
              '$.comparator_authority.package_input_fingerprint'
          ) IS stage.package_input_fingerprint
          AND json_extract(
              NEW.metadata_json,
              '$.comparator_authority.source_commit_sha'
          ) IS stage.source_commit_sha
          AND json_extract(
              NEW.metadata_json,
              '$.comparator_authority.workflow_run_id'
          ) IS stage.workflow_run_id
          AND json_extract(
              NEW.metadata_json,
              '$.comparator_authority.workflow_run_attempt'
          ) IS stage.workflow_run_attempt
          AND json_extract(receipt.verification_json, '$.verifier_name') = 'gh'
          AND json_extract(receipt.verification_json, '$.verifier_version') = '2.96.0'
          AND json_extract(
              receipt.verification_json,
              '$.verifier_binary_sha256'
          ) = '56b8bbbb27b066ecb33dbef9a256dc9d1314adaeff0908a752feba6c34053b40'
          AND json_extract(receipt.verification_json, '$.repository') = 'Mnehmos/MathOS'
          AND json_extract(
              receipt.verification_json,
              '$.signer_workflow'
          ) = 'Mnehmos/MathOS/.github/workflows/publication.yml'
          AND json_extract(
              receipt.verification_json,
              '$.certificate_identity'
          ) = 'https://github.com/Mnehmos/MathOS/.github/workflows/publication.yml@refs/heads/main'
          AND json_extract(receipt.verification_json, '$.source_ref') = 'refs/heads/main'
          AND json_extract(
              receipt.verification_json,
              '$.source_commit_sha'
          ) IS stage.source_commit_sha
          AND json_extract(
              receipt.verification_json,
              '$.workflow_run_id'
          ) IS stage.workflow_run_id
          AND json_extract(
              receipt.verification_json,
              '$.workflow_run_attempt'
          ) IS stage.workflow_run_attempt
          AND json_extract(
              receipt.verification_json,
              '$.predicate_type'
          ) = 'https://slsa.dev/provenance/v1'
          AND json_type(
              receipt.verification_json,
              '$.self_hosted_runners_denied'
          ) = 'true'
          AND json_extract(
              receipt.verification_json,
              '$.verified_attestation_count'
          ) = 1
          AND json_extract(
              receipt.verification_json,
              '$.verified_timestamp_count'
          ) BETWEEN 1 AND 8
          AND json_type(receipt.verification_json, '$.authoritative') = 'false'
          AND NOT EXISTS (
              SELECT 1
              FROM json_each(NEW.artifact_hashes_json) AS artifact
              WHERE NOT (
                  artifact.value IS stage.stage_hash
                  OR artifact.value IS stage.plan_artifact_hash
                  OR artifact.value IS stage.attestation_bundle_artifact_hash
                  OR artifact.value IS stage.policy_hash
                  OR artifact.value IS receipt.raw_verification_hash
                  OR artifact.value IS receipt.receipt_hash
                  OR EXISTS (
                      SELECT 1
                      FROM json_each(stage.stage_json, '$.artifacts') AS member
                      WHERE json_extract(member.value, '$.content_hash') IS artifact.value
                  )
              )
          )
          AND NOT EXISTS (
              SELECT 1
              FROM json_each(stage.stage_json, '$.artifacts') AS member
              WHERE NOT EXISTS (
                  SELECT 1
                  FROM json_each(NEW.artifact_hashes_json) AS artifact
                  WHERE artifact.value IS json_extract(member.value, '$.content_hash')
              )
          )
          AND EXISTS (
              SELECT 1 FROM json_each(NEW.artifact_hashes_json)
              WHERE value IS stage.stage_hash
          )
          AND EXISTS (
              SELECT 1 FROM json_each(NEW.artifact_hashes_json)
              WHERE value IS stage.plan_artifact_hash
          )
          AND EXISTS (
              SELECT 1 FROM json_each(NEW.artifact_hashes_json)
              WHERE value IS stage.attestation_bundle_artifact_hash
          )
          AND EXISTS (
              SELECT 1 FROM json_each(NEW.artifact_hashes_json)
              WHERE value IS stage.policy_hash
          )
          AND EXISTS (
              SELECT 1 FROM json_each(NEW.artifact_hashes_json)
              WHERE value IS receipt.raw_verification_hash
          )
          AND EXISTS (
              SELECT 1 FROM json_each(NEW.artifact_hashes_json)
              WHERE value IS receipt.receipt_hash
          )
    )
    AND EXISTS (
        SELECT 1
        FROM records AS record
        JOIN record_versions AS version
          ON version.object_id = record.object_id
         AND version.version_hash = record.head_version_hash
        WHERE record.object_id = NEW.subject_object_id
          AND record.record_type = 'formalization'
          AND record.tombstoned = 0
          AND record.head_version_hash = NEW.subject_version_hash
          AND json_extract(version.payload_json, '$.environment_hash')
              IS NEW.environment_hash
    )
)
BEGIN
    SELECT RAISE(ABORT, 'Comparator authority evidence violates the closed gate');
END;
