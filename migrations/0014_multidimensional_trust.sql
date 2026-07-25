CREATE TABLE trust_transitions (
    transition_id TEXT PRIMARY KEY
        CHECK (length(transition_id) = 36),
    transition_hash TEXT NOT NULL UNIQUE
        CHECK (
            length(transition_hash) = 64
            AND transition_hash NOT GLOB '*[^0-9a-f]*'
        ),
    subject_object_id TEXT NOT NULL REFERENCES records(object_id),
    subject_version_hash TEXT NOT NULL REFERENCES record_versions(version_hash),
    dimension TEXT NOT NULL
        CHECK (dimension IN ('definition', 'reuse', 'coverage')),
    from_status TEXT NOT NULL,
    to_status TEXT NOT NULL,
    reviewer_identity TEXT NOT NULL
        CHECK (
            length(trim(reviewer_identity)) BETWEEN 1 AND 256
        ),
    evidence_artifact_hashes_json TEXT NOT NULL
        CHECK (
            json_valid(evidence_artifact_hashes_json)
            AND json_type(evidence_artifact_hashes_json) = 'array'
            AND json_array_length(evidence_artifact_hashes_json) BETWEEN 1 AND 256
        ),
    reason TEXT NOT NULL
        CHECK (length(trim(reason)) BETWEEN 1 AND 4096),
    predecessor_transition_id TEXT REFERENCES trust_transitions(transition_id),
    request_json TEXT NOT NULL
        CHECK (
            json_valid(request_json)
            AND json_type(request_json) = 'object'
            AND json_extract(request_json, '$.schema_version') = 'trust_transition/1'
            AND json_extract(request_json, '$.formalization.object_id') IS subject_object_id
            AND json_extract(request_json, '$.formalization.version_hash') IS subject_version_hash
            AND json_extract(request_json, '$.dimension') IS dimension
            AND json_extract(request_json, '$.from_status') IS from_status
            AND json_extract(request_json, '$.to_status') IS to_status
            AND json_extract(request_json, '$.reviewer_identity') IS reviewer_identity
            AND json_extract(request_json, '$.evidence_artifact_hashes')
                IS evidence_artifact_hashes_json
            AND json_extract(request_json, '$.reason') IS reason
            AND json_extract(request_json, '$.predecessor_transition_id')
                IS predecessor_transition_id
            AND json_type(request_json, '$.predecessor_transition_id')
                IS NOT NULL
            AND json_type(request_json, '$.predecessor_transition_id')
                IN ('null', 'text')
        ),
    created_at INTEGER NOT NULL CHECK (created_at >= 0),
    created_by TEXT NOT NULL
        CHECK (length(trim(created_by)) BETWEEN 1 AND 256),
    CHECK (created_by = reviewer_identity),
    CHECK (
        (
            dimension = 'definition'
            AND from_status IN (
                'ungrounded',
                'source_grounded',
                'project_approved',
                'upstream_accepted'
            )
            AND to_status IN (
                'ungrounded',
                'source_grounded',
                'project_approved',
                'upstream_accepted'
            )
        )
        OR (
            dimension = 'reuse'
            AND from_status IN (
                'experimental',
                'campaign_specific',
                'candidate',
                'review_ready',
                'upstreamed'
            )
            AND to_status IN (
                'experimental',
                'campaign_specific',
                'candidate',
                'review_ready',
                'upstreamed'
            )
        )
        OR (
            dimension = 'coverage'
            AND from_status IN (
                'unknown',
                'statement_only',
                'analytical_core',
                'supporting_lemma',
                'finite_specialization',
                'asymptotic_component',
                'full_theorem'
            )
            AND to_status IN (
                'unknown',
                'statement_only',
                'analytical_core',
                'supporting_lemma',
                'finite_specialization',
                'asymptotic_component',
                'full_theorem'
            )
        )
    ),
    CHECK (
        (
            dimension = 'definition'
            AND (
                (from_status = 'ungrounded' AND to_status = 'source_grounded')
                OR (
                    from_status = 'source_grounded'
                    AND to_status IN ('ungrounded', 'project_approved')
                )
                OR (
                    from_status = 'project_approved'
                    AND to_status IN (
                        'ungrounded',
                        'source_grounded',
                        'upstream_accepted'
                    )
                )
                OR (
                    from_status = 'upstream_accepted'
                    AND to_status IN (
                        'ungrounded',
                        'source_grounded',
                        'project_approved'
                    )
                )
            )
        )
        OR (
            dimension = 'reuse'
            AND (
                (
                    from_status = 'experimental'
                    AND to_status = 'campaign_specific'
                )
                OR (
                    from_status = 'campaign_specific'
                    AND to_status IN ('experimental', 'candidate')
                )
                OR (
                    from_status = 'candidate'
                    AND to_status IN (
                        'experimental',
                        'campaign_specific',
                        'review_ready'
                    )
                )
                OR (
                    from_status = 'review_ready'
                    AND to_status IN (
                        'experimental',
                        'campaign_specific',
                        'candidate',
                        'upstreamed'
                    )
                )
                OR (
                    from_status = 'upstreamed'
                    AND to_status IN (
                        'experimental',
                        'campaign_specific',
                        'candidate',
                        'review_ready'
                    )
                )
            )
        )
        OR (dimension = 'coverage' AND from_status != to_status)
    )
) STRICT;

