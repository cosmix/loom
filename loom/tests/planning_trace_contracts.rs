//! Contracts for the planning trace: edge lines, settled marks, the trace build and the
//! findings ledger.

use loom::planning::findings::{self, import_review};
use loom::planning::implements::{self, Implements};
use loom::planning::metaplan::entries;
use loom::planning::project::Project;
use loom::planning::trace::settle::{body_hash, clear, mark, settled_line, SettledLine};
use loom::planning::trace::tokens::SectionRef;
use loom::planning::trace::{CoverKind, GapKind, NodeRef, SettleState};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const VISION: &str = "doc/product/demo/index.md";
const SECTION_01: &str = "doc/product/demo/section_01.md";
const SECTION_02: &str = "doc/product/demo/section_02.md";
const METAPLAN: &str = "doc/plans/demo/index.md";
const PLAN_FIRST: &str = "doc/plans/demo/PLAN-first.md";
const RUNNING_FIRST: &str = "doc/plans/demo/IN_PROGRESS-PLAN-first.md";
const LEDGER: &str = "doc/plans/demo/findings.md";

fn tmp() -> (TempDir, PathBuf) {
    let builder = tempfile::Builder::new().prefix("loom-trace-").clone();
    let temp = builder.tempdir_in(std::env::temp_dir()).unwrap();
    let path = temp.path().to_path_buf();
    (temp, path)
}

fn put(root: &Path, rel: &str, content: &str) {
    fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    fs::write(root.join(rel), content).unwrap();
}

fn read(root: &Path, rel: &str) -> String {
    fs::read_to_string(root.join(rel)).unwrap()
}

/// The DEMO fixture: a vision of two sections, both written, a metaplan of two entries and
/// one plan implementing `section_01#one`.
fn demo() -> (TempDir, PathBuf) {
    let (temp, root) = tmp();
    put(
        &root,
        VISION,
        "# Demo\n\nA demo.\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n\
         | 01 | Alpha |\n| 02 | Beta |\n",
    );
    put(
        &root,
        SECTION_01,
        "# Alpha\n\n## One\n\nText one.\n\n## Two\n\nText two.\n\n## Open questions\n\nNone.\n",
    );
    put(&root, SECTION_02, "# Beta\n\n## Three\n\nText three.\n");
    put(
        &root,
        METAPLAN,
        "# Engineering\n\n## 1. First (`PLAN-first.md`)\n\nCovers: section_01#one\n\n\
         First plan.\n\n## 2. Second (`PLAN-second.md`)\n\nCovers: section_02\n\nSecond plan.\n",
    );
    put(
        &root,
        PLAN_FIRST,
        "# First\n\nImplements: section_01#one\n\nBody.\n",
    );
    (temp, root)
}

/// The DEMO fixture with its plan running.
fn demo_running() -> (TempDir, PathBuf) {
    let (temp, root) = demo();
    fs::rename(root.join(PLAN_FIRST), root.join(RUNNING_FIRST)).unwrap();
    (temp, root)
}

fn review(text: &str) -> String {
    format!(
        "# Code Review: First\n\n## Open Questions\n\nNo open questions.\n\n\
         ## Open Findings\n\n- **Build** (`build`): {text}\n"
    )
}

fn load(root: &Path) -> Project {
    Project::load(root).unwrap()
}

/// Write the Settled line the loaded trace would write now into `rel`.
fn settle_file(root: &Path, rel: &str) {
    let project = load(root);
    let trace = &project.feature("demo").unwrap().trace;
    let doc = trace
        .documents
        .iter()
        .find(|doc| doc.path == Path::new(rel))
        .unwrap_or_else(|| panic!("no trace document for {rel}"));
    let marked = mark(&read(root, rel), &doc.current);
    put(root, rel, &marked);
}

fn assert_states(root: &Path, expected: &[(&str, SettleState)]) {
    let project = load(root);
    let trace = &project.feature("demo").unwrap().trace;
    for (rel, state) in expected {
        let doc = trace
            .documents
            .iter()
            .find(|doc| doc.path == Path::new(rel))
            .unwrap_or_else(|| panic!("no trace document for {rel}"));
        assert_eq!(&doc.settle, state, "settle state of {rel}");
    }
}

