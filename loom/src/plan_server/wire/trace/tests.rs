//! The trace wire: the fixture project's view is checked byte for byte against
//! `wire/fixtures/trace.json`, which the web schema test parses too.
//!
//! `LOOM_WRITE_FIXTURES=1 cargo test ... trace_fixture` rewrites the file; copy the same bytes
//! to `web/src/plan/api/fixtures/trace.json`.

use std::path::{Path, PathBuf};

use super::super::tests_schema::assert_all_in_source;
use super::*;
use crate::planning::project::Project;
use crate::planning::trace::settle;

const SCHEMA_TRACE_TS: &str = include_str!("../../../../../web/src/plan/api/schema-trace.ts");

const VISION: &str = "# Demo\n\nA demo.\n\n## Specification sections\n\n| # | Section |\n\
                      | --- | --- |\n| 01 | Alpha |\n| 02 | Beta |\n| 03 | Gamma |\n";
const SECTION_01: &str = "# Alpha\n\n## One\n\nText one.\n\n## Two\n\nText two.\n\n\
                          ## Open questions\n\n- Which flag?\n";
const SECTION_02: &str = "# Beta\n\n## Three\n\nText three. (My call.)\n";
const METAPLAN: &str =
    "# Engineering\n\n## 1. First (`PLAN-first.md`)\n\nCovers: section_01#one\n\n\
                        First plan.\n\n## 2. Second (`PLAN-second.md`)\n\nCovers: section_02\n\n\
                        Second plan.\n";
const PLAN: &str =
    "# First\n\nImplements: section_01#one\n\nImplements (stage:build): section_01#two\n\n\
                    Body.\n";
const LEDGER: &str = "# Findings\n\n\
    ## F-1 Rebuild ignores briefs\n\nAbout: PLAN-first.md#stage:build, section_01#two\n\
    Source: REVIEW-PLAN-first.md#open-findings (build, 1a2b3c4d5e6f)\nStatus: open\n\n\
    The rebuild budget counts plans but not briefs.\n\n\
    ## F-2 Rename the flag\n\nAbout: section_02\nStatus: dismissed: not needed\n\nRename it.\n";

fn put(root: &Path, rel: &str, text: &str) {
    std::fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    std::fs::write(root.join(rel), text).unwrap();
}

/// Settle the document at `rel` as the loaded project describes it.
fn settle_file(root: &Path, rel: &str) {
    let project = Project::load(root).unwrap();
    let trace = &project.feature("demo").unwrap().trace;
    let doc = trace
        .documents
        .iter()
        .find(|d| d.path == Path::new(rel))
        .unwrap();
    let text = std::fs::read_to_string(root.join(rel)).unwrap();
    put(root, rel, &settle::mark(&text, &doc.current));
}

/// The fixture project: section 01 settled, section 02 settled and edited since, a plan with
/// a stage line, a ledger of an open and a dismissed finding, and a section only the vision lists.
fn fixture_view() -> TraceView {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "doc/product/demo/index.md", VISION);
    put(root, "doc/product/demo/section_01.md", SECTION_01);
    put(root, "doc/product/demo/section_02.md", SECTION_02);
    put(root, "doc/plans/demo/index.md", METAPLAN);
    put(root, "doc/plans/demo/PLAN-first.md", PLAN);
    put(root, "doc/plans/demo/findings.md", LEDGER);
    put(
        root,
        "doc/plans/demo/briefs/first/build/r1.md",
        "# Brief\n\nBuild it.\n",
    );
    settle_file(root, "doc/product/demo/section_01.md");
    settle_file(root, "doc/product/demo/section_02.md");
    let edited = format!("{}\n## Four\n\nText four.\n", SECTION_02);
    let settled = std::fs::read_to_string(root.join("doc/product/demo/section_02.md")).unwrap();
    put(
        root,
        "doc/product/demo/section_02.md",
        &format!("{settled}{}", &edited[SECTION_02.len()..]),
    );
    let project = Project::load(root).unwrap();
    TraceView::from(&project.feature("demo").unwrap().trace)
}

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/plan_server/wire/fixtures/trace.json")
}

fn render(view: &TraceView) -> String {
    let mut text = serde_json::to_string_pretty(view).unwrap();
    text.push('\n');
    text
}

#[test]
fn trace_fixture_matches_serde_output() {
    let view = fixture_view();
    let written = render(&view);
    if std::env::var("LOOM_WRITE_FIXTURES").as_deref() == Ok("1") {
        std::fs::write(fixture_path(), &written).unwrap();
    }
    let fixture = std::fs::read_to_string(fixture_path()).expect("wire/fixtures/trace.json");
    assert_eq!(fixture, written, "regenerate with LOOM_WRITE_FIXTURES=1");
    let parsed: TraceView = serde_json::from_str(&fixture).unwrap();
    assert_eq!(parsed, view);
}

/// The web schema test parses the web copy; it must stay the bytes the writer test checks
/// (`web/.oxfmtrc.json` ignores the file so the formatter leaves them alone).
#[test]
fn the_web_trace_fixture_is_byte_identical() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let web = std::fs::read(root.join("../web/src/plan/api/fixtures/trace.json"))
        .expect("web/src/plan/api/fixtures/trace.json");
    let rust = std::fs::read(fixture_path()).expect("wire/fixtures/trace.json");
    assert!(
        web == rust,
        "copy wire/fixtures/trace.json to the web fixture"
    );
}

