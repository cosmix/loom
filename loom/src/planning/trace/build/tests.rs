mod done;
mod frontier;
mod ledger;
mod refresh;

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::planning::project::Project;
use crate::planning::trace::settle::mark;
use crate::planning::trace::{
    CoverKind, CoverRef, DocRole, GapKind, NodeRef, Progress, SettleState, Trace, TraceDoc,
    TraceHeading,
};
use crate::planning::watch::Batch;

const VISION: &str = "doc/product/demo/index.md";
const METAPLAN: &str = "doc/plans/demo/index.md";
const LEDGER: &str = "doc/plans/demo/findings.md";
const PLAN_FIRST: &str = "doc/plans/demo/PLAN-first.md";

fn section(number: u32) -> String {
    format!("doc/product/demo/section_{number:02}.md")
}

/// A vision listing `numbers`, row `n` named `S<n>`.
fn vision(numbers: &[u32]) -> String {
    let rows: String = numbers
        .iter()
        .map(|n| format!("| {n:02} | S{n} |\n"))
        .collect();
    format!("# Demo\n\n## Specification sections\n\n| # | Name |\n| --- | --- |\n{rows}")
}

struct Fixture {
    temp: TempDir,
}

impl Fixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let fixture = Fixture {
            temp: TempDir::new().unwrap(),
        };
        for (rel, text) in files {
            fixture.put(rel, text);
        }
        fixture
    }

    fn put(&self, rel: &str, text: &str) {
        let path = self.temp.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.temp.path().join(rel)).unwrap()
    }

    fn rename(&self, from: &str, to: &str) {
        let root = self.temp.path();
        fs::rename(root.join(from), root.join(to)).unwrap();
    }

    fn load(&self) -> Project {
        Project::load(self.temp.path()).unwrap()
    }

    fn trace(&self) -> Trace {
        self.load().feature("demo").unwrap().trace.clone()
    }

    /// Write the line settling `rel` would write now.
    fn settle(&self, rel: &str) {
        let trace = self.trace();
        let marked = mark(&self.read(rel), &doc(&trace, rel).current);
        self.put(rel, &marked);
    }
}

fn doc<'a>(trace: &'a Trace, rel: &str) -> &'a TraceDoc {
    let found = trace
        .documents
        .iter()
        .find(|doc| doc.path == Path::new(rel));
    found.unwrap_or_else(|| panic!("no trace document {rel}"))
}

fn heading<'a>(trace: &'a Trace, rel: &str, anchor: &str) -> &'a TraceHeading {
    let found = trace
        .headings
        .iter()
        .find(|h| h.path == Path::new(rel) && h.anchor == anchor);
    found.unwrap_or_else(|| panic!("no heading {rel}#{anchor}"))
}

fn node(rel: &str, anchor: Option<&str>) -> NodeRef {
    NodeRef {
        path: PathBuf::from(rel),
        anchor: anchor.map(str::to_owned),
    }
}

fn uncovered(trace: &Trace) -> Vec<NodeRef> {
    let gaps = trace
        .gaps
        .iter()
        .filter(|gap| gap.kind == GapKind::Uncovered);
    gaps.map(|gap| gap.node.clone()).collect()
}

#[test]
fn a_plan_whose_brief_changes_reads_revised() {
    let (plan, brief) = (PLAN_FIRST, "doc/plans/demo/briefs/first/build/brief.md");
    let fx = Fixture::new(&[
        (VISION, &vision(&[1])),
        (&section(1), "# S1\n\n## One\n\nText.\n"),
        (plan, "# First\n\nImplements: section_01\n\nBody.\n"),
        (brief, "# Brief\n\nDo it.\n"),
    ]);
    assert_eq!(doc(&fx.trace(), plan).bundle, [PathBuf::from(brief)]);
    fx.settle(plan);
    assert_eq!(doc(&fx.trace(), plan).settle, SettleState::Settled);
    fx.put(brief, "# Brief\n\nDo it differently.\n");
    assert_eq!(doc(&fx.trace(), plan).settle, SettleState::Revised);
}

#[test]
fn settling_a_section_is_no_upstream_change_for_a_plan_implementing_it_whole() {
    let plan = PLAN_FIRST;
    let fx = Fixture::new(&[
        (VISION, &vision(&[1])),
        (&section(1), "# S1\n\n## One\n\nText.\n"),
        (plan, "# First\n\nImplements: section_01\n\nBody.\n"),
    ]);
    fx.settle(plan);
    fx.settle(&section(1));
    let trace = fx.trace();
    assert_eq!(doc(&trace, plan).settle, SettleState::Settled);
    assert_eq!(doc(&trace, &section(1)).settle, SettleState::Settled);
    fx.put(
        &section(1),
        &fx.read(&section(1)).replace("Text.", "Changed."),
    );
    assert_eq!(doc(&fx.trace(), plan).settle, SettleState::UpstreamChanged);
}

