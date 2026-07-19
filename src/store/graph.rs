use std::collections::HashSet;

use rusqlite::{Connection, params_from_iter, types::Value as SqlValue};

use super::{Store, read_edge, validate_edge_endpoint, validate_hash};
use crate::domain::{
    EdgeKind, EdgeSnapshot, GraphTraversalHit, GraphTraversalRequest, TraversalDirection,
};
use crate::error::AppError;

const MAX_TRAVERSAL_DEPTH: u32 = 32;
const MAX_TRAVERSAL_RESULTS: usize = 1_000;
const MAX_TRAVERSAL_EDGE_SCAN: usize = 10_000;

type NodeIdentity = (String, String);

impl Store {
    pub fn traverse_graph(
        &self,
        request: &GraphTraversalRequest,
    ) -> Result<Vec<GraphTraversalHit>, AppError> {
        validate_traversal_request(request)?;
        validate_edge_endpoint(
            &self.connection,
            "root",
            &request.root_object_id,
            &request.root_version_hash,
        )?;

        let mut kinds = request.edge_kinds.clone();
        kinds.sort_by_key(|kind| kind.as_str());
        kinds.dedup();

        let root = (
            request.root_object_id.clone(),
            request.root_version_hash.clone(),
        );
        let mut visited_nodes = HashSet::from([root.clone()]);
        let mut visited_edges = HashSet::new();
        let mut frontier = vec![root];
        let mut hits = Vec::new();
        let mut scanned = 0_usize;

        for depth in 1..=request.max_depth {
            if frontier.is_empty() || hits.len() == request.limit {
                break;
            }
            frontier.sort();
            frontier.dedup();
            let frontier_set: HashSet<NodeIdentity> = frontier.iter().cloned().collect();
            let mut candidate_ids = Vec::new();

            for (object_id, version_hash) in &frontier {
                if matches!(
                    request.direction,
                    TraversalDirection::Outgoing | TraversalDirection::Both
                ) {
                    let ids = load_adjacent_edge_ids(
                        &self.connection,
                        object_id,
                        version_hash,
                        true,
                        &kinds,
                        MAX_TRAVERSAL_EDGE_SCAN.saturating_sub(scanned) + 1,
                    )?;
                    scanned += ids.len();
                    enforce_scan_bound(scanned)?;
                    candidate_ids.extend(ids);
                }
                if matches!(
                    request.direction,
                    TraversalDirection::Incoming | TraversalDirection::Both
                ) {
                    let ids = load_adjacent_edge_ids(
                        &self.connection,
                        object_id,
                        version_hash,
                        false,
                        &kinds,
                        MAX_TRAVERSAL_EDGE_SCAN.saturating_sub(scanned) + 1,
                    )?;
                    scanned += ids.len();
                    enforce_scan_bound(scanned)?;
                    candidate_ids.extend(ids);
                }
            }

            candidate_ids.sort();
            candidate_ids.dedup();
            let mut candidates = candidate_ids
                .into_iter()
                .filter(|edge_id| visited_edges.insert(edge_id.clone()))
                .map(|edge_id| read_edge(&self.connection, &edge_id))
                .collect::<Result<Vec<_>, _>>()?;
            candidates.sort_by(compare_edges);

            let mut next_frontier = Vec::new();
            for edge in candidates {
                add_reached_nodes(
                    &edge,
                    request.direction,
                    &frontier_set,
                    &mut visited_nodes,
                    &mut next_frontier,
                );
                hits.push(GraphTraversalHit { depth, edge });
                if hits.len() == request.limit {
                    break;
                }
            }
            frontier = next_frontier;
        }

        Ok(hits)
    }
}

fn validate_traversal_request(request: &GraphTraversalRequest) -> Result<(), AppError> {
    validate_hash(&request.root_version_hash, "graph root version")?;
    if request.root_object_id.trim().is_empty() {
        return Err(AppError::new(
            "MCL_GRAPH_ROOT_INVALID",
            "graph root object ID must be nonempty",
            false,
            "Use an exact object ID returned by canonical lookup.",
        ));
    }
    if !(1..=MAX_TRAVERSAL_DEPTH).contains(&request.max_depth) {
        return Err(AppError::new(
            "MCL_GRAPH_DEPTH_INVALID",
            format!("graph traversal depth must be between 1 and {MAX_TRAVERSAL_DEPTH}"),
            false,
            "Choose a bounded traversal depth within the documented limit.",
        ));
    }
    if !(1..=MAX_TRAVERSAL_RESULTS).contains(&request.limit) {
        return Err(AppError::new(
            "MCL_GRAPH_LIMIT_INVALID",
            format!("graph traversal result limit must be between 1 and {MAX_TRAVERSAL_RESULTS}"),
            false,
            "Choose a bounded result limit within the documented range.",
        ));
    }
    Ok(())
}

