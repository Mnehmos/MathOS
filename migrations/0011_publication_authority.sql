ALTER TABLE publication_ingestion_receipts
    ADD COLUMN subject_object_id TEXT
        REFERENCES records(object_id);

ALTER TABLE publication_ingestion_receipts
    ADD COLUMN subject_version_hash TEXT
        REFERENCES record_versions(version_hash)
        CHECK (
            subject_version_hash IS NULL
            OR (
                length(subject_version_hash) = 64
                AND subject_version_hash NOT GLOB '*[^0-9a-f]*'
            )
        );

ALTER TABLE evidence
    ADD COLUMN publication_receipt_hash TEXT
        REFERENCES publication_ingestion_receipts(receipt_hash)
        CHECK (
            publication_receipt_hash IS NULL
            OR (
                length(publication_receipt_hash) = 64
                AND publication_receipt_hash NOT GLOB '*[^0-9a-f]*'
            )
        );

ALTER TABLE evidence
    ADD COLUMN publication_stage_hash TEXT
        REFERENCES publication_stages(stage_hash)
        CHECK (
            publication_stage_hash IS NULL
            OR (
                length(publication_stage_hash) = 64
                AND publication_stage_hash NOT GLOB '*[^0-9a-f]*'
            )
        );

CREATE UNIQUE INDEX evidence_publication_receipt_identity
ON evidence(publication_receipt_hash)
WHERE publication_receipt_hash IS NOT NULL;

CREATE TRIGGER publication_ingestion_receipts_reject_unbound_subject_insert
BEFORE INSERT ON publication_ingestion_receipts
WHEN NEW.subject_object_id IS NULL
  OR NEW.subject_version_hash IS NULL
  OR NOT EXISTS (
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
  OR 1 != (
      SELECT count(*)
      FROM publication_stages AS stage
      JOIN json_each(stage.stage_json, '$.retained_artifacts') AS retained
      JOIN artifacts AS request_artifact
        ON request_artifact.artifact_hash = json_extract(
            retained.value,
            '$.artifact_hash'
        )
      WHERE stage.stage_hash = NEW.stage_hash
        AND json_extract(retained.value, '$.role') = 'publication_request'
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
BEGIN
    SELECT RAISE(ABORT, 'publication receipt subject must be the current exact formalization');
END;

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
BEGIN
    SELECT RAISE(ABORT, 'publication authority evidence violates the closed gate');
END;