CREATE INDEX trust_transitions_subject
ON trust_transitions(
    subject_object_id,
    subject_version_hash,
    dimension,
    created_at,
    transition_id
);

CREATE UNIQUE INDEX trust_transitions_initial
ON trust_transitions(subject_object_id, subject_version_hash, dimension)
WHERE predecessor_transition_id IS NULL;

CREATE UNIQUE INDEX trust_transitions_successor
ON trust_transitions(predecessor_transition_id)
WHERE predecessor_transition_id IS NOT NULL;

CREATE TRIGGER trust_transitions_reject_invalid_insert
BEFORE INSERT ON trust_transitions
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1
        FROM record_versions AS version
        JOIN records AS record
          ON record.object_id = version.object_id
        WHERE version.version_hash = NEW.subject_version_hash
          AND version.object_id = NEW.subject_object_id
          AND record.record_type = 'formalization'
    ) THEN RAISE(
        ABORT,
        'trust transition subject must be one exact formalization'
    ) END;

    SELECT CASE WHEN EXISTS (
        SELECT 1
        FROM json_each(NEW.evidence_artifact_hashes_json) AS evidence
        WHERE evidence.type != 'text'
           OR length(evidence.value) != 64
           OR evidence.value GLOB '*[^0-9a-f]*'
           OR NOT EXISTS (
               SELECT 1
               FROM artifacts
               WHERE artifact_hash = evidence.value
           )
    ) THEN RAISE(
        ABORT,
        'trust transition evidence must be registered artifact hashes'
    ) END;

    SELECT CASE WHEN EXISTS (
        SELECT 1
        FROM json_each(NEW.evidence_artifact_hashes_json) AS left_item
        JOIN json_each(NEW.evidence_artifact_hashes_json) AS right_item
          ON left_item.key < right_item.key
        WHERE left_item.value >= right_item.value
    ) THEN RAISE(
        ABORT,
        'trust transition evidence hashes must be strictly sorted and unique'
    ) END;

    SELECT CASE WHEN (
        SELECT COUNT(*)
        FROM trust_transitions
        WHERE subject_object_id = NEW.subject_object_id
          AND subject_version_hash = NEW.subject_version_hash
          AND dimension = NEW.dimension
    ) >= 256 THEN RAISE(
        ABORT,
        'trust transition dimension history cannot exceed 256 entries'
    ) END;

    SELECT CASE WHEN (
        NEW.predecessor_transition_id IS NULL
        AND (
            NEW.from_status != CASE NEW.dimension
                WHEN 'definition' THEN 'ungrounded'
                WHEN 'reuse' THEN 'experimental'
                WHEN 'coverage' THEN 'unknown'
            END
            OR EXISTS (
                SELECT 1
                FROM trust_transitions
                WHERE subject_object_id = NEW.subject_object_id
                  AND subject_version_hash = NEW.subject_version_hash
                  AND dimension = NEW.dimension
            )
        )
    ) OR (
        NEW.predecessor_transition_id IS NOT NULL
        AND NOT EXISTS (
            SELECT 1
            FROM trust_transitions AS predecessor
            WHERE predecessor.transition_id = NEW.predecessor_transition_id
              AND predecessor.subject_object_id = NEW.subject_object_id
              AND predecessor.subject_version_hash = NEW.subject_version_hash
              AND predecessor.dimension = NEW.dimension
              AND predecessor.to_status = NEW.from_status
              AND NOT EXISTS (
                  SELECT 1
                  FROM trust_transitions AS successor
                  WHERE successor.predecessor_transition_id =
                      predecessor.transition_id
              )
        )
    ) THEN RAISE(
        ABORT,
        'trust transition must extend the exact current dimension head'
    ) END;
END;

CREATE TRIGGER trust_transitions_reject_update
BEFORE UPDATE ON trust_transitions
BEGIN
    SELECT RAISE(ABORT, 'trust transitions are immutable');
END;

CREATE TRIGGER trust_transitions_reject_delete
BEFORE DELETE ON trust_transitions
BEGIN
    SELECT RAISE(ABORT, 'trust transition history is durable');
END;

CREATE TRIGGER evidence_reject_kernel_trust_history_overflow
BEFORE INSERT ON evidence
WHEN NEW.evidence_kind IN (
    'lean_elaboration',
    'lean_kernel_proof',
    'lean_kernel_refutation'
)
AND (
    SELECT COUNT(*)
    FROM evidence
    WHERE subject_object_id = NEW.subject_object_id
      AND subject_version_hash = NEW.subject_version_hash
      AND evidence_kind IN (
          'lean_elaboration',
          'lean_kernel_proof',
          'lean_kernel_refutation'
      )
) >= 256
BEGIN
    SELECT RAISE(
        ABORT,
        'kernel trust evidence history cannot exceed 256 admitted entries'
    );
END;
