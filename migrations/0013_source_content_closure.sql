CREATE TRIGGER source_versions_require_registered_content
BEFORE INSERT ON record_versions
WHEN (
    SELECT record_type
    FROM records
    WHERE object_id = NEW.object_id
) = 'source'
AND NOT (
    json_type(NEW.payload_json, '$.content_hash') = 'null'
    OR (
        json_type(NEW.payload_json, '$.content_hash') = 'text'
        AND length(json_extract(NEW.payload_json, '$.content_hash')) = 64
        AND json_extract(NEW.payload_json, '$.content_hash') NOT GLOB '*[^0-9a-f]*'
        AND EXISTS (
            SELECT 1
            FROM artifacts AS artifact
            WHERE artifact.artifact_hash =
                    json_extract(NEW.payload_json, '$.content_hash')
              AND artifact.creation_source IN ('user_ingest', 'import', 'migration')
              AND json_extract(
                    artifact.metadata_json,
                    '$.semantic_metadata.source_role'
                  ) = 'source_content'
              AND json_extract(artifact.metadata_json, '$.creation_source')
                    IS artifact.creation_source
              AND json_extract(artifact.metadata_json, '$.license_expression')
                    IS artifact.license_expression
              AND json_extract(artifact.metadata_json, '$.restriction')
                    IS artifact.restriction
              AND (
                    json_type(NEW.payload_json, '$.license_expression') = 'null'
                    OR (
                        json_type(NEW.payload_json, '$.license_expression') = 'text'
                        AND artifact.license_expression IS
                            json_extract(NEW.payload_json, '$.license_expression')
                    )
                  )
              AND (
                    NOT (
                        json_extract(NEW.payload_json, '$.redaction_class') = 'public'
                        AND json_extract(
                            NEW.payload_json,
                            '$.redistribution_status'
                        ) = 'allowed'
                    )
                    OR (
                        artifact.restriction = 'public'
                        AND json_type(
                            NEW.payload_json,
                            '$.license_expression'
                        ) = 'text'
                        AND artifact.license_expression IS
                            json_extract(NEW.payload_json, '$.license_expression')
                    )
                  )
        )
    )
)
BEGIN
    SELECT RAISE(
        ABORT,
        'source content must resolve to a policy-compatible registered source-content artifact'
    );
END;

CREATE TEMP TABLE source_content_closure_migration_guard (
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;

INSERT INTO source_content_closure_migration_guard(valid)
SELECT 0
FROM record_versions AS version
JOIN records AS record
  ON record.object_id = version.object_id
WHERE record.record_type = 'source'
  AND NOT (
      json_type(version.payload_json, '$.content_hash') = 'null'
      OR (
          json_type(version.payload_json, '$.content_hash') = 'text'
          AND length(json_extract(version.payload_json, '$.content_hash')) = 64
          AND json_extract(version.payload_json, '$.content_hash')
                NOT GLOB '*[^0-9a-f]*'
          AND EXISTS (
              SELECT 1
              FROM artifacts AS artifact
              WHERE artifact.artifact_hash =
                      json_extract(version.payload_json, '$.content_hash')
                AND artifact.creation_source IN ('user_ingest', 'import', 'migration')
                AND json_extract(
                      artifact.metadata_json,
                      '$.semantic_metadata.source_role'
                    ) = 'source_content'
                AND json_extract(artifact.metadata_json, '$.creation_source')
                      IS artifact.creation_source
                AND json_extract(artifact.metadata_json, '$.license_expression')
                      IS artifact.license_expression
                AND json_extract(artifact.metadata_json, '$.restriction')
                      IS artifact.restriction
                AND (
                      json_type(version.payload_json, '$.license_expression') = 'null'
                      OR (
                          json_type(version.payload_json, '$.license_expression') = 'text'
                          AND artifact.license_expression IS
                              json_extract(version.payload_json, '$.license_expression')
                      )
                    )
                AND (
                      NOT (
                          json_extract(version.payload_json, '$.redaction_class') = 'public'
                          AND json_extract(
                              version.payload_json,
                              '$.redistribution_status'
                          ) = 'allowed'
                      )
                      OR (
                          artifact.restriction = 'public'
                          AND json_type(
                              version.payload_json,
                              '$.license_expression'
                          ) = 'text'
                          AND artifact.license_expression IS
                              json_extract(version.payload_json, '$.license_expression')
                      )
                    )
          )
      )
  )
LIMIT 1;

DROP TABLE source_content_closure_migration_guard;
