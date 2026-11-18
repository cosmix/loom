use super::tests::{graph_of, node, view_options};
use super::*;
use crate::context::source_graph::{
    EdgeProvenance, FileCoverage, SourceEdge, SourceEdgeKind, SourceNodeKind, Span,
};
use serde_json::Value;
use tempfile::TempDir;

fn round_trip(value: Value) -> Value {
    serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap()
}

fn identity_graph(path: &str, id: &str, caller_path: &str, caller_id: &str) -> ResolvedGraph {
    let target = node(
        id,
        path,
        SourceNodeKind::Function,
        &["target"],
        FileCoverage::Full,
    );
    let caller = node(
        caller_id,
        caller_path,
        SourceNodeKind::Function,
        &["caller"],
        FileCoverage::Full,
    );
    let edge = SourceEdge::bound(
        caller_id,
        id,
        SourceEdgeKind::Calls,
        "target",
        Span::default(),
        EdgeProvenance::LocalName,
    );
    graph_of(vec![
        (path, target, vec![]),
        (caller_path, caller, vec![edge]),
    ])
}

#[test]
fn json_identities_round_trip_and_resolve_exactly() {
    let path = "src/`tab\tline\n.rs";
    let id = format!("{path}#function:{}", "x".repeat(220));
    let caller_path = "src/`caller\tline\n.rs";
    let caller_id = format!("{caller_path}#function:caller");
    let graph = identity_graph(path, &id, caller_path, &caller_id);
    let root = TempDir::new().unwrap();
    let opts = view_options(vec![]);
    let found = round_trip(find_all_json(&graph, "target", &opts));
    let parsed_id = found["matches"][0]["id"].as_str().unwrap();
    assert_eq!(parsed_id, id);
    assert_eq!(found["matches"][0]["path"], path);
    assert_eq!(graph.node(parsed_id).unwrap().id, id);
    let outline = round_trip(outline_json(&graph, root.path(), path));
    assert_eq!(outline["path"], path);
    assert_eq!(outline["symbols"][0]["id"], id);
    let impact = round_trip(impact_json(&graph, root.path(), parsed_id, &opts));
    assert_eq!(impact["start_ids"][0], id);
    assert_eq!(impact["hits"][0]["id"], caller_id);
    assert_eq!(impact["hits"][0]["path"], caller_path);
    let callers = round_trip(callers_json(&graph, root.path(), parsed_id, &opts));
    assert_eq!(callers["start_ids"][0], id);
    assert_eq!(callers["neighbors"][0]["id"], caller_id);
    assert_eq!(callers["neighbors"][0]["path"], caller_path);
    assert_eq!(callers["neighbors"][0]["site_path"], caller_path);
    let rendered = render_impact(
        &graph,
        root.path(),
        parsed_id,
        &ResolutionStats::default(),
        &opts,
    );
    assert!(!rendered.contains('\t') && !rendered.contains('`'));
    assert!(!rendered.contains(&id));
}

fn import_graph() -> ResolvedGraph {
    let target = node(
        "target#function:t",
        "target",
        SourceNodeKind::Function,
        &["t"],
        FileCoverage::Full,
    );
    let caller = node(
        "caller#function:c",
        "caller",
        SourceNodeKind::Function,
        &["c"],
        FileCoverage::Full,
    );
    let edge = SourceEdge::bound(
        &caller.id,
        &target.id,
        SourceEdgeKind::Imports,
        "t",
        Span::default(),
        EdgeProvenance::LocalName,
    );
    graph_of(vec![
        ("target", target, vec![]),
        ("caller", caller, vec![edge]),
    ])
}

#[test]
fn impact_explains_filters_and_labels_the_traversed_kinds() {
    let mut graph = import_graph();
    let root = TempDir::new().unwrap();
    let stats = ResolutionStats::default();
    let default = view_options(vec![]);
    let rendered = render_impact(&graph, root.path(), "t", &stats, &default);
    assert!(rendered.contains("kinds: calls,references,implements,extends"));
    assert!(rendered.contains("no matching hits under these bounds"));
    assert_eq!(
        impact_json(&graph, root.path(), "t", &default)["kinds"],
        serde_json::json!(["calls", "references", "implements", "extends"])
    );
    let all = view_options(vec![
        SourceEdgeKind::Contains,
        SourceEdgeKind::Imports,
        SourceEdgeKind::Calls,
        SourceEdgeKind::References,
        SourceEdgeKind::Implements,
        SourceEdgeKind::Extends,
    ]);
    assert!(render_impact(&graph, root.path(), "t", &stats, &all).contains("kinds: all"));
    assert_eq!(
        impact_json(&graph, root.path(), "t", &all)["kinds"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert_eq!(
        impact_json(&graph, root.path(), "t", &all)["hits"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    graph.files.get_mut("caller").unwrap().edges[0].kind = SourceEdgeKind::Calls;
    let filtered = ViewOptions {
        filters: ViewFilters::new(Some("elsewhere".into()), None),
        ..all
    };
    let rendered = render_impact(&graph, root.path(), "t", &stats, &filtered);
    assert!(rendered.contains("no matching hits under these bounds and display filters"));
    assert!(!rendered.contains("no resolved edge reaches"));
}