#[test]
fn the_fixture_view_shows_each_part_of_the_trace() {
    let view = fixture_view();
    let state = |path: &str| {
        view.documents
            .iter()
            .find(|d| d.path == path)
            .map(|d| d.settle)
    };
    assert_eq!(
        state("doc/product/demo/section_01.md"),
        Some(SettleStateView::Settled)
    );
    assert_eq!(
        state("doc/product/demo/section_02.md"),
        Some(SettleStateView::Revised)
    );
    assert!(view
        .documents
        .iter()
        .any(|d| d.progress == ProgressView::Stub));
    assert!(view.headings.iter().any(|h| {
        h.covered_by
            .iter()
            .any(|c| c.kind == CoverKindView::Stage && c.stage.as_deref() == Some("build"))
    }));
    let kinds: Vec<FindingKindView> = view
        .findings
        .iter()
        .map(|f| f.finding.status.kind)
        .collect();
    assert_eq!(kinds, [FindingKindView::Open, FindingKindView::Dismissed]);
    assert_eq!(
        view.findings[1].finding.status.detail.as_deref(),
        Some("not needed")
    );
    assert!(view.documents.iter().any(|d| d.my_calls == 1));
    assert!(view
        .documents
        .iter()
        .any(|d| d.bundle == ["doc/plans/demo/briefs/first/build/r1.md"]));
    assert!(!view.frontier.is_empty() && view.frontier.len() <= 5);
}

#[test]
fn views_reject_unknown_fields() {
    let mut value = serde_json::to_value(fixture_view()).unwrap();
    value["extra"] = serde_json::json!(true);
    assert!(serde_json::from_value::<TraceView>(value).is_err());
}

// Each match fails to compile when a variant is added without a case here.
fn all_doc_roles() -> Vec<DocRoleView> {
    match DocRoleView::Vision {
        DocRoleView::Vision | DocRoleView::Section | DocRoleView::Metaplan | DocRoleView::Plan => {}
    }
    use DocRoleView::*;
    vec![Vision, Section, Metaplan, Plan]
}

fn all_progress() -> Vec<ProgressView> {
    match ProgressView::Stub {
        ProgressView::Stub
        | ProgressView::Written
        | ProgressView::Planned
        | ProgressView::Built
        | ProgressView::Running
        | ProgressView::Done => {}
    }
    use ProgressView::*;
    vec![Stub, Written, Planned, Built, Running, Done]
}

fn all_settle_states() -> Vec<SettleStateView> {
    match SettleStateView::Draft {
        SettleStateView::Draft
        | SettleStateView::Revised
        | SettleStateView::StubChanged
        | SettleStateView::UpstreamChanged
        | SettleStateView::Settled => {}
    }
    use SettleStateView::*;
    vec![Draft, Revised, StubChanged, UpstreamChanged, Settled]
}

fn all_gap_kinds() -> Vec<GapKindView> {
    match GapKindView::Revised {
        GapKindView::Revised
        | GapKindView::StubChanged
        | GapKindView::UpstreamChanged
        | GapKindView::Revisit
        | GapKindView::Settle
        | GapKindView::Uncovered
        | GapKindView::Triage => {}
    }
    use GapKindView::*;
    vec![
        Revised,
        StubChanged,
        UpstreamChanged,
        Revisit,
        Settle,
        Uncovered,
        Triage,
    ]
}

fn all_cover_and_finding_kinds() -> (Vec<CoverKindView>, Vec<FindingKindView>) {
    match (CoverKindView::Entry, FindingKindView::Open) {
        (
            CoverKindView::Entry | CoverKindView::Plan | CoverKindView::Stage,
            FindingKindView::Open
            | FindingKindView::Planned
            | FindingKindView::Folded
            | FindingKindView::Dismissed,
        ) => {}
    }
    (
        vec![
            CoverKindView::Entry,
            CoverKindView::Plan,
            CoverKindView::Stage,
        ],
        vec![
            FindingKindView::Open,
            FindingKindView::Planned,
            FindingKindView::Folded,
            FindingKindView::Dismissed,
        ],
    )
}

#[test]
fn trace_enum_wire_values_are_in_trace_schema() {
    let ts = SCHEMA_TRACE_TS;
    assert_all_in_source(ts, &all_doc_roles(), "DocRoleView", "DOC_ROLES", None);
    assert_all_in_source(ts, &all_progress(), "ProgressView", "PROGRESS_KINDS", None);
    assert_all_in_source(
        ts,
        &all_settle_states(),
        "SettleStateView",
        "SETTLE_STATES",
        None,
    );
    assert_all_in_source(ts, &all_gap_kinds(), "GapKindView", "GAP_KINDS", None);
    let (covers, findings) = all_cover_and_finding_kinds();
    assert_all_in_source(ts, &covers, "CoverKindView", "COVER_KINDS", None);
    assert_all_in_source(ts, &findings, "FindingKindView", "FINDING_KINDS", None);
}