fn load_adjacent_edge_ids(
    connection: &Connection,
    object_id: &str,
    version_hash: &str,
    outgoing: bool,
    kinds: &[EdgeKind],
    scan_limit: usize,
) -> Result<Vec<String>, AppError> {
    let (object_column, version_column) = if outgoing {
        ("source_object_id", "source_version_hash")
    } else {
        ("target_object_id", "target_version_hash")
    };
    let mut sql =
        format!("SELECT edge_id FROM edges WHERE {object_column} = ? AND {version_column} = ?");
    let mut parameters = vec![
        SqlValue::Text(object_id.to_owned()),
        SqlValue::Text(version_hash.to_owned()),
    ];
    if !kinds.is_empty() {
        sql.push_str(" AND edge_type IN (");
        sql.push_str(&vec!["?"; kinds.len()].join(","));
        sql.push(')');
        parameters.extend(
            kinds
                .iter()
                .map(|kind| SqlValue::Text(kind.as_str().to_owned())),
        );
    }
    sql.push_str(
        " ORDER BY edge_type, source_object_id, source_version_hash, target_object_id, target_version_hash, edge_id LIMIT ?",
    );
    parameters.push(SqlValue::Integer(scan_limit as i64));

    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| AppError::database("prepare bounded graph traversal", error))?;
    statement
        .query_map(params_from_iter(parameters.iter()), |row| row.get(0))
        .map_err(|error| AppError::database("query bounded graph traversal", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::database("read bounded graph traversal", error))
}

fn enforce_scan_bound(scanned: usize) -> Result<(), AppError> {
    if scanned > MAX_TRAVERSAL_EDGE_SCAN {
        return Err(AppError::new(
            "MCL_GRAPH_SCAN_LIMIT",
            format!("graph traversal exceeded the {MAX_TRAVERSAL_EDGE_SCAN}-edge scan bound"),
            true,
            "Use a narrower edge-kind filter, root, or depth.",
        ));
    }
    Ok(())
}

fn compare_edges(left: &EdgeSnapshot, right: &EdgeSnapshot) -> std::cmp::Ordering {
    left.kind
        .as_str()
        .cmp(right.kind.as_str())
        .then_with(|| left.source_object_id.cmp(&right.source_object_id))
        .then_with(|| left.source_version_hash.cmp(&right.source_version_hash))
        .then_with(|| left.target_object_id.cmp(&right.target_object_id))
        .then_with(|| left.target_version_hash.cmp(&right.target_version_hash))
        .then_with(|| left.edge_id.cmp(&right.edge_id))
}

