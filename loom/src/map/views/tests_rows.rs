use super::tests::{graph_of, impact_chain_graph, node, view_options};
use super::*;
use crate::context::freshness::GraphState;
use crate::context::refresh::clean_generation;
use crate::context::source_graph::{
    EdgeProvenance, FileCoverage, NodeLanguage, SourceEdge, SourceEdgeKind, SourceNode,
    SourceNodeKind, Span,
};
use crate::context::view::ViewOrigin;
use crate::context::window::SourceWindow;
use crate::map::views::matching::find_symbol_matches;
use crate::map::views::snapshot::SnapshotIdentity;
use tempfile::TempDir;

fn foo_at(id: &str, path: &str) -> SourceNode {
    node(
        id,
        path,
        SourceNodeKind::Function,
        &["foo"],
        FileCoverage::Full,
    )
}

fn filtered(filters: ViewFilters) -> ViewOptions {
    ViewOptions {
        filters,
        ..view_options(Vec::new())
    }
}

#[test]
fn matching_reports_id_exact_and_substring_modes() {
    let graph = impact_chain_graph();

    let (nodes, mode) = find_symbol_matches(&graph, "src/a.rs#function:foo");
    assert_eq!((nodes.len(), mode), (1, MatchMode::Id));
    let (nodes, mode) = find_symbol_matches(&graph, "foo");
    assert_eq!((nodes.len(), mode), (1, MatchMode::Exact));
    let (nodes, mode) = find_symbol_matches(&graph, "FO");
    assert_eq!((nodes.len(), mode), (1, MatchMode::Substring));
    // An id never falls back to a substring, even when a name contains it.
    let (nodes, mode) = find_symbol_matches(&graph, "src/a.rs#function:fo");
    assert_eq!((nodes.len(), mode), (0, MatchMode::Id));
    assert_eq!(MatchMode::Substring.as_str(), "substring");
    assert_eq!(MatchMode::Exact.label(), "");
}

#[test]
fn substring_fallback_is_labelled_in_every_human_view() {
    let graph = impact_chain_graph();
    let root = TempDir::new().unwrap();
    let opts = view_options(Vec::new());

    for rendered in [
        render_find_all(&graph, "fo", &opts),
        render_impact(
            &graph,
            root.path(),
            "fo",
            &ResolutionStats::default(),
            &opts,
        ),
        render_callers(&graph, root.path(), "fo", &opts),
        render_callees(&graph, root.path(), "fo", &opts),
        render_references(&graph, root.path(), "fo", &opts),
    ] {
        assert!(rendered.contains("(substring matches)"), "{rendered}");
    }
    assert!(!render_callers(&graph, root.path(), "foo", &opts).contains("substring"));
}

#[test]
fn find_all_path_filter_is_component_aware_and_reported() {
    let graph = graph_of(vec![
        (
            "src/a/x.rs",
            foo_at("src/a/x.rs#function:foo", "src/a/x.rs"),
            vec![],
        ),
        (
            "src/ab/x.rs",
            foo_at("src/ab/x.rs#function:foo", "src/ab/x.rs"),
            vec![],
        ),
    ]);
    let opts = filtered(ViewFilters::new(Some("./src/a/".to_string()), None));

    let json = find_all_json(&graph, "foo", &opts);

    assert_eq!(json["matches"].as_array().unwrap().len(), 1);
    assert_eq!(json["matches"][0]["path"], "src/a/x.rs");
    assert_eq!(json["filters"]["path"]["value"], "src/a");
    assert_eq!(json["filters"]["path"]["applied"], "scan");
    assert_eq!(json["filters"]["path"]["filtered_out"], 1);
    assert!(render_find_all(&graph, "foo", &opts).contains("filters: path src/a removed 1"));
}

#[test]
fn find_all_lang_filter_counts_what_it_removed() {
    let mut python = foo_at("src/p.py#function:foo", "src/p.py");
    python.language = NodeLanguage::Python;
    let graph = graph_of(vec![
        (
            "src/a.rs",
            foo_at("src/a.rs#function:foo", "src/a.rs"),
            vec![],
        ),
        ("src/p.py", python, vec![]),
    ]);

    let json = find_all_json(
        &graph,
        "foo",
        &filtered(ViewFilters::new(None, Some("python".into()))),
    );

    assert_eq!(json["matches"].as_array().unwrap().len(), 1);
    assert_eq!(json["matches"][0]["path"], "src/p.py");
    assert_eq!(json["filters"]["lang"]["filtered_out"], 1);
    assert_eq!(json["filters"]["path"]["filtered_out"], 0);
}