/// Sections 01 and 02 covered by an entry (`#one`), a stage line (`#two`) and a plan naming
/// section 02 whole.
fn coverage_fixture() -> Fixture {
    let one = "# S1\n\n## One\n\nText.\n\n## Two\n\nText.\n\n## Out of scope\n\nNone.\n";
    let entry = "# E\n\n## 1. First (`PLAN-first.md`)\n\nCovers: section_01#one\n";
    let plan = "# First\n\nImplements: section_02\n\nImplements (stage:build): section_01#two\n";
    Fixture::new(&[
        (VISION, &vision(&[1, 2])),
        (&section(1), one),
        (
            &section(2),
            "# S2\n\n## Three\n\nText.\n\n## Four\n\nText.\n",
        ),
        (METAPLAN, entry),
        (PLAN_FIRST, plan),
    ])
}

fn cover(kind: CoverKind, path: &str, anchor: Option<&str>, stage: Option<&str>) -> CoverRef {
    CoverRef {
        kind,
        path: PathBuf::from(path),
        anchor: anchor.map(str::to_owned),
        slug: Some("first".to_owned()),
        stage: stage.map(str::to_owned),
    }
}

#[test]
fn coverage_comes_from_entries_plans_and_stage_lines() {
    let trace = coverage_fixture().trace();
    let (one, two) = (section(1), section(2));
    let anchors: Vec<(String, &str)> = trace
        .headings
        .iter()
        .map(|h| (h.path.display().to_string(), h.anchor.as_str()))
        .collect();
    let expected = [
        (&one, "one"),
        (&one, "two"),
        (&two, "three"),
        (&two, "four"),
    ];
    assert_eq!(
        anchors,
        expected.map(|(path, anchor)| (path.clone(), anchor))
    );
    let entry = Some(trace.entries[0].anchor.as_str());
    let by_entry = cover(CoverKind::Entry, METAPLAN, entry, None);
    assert_eq!(heading(&trace, &one, "one").covered_by, [by_entry]);
    let by_stage = cover(
        CoverKind::Stage,
        PLAN_FIRST,
        Some("stage:build"),
        Some("build"),
    );
    assert_eq!(heading(&trace, &one, "two").covered_by, [by_stage]);
    let by_plan = cover(CoverKind::Plan, PLAN_FIRST, None, None);
    assert_eq!(heading(&trace, &two, "four").covered_by, [by_plan]);
    assert!(uncovered(&trace).is_empty(), "{:?}", uncovered(&trace));
}

#[test]
fn a_section_named_whole_covers_every_heading_its_anchors_leave_out() {
    let fx = coverage_fixture();
    let (one, two) = (section(1), section(2));
    fx.put(PLAN_FIRST, "# First\n\nImplements: section_02#three\n");
    let expected = [node(&one, Some("two")), node(&two, Some("four"))];
    assert_eq!(uncovered(&fx.trace()), expected);
    fx.put(
        PLAN_FIRST,
        "# First\n\nImplements: section_02#three, section_02\n",
    );
    assert_eq!(uncovered(&fx.trace()), [node(&one, Some("two"))]);
}

/// Section 01 built, 02 planned by a done plan and an unwritten entry, 03 planned by a
/// retired plan only, 04 written and 05 listed only; plans unwritten, done, retired, running.
fn progress_fixture() -> Fixture {
    Fixture::new(&[
        (VISION, &vision(&[1, 2, 3, 4, 5])),
        (&section(1), "# S1\n\n## A\n\nText.\n"),
        (&section(2), "# S2\n\n## X\n\nText.\n"),
        (&section(3), "# S3\n\n## C\n\nText.\n"),
        (&section(4), "# S4\n\n## D\n\nText.\n"),
        (
            METAPLAN,
            "# E\n\n## 1. B (`PLAN-b.md`)\n\nCovers: section_02#x\n",
        ),
        (DONE_A, "# A\n\nImplements: section_01, section_02\n"),
        (RETIRED_C, "# C\n\nImplements: section_01, section_03\n"),
        (RUNNING_D, "# D\n\nImplements: none\n"),
    ])
}

const STUB_B: &str = "doc/plans/demo/PLAN-b.md";
const DONE_A: &str = "doc/plans/demo/DONE-PLAN-a.md";
const RETIRED_C: &str = "doc/plans/demo/RETIRED-PLAN-c.md";
const RUNNING_D: &str = "doc/plans/demo/IN_PROGRESS-PLAN-d.md";