fn add_reached_nodes(
    edge: &EdgeSnapshot,
    direction: TraversalDirection,
    frontier: &HashSet<NodeIdentity>,
    visited: &mut HashSet<NodeIdentity>,
    next: &mut Vec<NodeIdentity>,
) {
    let source = (
        edge.source_object_id.clone(),
        edge.source_version_hash.clone(),
    );
    let target = (
        edge.target_object_id.clone(),
        edge.target_version_hash.clone(),
    );
    if matches!(
        direction,
        TraversalDirection::Outgoing | TraversalDirection::Both
    ) && frontier.contains(&source)
        && visited.insert(target.clone())
    {
        next.push(target.clone());
    }
    if matches!(
        direction,
        TraversalDirection::Incoming | TraversalDirection::Both
    ) && frontier.contains(&target)
        && visited.insert(source.clone())
    {
        next.push(source);
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::domain::{EdgeDraft, RecordDraft, RecordKind, RecordSnapshot};
    use serde_json::json;

    fn open_store(path: &std::path::Path) -> Store {
        let mut store = Store::open(path).expect("database opens");
        store.migrate().expect("migrations apply");
        store
    }

    fn record(store: &mut Store, label: &str) -> RecordSnapshot {
        store
            .create_record(
                &RecordDraft {
                    kind: RecordKind::Concept,
                    schema_version: "concept/1".to_owned(),
                    payload: json!({
                        "name": label,
                        "aliases": [],
                        "description": format!("test concept {label}"),
                        "subject_domains": ["test mathematics"],
                        "formal_declarations": [],
                        "external_taxonomy_crosswalks": [],
                        "pedagogy_metadata_references": [],
                        "provenance_references": []
                    }),
                    searchable_text: label.to_owned(),
                },
                "graph-test",
                &format!("record-{label}"),
            )
            .expect("record created")
    }

    fn edge(
        store: &mut Store,
        source: &RecordSnapshot,
        target: &RecordSnapshot,
        kind: EdgeKind,
        key: &str,
    ) -> EdgeSnapshot {
        store
            .create_edge(
                &EdgeDraft {
                    kind,
                    source_object_id: source.object_id.clone(),
                    source_version_hash: source.version_hash.clone(),
                    target_object_id: target.object_id.clone(),
                    target_version_hash: target.version_hash.clone(),
                    payload: json!({}),
                },
                "graph-test",
                key,
            )
            .expect("edge created")
    }

    fn request(root: &RecordSnapshot, direction: TraversalDirection) -> GraphTraversalRequest {
        GraphTraversalRequest {
            root_object_id: root.object_id.clone(),
            root_version_hash: root.version_hash.clone(),
            direction,
            edge_kinds: Vec::new(),
            max_depth: 8,
            limit: 100,
        }
    }

    #[test]
    fn traversal_preserves_direction_depth_kind_and_exact_versions() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = open_store(&database);
        let a = record(&mut store, "a");
        let b = record(&mut store, "b");
        let c = record(&mut store, "c");
        let first = edge(&mut store, &a, &b, EdgeKind::LogicDependsOn, "edge-ab");
        let second = edge(
            &mut store,
            &b,
            &c,
            EdgeKind::PedagogySoftPrerequisite,
            "edge-bc",
        );

        let outgoing = store
            .traverse_graph(&request(&a, TraversalDirection::Outgoing))
            .expect("outgoing traversal");
        assert_eq!(outgoing.len(), 2);
        assert_eq!(outgoing[0].depth, 1);
        assert_eq!(outgoing[0].edge, first);
        assert_eq!(outgoing[1].depth, 2);
        assert_eq!(outgoing[1].edge, second);

        let incoming = store
            .traverse_graph(&request(&c, TraversalDirection::Incoming))
            .expect("incoming traversal");
        assert_eq!(incoming.len(), 2);
        assert_eq!(incoming[0].edge.target_version_hash, c.version_hash);
        assert_eq!(incoming[1].edge.source_version_hash, a.version_hash);

        let mut filtered = request(&a, TraversalDirection::Outgoing);
        filtered.edge_kinds = vec![EdgeKind::PedagogySoftPrerequisite];
        assert!(
            store
                .traverse_graph(&filtered)
                .expect("filtered traversal")
                .is_empty()
        );
    }

    #[test]
    fn logical_cycle_terminates_without_duplicates_and_restarts_deterministically() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let (a, expected) = {
            let mut store = open_store(&database);
            let a = record(&mut store, "cycle-a");
            let b = record(&mut store, "cycle-b");
            edge(&mut store, &a, &b, EdgeKind::LogicEquivalentTo, "cycle-ab");
            edge(&mut store, &b, &a, EdgeKind::LogicEquivalentTo, "cycle-ba");
            let expected = store
                .traverse_graph(&request(&a, TraversalDirection::Outgoing))
                .expect("cycle traversal");
            (a, expected)
        };
        assert_eq!(expected.len(), 2);
        assert_eq!(expected[0].depth, 1);
        assert_eq!(expected[1].depth, 2);
        let reopened = open_store(&database);
        assert_eq!(
            reopened
                .traverse_graph(&request(&a, TraversalDirection::Outgoing))
                .expect("restarted traversal"),
            expected
        );
    }

    #[test]
    fn both_direction_and_result_limit_are_deterministic() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = open_store(&database);
        let center = record(&mut store, "center");
        let left = record(&mut store, "left");
        let right = record(&mut store, "right");
        let other = record(&mut store, "other");
        edge(
            &mut store,
            &left,
            &center,
            EdgeKind::LogicImplies,
            "left-center",
        );
        edge(
            &mut store,
            &center,
            &right,
            EdgeKind::LogicDependsOn,
            "center-right",
        );
        edge(
            &mut store,
            &center,
            &other,
            EdgeKind::ResearchUsesTechnique,
            "center-other",
        );
        let mut bounded = request(&center, TraversalDirection::Both);
        bounded.max_depth = 1;
        bounded.limit = 2;
        let hits = store
            .traverse_graph(&bounded)
            .expect("both-direction traversal");
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|hit| hit.depth == 1));
        assert!(hits[0].edge.kind.as_str() <= hits[1].edge.kind.as_str());
    }

    #[test]
    fn invalid_root_depth_and_limit_fail_with_structured_errors() {
        let temporary = TempDir::new().expect("temporary directory");
        let database = temporary.path().join("state.sqlite3");
        let mut store = open_store(&database);
        let a = record(&mut store, "validation-a");
        let b = record(&mut store, "validation-b");

        let mut mismatched = request(&a, TraversalDirection::Outgoing);
        mismatched.root_version_hash = b.version_hash;
        assert_eq!(
            store
                .traverse_graph(&mismatched)
                .expect_err("mismatched version")
                .code,
            "MCL_EDGE_ENDPOINT_INVALID"
        );

        let mut depth = request(&a, TraversalDirection::Outgoing);
        depth.max_depth = 0;
        assert_eq!(
            store.traverse_graph(&depth).expect_err("zero depth").code,
            "MCL_GRAPH_DEPTH_INVALID"
        );

        let mut limit = request(&a, TraversalDirection::Outgoing);
        limit.limit = MAX_TRAVERSAL_RESULTS + 1;
        assert_eq!(
            store
                .traverse_graph(&limit)
                .expect_err("excessive limit")
                .code,
            "MCL_GRAPH_LIMIT_INVALID"
        );
    }
}