fn uncovered(project: &Project) -> Vec<NodeRef> {
    let trace = &project.feature("demo").unwrap().trace;
    trace
        .gaps
        .iter()
        .filter(|gap| matches!(gap.kind, GapKind::Uncovered))
        .map(|gap| NodeRef {
            path: gap.node.path.clone(),
            anchor: gap.node.anchor.clone(),
        })
        .collect()
}

#[test]
fn metaplan_entry_reads_its_covers_line() {
    let fence = "`".repeat(3);
    let text = format!(
        "# E\n\n## 1. A (`PLAN-a.md`)\n\nCovers: section_01#one, section_02, bogus\n\n\
         {fence}\ntext\nCovers: section_09\n{fence}\n\n## 2. B (`PLAN-b.md`)\n\n\
         Covers: section_03\n"
    );
    let found = entries(&text);
    assert_eq!(found.len(), 2);
    assert_eq!(
        found[0].covers,
        vec![
            SectionRef {
                number: 1,
                anchor: Some("one".into())
            },
            SectionRef {
                number: 2,
                anchor: None
            },
        ]
    );
    assert_eq!(found[0].bad_covers, vec!["bogus".to_string()]);
    assert_eq!(
        found[1].covers,
        vec![SectionRef {
            number: 3,
            anchor: None
        }]
    );
    assert_ne!(found[0].span_hash, found[1].span_hash);
}

#[test]
fn implements_keeps_heading_anchors() {
    let line = implements::parse(
        "# X\n\nImplements: section_04#rebuild-budget, section_02, section_04#rebuild-budget, \
         section_04\n",
    );
    assert_eq!(line.value, Implements::Sections(vec![4, 2]));
    assert_eq!(line.anchors, vec![(4, "rebuild-budget".to_string())]);
    assert!(line.bad_tokens.is_empty(), "bad: {:?}", line.bad_tokens);
}

#[test]
fn settle_mark_round_trips() {
    let text = "# Alpha\n\nImplements: section_01\n\nBody.\n";
    let h = body_hash(text, &[]);
    assert_eq!(h.len(), 12, "hash {h}");
    assert!(
        h.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
        "hash {h}"
    );
    let line = SettledLine {
        body: h.clone(),
        stub: None,
        upstream: None,
    };
    let marked = mark(text, &line);
    assert_eq!(
        marked,
        format!("# Alpha\n\nImplements: section_01\n\nSettled: {h}\n\nBody.\n")
    );
    assert_eq!(body_hash(&marked, &[]), h);
    assert_eq!(mark(&marked, &line), marked);
    assert_eq!(settled_line(&marked), Some(line));
    assert_eq!(clear(&marked), text);
}

#[test]
fn settle_states_follow_body_stub_and_upstream() {
    let (_temp, root) = demo();
    assert_states(&root, &[(SECTION_02, SettleState::Draft)]);
    settle_file(&root, SECTION_01);
    settle_file(&root, PLAN_FIRST);
    assert_states(
        &root,
        &[
            (SECTION_01, SettleState::Settled),
            (PLAN_FIRST, SettleState::Settled),
            (SECTION_02, SettleState::Draft),
        ],
    );

    let settled = read(&root, SECTION_01);
    put(
        &root,
        SECTION_01,
        &settled.replace("Text one.", "Text one, changed."),
    );
    assert_states(
        &root,
        &[
            (SECTION_01, SettleState::Revised),
            (PLAN_FIRST, SettleState::UpstreamChanged),
            (SECTION_02, SettleState::Draft),
        ],
    );

    put(&root, SECTION_01, &settled);
    let vision = read(&root, VISION).replace("| 01 | Alpha |", "| 01 | Alpha first |");
    put(&root, VISION, &vision);
    assert_states(
        &root,
        &[
            (SECTION_01, SettleState::StubChanged),
            (SECTION_02, SettleState::Draft),
        ],
    );
}

