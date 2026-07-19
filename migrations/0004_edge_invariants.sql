CREATE TRIGGER edges_reject_update
BEFORE UPDATE ON edges
BEGIN
    SELECT RAISE(ABORT, 'edges are immutable');
END;

CREATE TRIGGER edges_reject_delete
BEFORE DELETE ON edges
BEGIN
    SELECT RAISE(ABORT, 'edges are immutable');
END;

CREATE TRIGGER edges_require_owned_versions
BEFORE INSERT ON edges
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM record_versions
        WHERE object_id = NEW.source_object_id
          AND version_hash = NEW.source_version_hash
    ) THEN RAISE(ABORT, 'edge source version is not owned by source object') END;
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM record_versions
        WHERE object_id = NEW.target_object_id
          AND version_hash = NEW.target_version_hash
    ) THEN RAISE(ABORT, 'edge target version is not owned by target object') END;
END;

CREATE TRIGGER edges_reject_hard_prerequisite_cycle
BEFORE INSERT ON edges
WHEN NEW.edge_type = 'pedagogy.hard_prerequisite'
BEGIN
    SELECT CASE WHEN NEW.source_object_id = NEW.target_object_id OR EXISTS (
        WITH RECURSIVE reachable(node) AS (
            SELECT target_object_id
            FROM edges
            WHERE edge_type = 'pedagogy.hard_prerequisite'
              AND source_object_id = NEW.target_object_id
            UNION
            SELECT edge.target_object_id
            FROM edges AS edge
            JOIN reachable ON edge.source_object_id = reachable.node
            WHERE edge.edge_type = 'pedagogy.hard_prerequisite'
        )
        SELECT 1 FROM reachable WHERE node = NEW.source_object_id
    ) THEN RAISE(ABORT, 'hard pedagogical prerequisite cycle') END;
END;