#[test]
fn find_all_limit_applies_after_the_scan_filters() {
    let graph = graph_of(vec![
        (
            "src/a.rs",
            foo_at("src/a.rs#function:foo", "src/a.rs"),
            vec![],
        ),
        (
            "src/b.rs",
            foo_at("src/b.rs#function:foo", "src/b.rs"),
            vec![],
        ),
        (
            "src/c.rs",
            foo_at("src/c.rs#function:foo", "src/c.rs"),
            vec![],
        ),
    ]);
    let mut opts = view_options(Vec::new());
    opts.limit = 2;

    let json = find_all_json(&graph, "foo", &opts);

    assert_eq!(json["matches"].as_array().unwrap().len(), 2);
    assert_eq!(json["suppressed"], 1);
    assert!(render_find_all(&graph, "foo", &opts).contains("1 more suppressed"));
}

#[test]
fn impact_reports_what_the_path_filter_removed_from_displayed_hits() {
    let graph = impact_chain_graph();
    let root = TempDir::new().unwrap();
    let opts = filtered(ViewFilters::new(Some("src/b.rs".to_string()), None));

    let json = impact_json(&graph, root.path(), "foo", &opts);

    let hits = json["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["id"], "src/b.rs#function:bar");
    assert_eq!(hits[0]["via_candidates"], false);
    assert_eq!(json["filters"]["path"]["applied"], "display");
    assert_eq!(json["filters"]["path"]["filtered_out"], 1);
}

#[test]
fn impact_evidence_restricts_traversal_to_the_listed_provenance() {
    let graph = impact_chain_graph();
    let root = TempDir::new().unwrap();
    let mut opts = view_options(Vec::new());
    opts.provenances = vec![EdgeProvenance::UniqueName];

    let json = impact_json(&graph, root.path(), "foo", &opts);

    // The only edge into `foo` is local-name, so nothing is reachable through
    // unique-name evidence alone.
    assert!(json["hits"].as_array().unwrap().is_empty());
}

#[test]
fn caller_rows_name_the_call_site_and_the_declaration() {
    let mut foo = foo_at("src/a.rs#function:foo", "src/a.rs");
    foo.span = Span {
        start_byte: 0,
        end_byte: 30,
        line_start: 2,
        line_end: 4,
    };
    let bar = node(
        "src/b.rs#function:bar",
        "src/b.rs",
        SourceNodeKind::Function,
        &["bar"],
        FileCoverage::Full,
    );
    let site = Span {
        start_byte: 40,
        end_byte: 45,
        line_start: 9,
        line_end: 9,
    };
    let call = SourceEdge::bound(
        "src/b.rs#function:bar",
        "src/a.rs#function:foo",
        SourceEdgeKind::Calls,
        "foo",
        site,
        EdgeProvenance::Import,
    );
    let graph = graph_of(vec![
        ("src/a.rs", foo, vec![]),
        ("src/b.rs", bar, vec![call]),
    ]);
    let root = TempDir::new().unwrap();
    let opts = view_options(Vec::new());

    let callers = render_callers(&graph, root.path(), "foo", &opts);
    let callees = render_callees(&graph, root.path(), "bar", &opts);

    assert!(
        callers.contains("src/b.rs:L9 → src/a.rs:L2  symbol=foo  import"),
        "{callers}"
    );
    assert!(callers.contains("sites=1"), "{callers}");
    assert!(
        callees.contains("src/b.rs:L9 → src/a.rs:L2  symbol=foo  import"),
        "{callees}"
    );
    assert!(!callers.contains("candidate ("));
}

#[test]
fn candidate_rows_report_the_size_of_their_set() {
    let site = Span {
        start_byte: 30,
        end_byte: 32,
        line_start: 3,
        line_end: 3,
    };
    let ambiguous = SourceEdge::syntax(
        "src/run.rs#function:run",
        SourceEdgeKind::Calls,
        "go",
        site,
        0.3,
    )
    .with_candidates(vec![
        "src/alpha.rs#function:alpha".to_string(),
        "src/beta.rs#function:beta".to_string(),
    ]);
    let func = |name: &str| {
        node(
            &format!("src/{name}.rs#function:{name}"),
            &format!("src/{name}.rs"),
            SourceNodeKind::Function,
            &[name],
            FileCoverage::Full,
        )
    };
    let graph = graph_of(vec![
        ("src/run.rs", func("run"), vec![ambiguous]),
        ("src/alpha.rs", func("alpha"), vec![]),
        ("src/beta.rs", func("beta"), vec![]),
    ]);
    let root = TempDir::new().unwrap();
    let opts = view_options(Vec::new());

    let rendered = render_callers(&graph, root.path(), "alpha", &opts);
    let json = callers_json(&graph, root.path(), "alpha", &opts);

    assert!(rendered.contains("symbol=go"), "{rendered}");
    assert!(rendered.contains("candidate (2 total)"), "{rendered}");
    assert_eq!(json["neighbors"][0]["candidate_of"], 2);
}

#[test]
fn references_with_none_print_an_empty_state() {
    let graph = impact_chain_graph();
    let root = TempDir::new().unwrap();

    let rendered = render_references(&graph, root.path(), "foo", &view_options(Vec::new()));

    assert!(
        rendered.contains("References of src/a.rs#function:foo"),
        "{rendered}"
    );
    assert!(rendered.contains("no direct references"), "{rendered}");
}

#[test]
fn every_outline_row_matches_the_read_discipline_pattern() {
    let mut first = foo_at("src/a.rs#function:foo", "src/a.rs");
    first.span = Span {
        start_byte: 0,
        end_byte: 20,
        line_start: 1,
        line_end: 3,
    };
    first.signature = "fn foo()".to_string();
    let mut second = foo_at("src/a.rs#function:bar", "src/a.rs");
    second.scope = vec!["bar".to_string()];
    second.span = Span {
        start_byte: 21,
        end_byte: 40,
        line_start: 5,
        line_end: 12,
    };
    let mut graph = graph_of(vec![("src/a.rs", first, vec![])]);
    graph.files.get_mut("src/a.rs").unwrap().nodes.push(second);
    let root = TempDir::new().unwrap();

    let rendered = render_outline(&graph, root.path(), "src/a.rs");

    let row = regex::Regex::new(r"^\s+L[0-9]+-L[0-9]+\s").unwrap();
    let rows: Vec<&str> = rendered
        .lines()
        .filter(|line| line.starts_with(' '))
        .collect();
    assert_eq!(rows.len(), 2, "{rendered}");
    assert!(rows.iter().all(|line| row.is_match(line)), "{rendered}");
}

fn identity(overlay: bool) -> SnapshotIdentity {
    let base = "abcdef0123456789".to_string();
    SnapshotIdentity {
        state: GraphState::Current,
        generation: if overlay {
            "fedcba9876543210".to_string()
        } else {
            clean_generation(&base)
        },
        base_revision: base,
        overlay: overlay.then(|| ("local".to_string(), "tree".to_string())),
        built_at: None,
        persisted: true,
        schema_version: 2,
        resolver_version: 1,
        view: ViewOrigin::Built,
        extractors: Default::default(),
    }
}

#[test]
fn window_header_names_state_base_and_generation() {
    let window = SourceWindow {
        path: "src/a.rs".to_string(),
        span: Span {
            start_byte: 0,
            end_byte: 10,
            line_start: 3,
            line_end: 4,
        },
        text: "one\x1b[31m\r\ntwo\r\n".to_string(),
        truncated: true,
    };

    let clean = render_window(&window, &identity(false));
    let overlaid = render_window(&window, &identity(true));

    assert!(
        clean.starts_with("src/a.rs:L3-L4  current abcdef01  hash match\n"),
        "{clean}"
    );
    assert!(
        overlaid.starts_with("src/a.rs:L3-L4  current abcdef01+fedcba98  hash match\n"),
        "{overlaid}"
    );
    assert!(
        !clean.contains('\u{1b}') && !clean.contains('\r'),
        "{clean:?}"
    );
    assert!(clean.contains("two"));
    assert!(clean.ends_with("... truncated (raise --window-lines)"));
    let unusual = SourceWindow {
        path: "src/`tab\tline\n.rs".into(),
        ..window
    };
    let json: serde_json::Value =
        serde_json::from_str(&window_json(&unusual, &identity(false)).to_string()).unwrap();
    assert_eq!(json["path"], unusual.path);
    assert!(!render_window(&unusual, &identity(false)).contains('`'));
}

#[test]
fn filter_parsers_accept_registry_names_and_reject_others() {
    assert_eq!(parse_language("Rust").as_deref(), Ok("rust"));
    assert!(parse_language("cobol")
        .unwrap_err()
        .contains("valid languages"));
    assert_eq!(parse_provenance("import"), Ok(EdgeProvenance::Import));
    assert!(parse_provenance("guess")
        .unwrap_err()
        .contains("local-name"));
}