#[test]
fn progress_follows_files_covers_and_plan_states() {
    let trace = progress_fixture().trace();
    let found: Vec<(String, Progress)> = trace
        .documents
        .iter()
        .map(|doc| (doc.path.display().to_string(), doc.progress))
        .collect();
    let sections: Vec<String> = (1..=5).map(section).collect();
    let expected: [(&str, Progress); 11] = [
        (VISION, Progress::Written),
        (&sections[0], Progress::Built),
        (&sections[1], Progress::Planned),
        (&sections[2], Progress::Planned),
        (&sections[3], Progress::Written),
        (&sections[4], Progress::Stub),
        (METAPLAN, Progress::Written),
        (STUB_B, Progress::Stub),
        (DONE_A, Progress::Done),
        (RETIRED_C, Progress::Done),
        (RUNNING_D, Progress::Running),
    ];
    assert_eq!(
        found,
        expected.map(|(path, progress)| (path.to_owned(), progress))
    );
}

#[test]
fn stubs_name_their_vision_row_or_metaplan_entry() {
    let trace = progress_fixture().trace();
    let listed = doc(&trace, &section(5));
    assert_eq!((listed.role, listed.number), (DocRole::Section, Some(5)));
    assert_eq!(listed.title, "S5");
    let table = node(VISION, Some("specification-sections"));
    assert_eq!(listed.stub, Some(table));
    let stub = doc(&trace, STUB_B);
    assert_eq!(stub.title, "1. B (`PLAN-b.md`)");
    assert_eq!(stub.slug.as_deref(), Some("b"));
    let entry = node(METAPLAN, Some(trace.entries[0].anchor.as_str()));
    assert_eq!(stub.stub, Some(entry));
    assert_eq!(
        (stub.settle, stub.current.stub.is_some()),
        (SettleState::Draft, true)
    );
}

#[test]
fn counts_my_calls_and_open_questions() {
    let fx = Fixture::new(&[
        (VISION, &vision(&[1])),
        (
            &section(1),
            "# S1\n\n## One\n\nA (My call.) and a (my call: why).\n\n## Open questions\n\n- Q?\n",
        ),
    ]);
    let trace = fx.trace();
    let found = doc(&trace, &section(1));
    assert_eq!((found.my_calls, found.open_questions), (2, 1));
}

#[test]
fn refresh_builds_the_same_trace_as_a_fresh_load() {
    let fx = Fixture::new(&[
        (VISION, &vision(&[1])),
        (&section(1), "# S1\n\n## One\n\nText.\n"),
        (
            METAPLAN,
            "# E\n\n## 1. P (`PLAN-p.md`)\n\nCovers: section_01\n",
        ),
    ]);
    let mut project = fx.load();
    fx.put(
        LEDGER,
        "# Findings\n\n## F-1 One\n\nAbout: section_01#one\nStatus: open\n\nText.\n",
    );
    fx.put(&section(1), "# S1\n\n## One\n\nText.\n\n## Two\n\nMore.\n");
    let batch = Batch {
        added: vec![PathBuf::from(LEDGER)],
        changed: vec![PathBuf::from(section(1))],
        ..Batch::default()
    };
    project.refresh(&batch).unwrap();
    assert_eq!(project, fx.load());
    let trace = &project.feature("demo").unwrap().trace;
    assert_eq!(trace.findings.len(), 1);
    assert_eq!(heading(trace, &section(1), "one").findings, ["F-1"]);
}

#[test]
fn a_plan_whose_non_markdown_brief_changes_reads_revised() {
    let (plan, schema) = (PLAN_FIRST, "doc/plans/demo/briefs/first/build/schema.json");
    let fx = Fixture::new(&[
        (VISION, &vision(&[1])),
        (&section(1), "# S1\n\n## One\n\nText.\n"),
        (plan, "# First\n\nImplements: section_01\n\nBody.\n"),
        (schema, "{\"a\": 1}\n"),
    ]);
    assert_eq!(doc(&fx.trace(), plan).bundle, [PathBuf::from(schema)]);
    fx.settle(plan);
    assert_eq!(doc(&fx.trace(), plan).settle, SettleState::Settled);
    fx.put(schema, "{\"a\": 2}\n");
    assert_eq!(doc(&fx.trace(), plan).settle, SettleState::Revised);
}

#[test]
fn settling_a_section_is_no_upstream_change_for_a_plan_implementing_its_h1_heading() {
    let (plan, section_path) = (PLAN_FIRST, section(1));
    let fx = Fixture::new(&[
        (VISION, &vision(&[1])),
        (&section_path, "# S1\n\n## One\n\nText.\n"),
        (plan, "# First\n\nImplements: section_01#s1\n\nBody.\n"),
    ]);
    fx.settle(plan);
    assert_eq!(doc(&fx.trace(), plan).settle, SettleState::Settled);
    fx.settle(&section_path);
    assert_eq!(doc(&fx.trace(), plan).settle, SettleState::Settled);
    fx.put(
        &section_path,
        &format!("{}\nMore.\n", fx.read(&section_path)),
    );
    assert_eq!(doc(&fx.trace(), plan).settle, SettleState::UpstreamChanged);
}
