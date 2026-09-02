//! Resolution of paths as written: qualified calls, and imports anchored on a
//! crate root or on the module the citing file belongs to.

use super::fixtures::*;
use super::*;
use crate::context::graph_store::FileEntry;
use crate::context::source_graph::{SourceNodeKind, UNRESOLVED_TARGET};

const LIB: &str = "loom/src/lib.rs";

/// The files given plus a crate root, since `crate::` needs one to anchor on.
fn crate_graph(files: Vec<(&'static str, FileEntry)>) -> ResolvedGraph {
    let mut entries = vec![(LIB, source_file(LIB, &[], vec![]))];
    entries.extend(files);
    graph_of(entries)
}

/// An unresolved edge of `kind` leaving `from`, naming `symbol`.
fn seeking(from: &str, kind: SourceEdgeKind, symbol: &str) -> Vec<SourceEdge> {
    vec![SourceEdge::unresolved(from, kind, symbol)]
}

/// A file with no symbols, present only as a path something can match.
fn empty_file(path: &'static str) -> (&'static str, FileEntry) {
    (path, source_file(path, &[], vec![]))
}

#[test]
fn a_crate_rooted_import_anchors_on_the_crate_root() {
    let mut graph = crate_graph(vec![
        (
            "loom/src/app.rs",
            source_file(
                "loom/src/app.rs",
                &[],
                seeking(
                    "loom/src/app.rs",
                    SourceEdgeKind::Imports,
                    "crate::codex::{CODEX_DOMAINS, CODEX_PATHS}",
                ),
            ),
        ),
        empty_file("loom/src/codex.rs"),
        empty_file("loom/src/deep/nested/codex.rs"),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(
        graph.files["loom/src/app.rs"].edges[0].to, "loom/src/codex.rs",
        "`crate::codex` is the crate root's codex.rs, not the one nested deeper"
    );
    assert_eq!(stats.retargeted, 1);
}

#[test]
fn an_import_group_ends_the_path_even_when_it_wraps() {
    let mut graph = crate_graph(vec![
        (
            "loom/src/app.rs",
            source_file(
                "loom/src/app.rs",
                &[],
                seeking(
                    "loom/src/app.rs",
                    SourceEdgeKind::Imports,
                    "crate::a::b::{\n    First,\n    Second,\n}",
                ),
            ),
        ),
        empty_file("loom/src/a/b.rs"),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(
        graph.files["loom/src/app.rs"].edges[0].to,
        "loom/src/a/b.rs"
    );
    assert_eq!(stats.retargeted, 1);
}

#[test]
fn a_super_import_anchors_on_the_module_above_the_citing_file() {
    let citing = "loom/src/skills/install_layout.rs";
    let mut graph = crate_graph(vec![
        (
            citing,
            source_file(
                citing,
                &[],
                seeking(
                    citing,
                    SourceEdgeKind::Imports,
                    "super::index_catalog::{is_core_skill}",
                ),
            ),
        ),
        empty_file("loom/src/skills/index_catalog.rs"),
        empty_file("loom/src/other/index_catalog.rs"),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(
        graph.files[citing].edges[0].to, "loom/src/skills/index_catalog.rs",
        "`super::` names the module above the file, which two same-named files cannot both be"
    );
    assert_eq!(stats.retargeted, 1);
}

/// `use super::*` at the top of a file means the module above it; the same line
/// inside an inline `mod tests` means the file itself. Extraction records
/// neither, so a relative path naming its own module resolves to nothing.
#[test]
fn a_relative_path_naming_its_own_module_stays_unresolved() {
    let citing = "loom/src/skills/install_layout.rs";
    let mut graph = crate_graph(vec![
        (
            citing,
            source_file(
                citing,
                &[],
                seeking(citing, SourceEdgeKind::Imports, "super::*"),
            ),
        ),
        empty_file("loom/src/skills/mod.rs"),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(graph.files[citing].edges[0].to, UNRESOLVED_TARGET);
    assert_eq!(stats.retargeted, 0);
}

/// A crate-rooted path may land on the crate root's own file, where an item is
/// often re-exported: `crate::helper` is `helper` in `lib.rs`.
#[test]
fn a_crate_rooted_path_can_land_on_the_crate_root_file() {
    let caller = func_id("loom/src/app.rs", "invoke");
    let mut graph = graph_of(vec![
        (LIB, source_file(LIB, &["helper"], vec![])),
        (
            "loom/src/app.rs",
            source_file(
                "loom/src/app.rs",
                &["invoke"],
                seeking(&caller, SourceEdgeKind::Calls, "crate::helper"),
            ),
        ),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(
        graph.files["loom/src/app.rs"].edges[0].to,
        func_id(LIB, "helper")
    );
    assert_eq!(stats.retargeted, 1);
}

#[test]
fn a_qualified_call_picks_the_definition_its_path_names() {
    let caller = func_id("src/app.rs", "invoke");
    let mut graph = graph_of(vec![
        (
            "src/app.rs",
            source_file(
                "src/app.rs",
                &["invoke"],
                seeking(&caller, SourceEdgeKind::Calls, "Widget::new"),
            ),
        ),
        (
            "src/widget.rs",
            nested_file("src/widget.rs", &[&["Widget", "new"]], vec![]),
        ),
        (
            "src/gadget.rs",
            nested_file("src/gadget.rs", &[&["Gadget", "new"]], vec![]),
        ),
    ]);

    let stats = resolve_graph(&mut graph);

    let expected = nested_node(
        "src/widget.rs",
        SourceNodeKind::Function,
        &["Widget", "new"],
    );
    assert_eq!(graph.files["src/app.rs"].edges[0].to, expected.id);
    assert_eq!(
        graph.files["src/app.rs"].edges[0].confidence,
        UNIQUE_MATCH_CONFIDENCE
    );
    assert_eq!(stats.retargeted, 1);
}

#[test]
fn a_bare_call_two_types_both_answer_to_stays_unresolved() {
    let caller = func_id("src/app.rs", "invoke");
    let mut graph = graph_of(vec![
        (
            "src/app.rs",
            source_file(
                "src/app.rs",
                &["invoke"],
                seeking(&caller, SourceEdgeKind::Calls, "new"),
            ),
        ),
        (
            "src/widget.rs",
            nested_file("src/widget.rs", &[&["Widget", "new"]], vec![]),
        ),
        (
            "src/gadget.rs",
            nested_file("src/gadget.rs", &[&["Gadget", "new"]], vec![]),
        ),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(graph.files["src/app.rs"].edges[0].to, UNRESOLVED_TARGET);
    assert_eq!(
        stats,
        ResolutionStats {
            retargeted: 0,
            ambiguous: 1,
            unresolved: 1
        },
        "dropping the qualifier is what makes `new` a contest"
    );
}

#[test]
fn a_qualified_call_two_files_both_spell_stays_unresolved() {
    let caller = func_id("src/app.rs", "invoke");
    let mut graph = graph_of(vec![
        (
            "src/app.rs",
            source_file(
                "src/app.rs",
                &["invoke"],
                seeking(&caller, SourceEdgeKind::Calls, "Widget::new"),
            ),
        ),
        (
            "src/one.rs",
            nested_file("src/one.rs", &[&["Widget", "new"]], vec![]),
        ),
        (
            "src/two.rs",
            nested_file("src/two.rs", &[&["Widget", "new"]], vec![]),
        ),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(graph.files["src/app.rs"].edges[0].to, UNRESOLVED_TARGET);
    assert_eq!(stats.ambiguous, 1);
}

#[test]
fn a_call_qualified_by_an_unknown_type_never_matches_a_namesake() {
    let caller = func_id("src/app.rs", "invoke");
    let mut graph = graph_from(vec![
        (
            "src/app.rs",
            &["invoke"],
            seeking(&caller, SourceEdgeKind::Calls, "String::from"),
        ),
        ("src/defs.rs", &["from"], vec![]),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(
        graph.files["src/app.rs"].edges[0].to, UNRESOLVED_TARGET,
        "nothing here defines `String`, so the local `from` is a namesake"
    );
    assert_eq!(
        stats,
        ResolutionStats {
            retargeted: 0,
            ambiguous: 0,
            unresolved: 1
        }
    );
}

#[test]
fn a_module_qualified_call_resolves_inside_the_file_it_names() {
    let caller = func_id("loom/src/app.rs", "invoke");
    let mut graph = crate_graph(vec![
        (
            "loom/src/app.rs",
            source_file(
                "loom/src/app.rs",
                &["invoke"],
                seeking(&caller, SourceEdgeKind::Calls, "crate::codex::run"),
            ),
        ),
        (
            "loom/src/codex.rs",
            source_file("loom/src/codex.rs", &["run"], vec![]),
        ),
        (
            "loom/src/other.rs",
            source_file("loom/src/other.rs", &["run"], vec![]),
        ),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(
        graph.files["loom/src/app.rs"].edges[0].to,
        func_id("loom/src/codex.rs", "run"),
        "the path named the module, so a `run` elsewhere is not a rival"
    );
    assert_eq!(stats.retargeted, 1);
}