#[test]
fn uncovered_headings_are_gaps() {
    let (_temp, root) = demo();
    let project = load(&root);
    let expected = NodeRef {
        path: PathBuf::from(SECTION_01),
        anchor: Some("two".into()),
    };
    assert_eq!(uncovered(&project), vec![expected]);
    let trace = &project.feature("demo").unwrap().trace;
    for gap in trace.gaps.iter().chain(trace.frontier.iter()) {
        assert_ne!(gap.node.anchor.as_deref(), Some("open-questions"));
    }
    let one = trace
        .headings
        .iter()
        .find(|h| h.path == Path::new(SECTION_01) && h.anchor == "one")
        .expect("heading section_01#one");
    let kinds = |pick: fn(&CoverKind) -> bool| one.covered_by.iter().any(|c| pick(&c.kind));
    assert!(kinds(|k| matches!(k, CoverKind::Entry)), "no entry cover");
    assert!(kinds(|k| matches!(k, CoverKind::Plan)), "no plan cover");

    put(
        &root,
        PLAN_FIRST,
        "# First\n\nImplements: section_01\n\nBody.\n",
    );
    assert_eq!(uncovered(&load(&root)), Vec::<NodeRef>::new());
}

#[test]
fn review_import_is_idempotent() {
    let (_temp, root) = demo_running();
    let text = review("Rename the flag");
    let plan = Path::new(RUNNING_FIRST);
    assert_eq!(import_review(&root, plan, &text).unwrap(), 1);
    assert_eq!(import_review(&root, plan, &text).unwrap(), 0);

    let ledger = read(&root, LEDGER);
    assert!(ledger.starts_with("# Findings\n"), "ledger:\n{ledger}");
    let entries = ledger.lines().filter(|l| l.starts_with("## F-")).count();
    assert_eq!(entries, 1, "ledger:\n{ledger}");
    let lines: Vec<&str> = ledger.lines().collect();
    assert!(
        lines.contains(&"About: PLAN-first.md#stage:build, section_01#one"),
        "ledger:\n{ledger}"
    );
    assert!(lines.contains(&"Status: open"), "ledger:\n{ledger}");

    put(&root, "doc/plans/PLAN-solo.md", "# Solo\n");
    let solo = Path::new("doc/plans/PLAN-solo.md");
    assert_eq!(import_review(&root, solo, &text).unwrap(), 0);
    assert!(!root.join("doc/plans/findings.md").exists());
}

#[test]
fn ledger_refuses_a_symlinked_file() {
    let (_temp, root) = demo_running();
    let (_outside_temp, outside) = tmp();
    let target = outside.join("findings.md");
    fs::write(&target, "keep\n").unwrap();
    symlink(&target, root.join(LEDGER)).unwrap();

    let result = import_review(&root, Path::new(RUNNING_FIRST), &review("Rename the flag"));
    assert!(result.is_err(), "import followed a symlinked ledger");
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep\n");
}

#[test]
fn finding_text_cannot_open_a_heading() {
    let (_temp, root) = demo_running();
    import_review(&root, Path::new(RUNNING_FIRST), &review("## F-9 injected")).unwrap();
    let ledger = read(&root, LEDGER);
    let headings: Vec<&str> = ledger.lines().filter(|l| l.starts_with("## ")).collect();
    assert_eq!(headings.len(), 1, "ledger:\n{ledger}");
    assert!(headings[0].starts_with("## F-1 "), "ledger:\n{ledger}");
    let parsed = findings::parse(&ledger).findings;
    assert_eq!(parsed.len(), 1, "ledger:\n{ledger}");
    assert!(parsed[0].text.contains("F-9 injected"), "ledger:\n{ledger}");
}

#[test]
fn finding_title_cuts_on_a_char_boundary() {
    let (_temp, root) = demo_running();
    let item = "é".repeat(100);
    import_review(&root, Path::new(RUNNING_FIRST), &review(&item)).unwrap();
    let ledger = read(&root, LEDGER);
    let parsed = findings::parse(&ledger).findings;
    assert_eq!(parsed.len(), 1, "ledger:\n{ledger}");
    assert_eq!(parsed[0].title, "é".repeat(80));
}
