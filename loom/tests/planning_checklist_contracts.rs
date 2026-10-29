//! Contracts for `loom::planning`'s checklist, document parsers and loaded project view.

use loom::planning::checklist::{Checklist, CurrentStep, ItemStatus, Offer, Step, StepMark};
use loom::planning::implements::{parse, Implements};
use loom::planning::project::Project;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[path = "planning_checklist_contracts/features.rs"]
mod features;
#[path = "planning_checklist_contracts/project_view.rs"]
mod project_view;

const META: &str = "doc/plans/demo/index.md";
const VISION: &str = "doc/product/demo/index.md";
const SECTIONS: [&str; 3] = [
    "doc/product/demo/section_01.md",
    "doc/product/demo/section_02.md",
    "doc/product/demo/section_03.md",
];
const RECORD: &str = "\n## Pressure tests\n\n- 2026-10-26: Claude opus; 1 round\n";

/// A temporary repository root holding `files`.
fn tree(files: &[(&str, String)]) -> (TempDir, PathBuf) {
    let builder = tempfile::Builder::new().prefix("loom-checklist-").clone();
    let temp = builder.tempdir_in(std::env::temp_dir()).unwrap();
    let root = temp.path().to_path_buf();
    fs::create_dir_all(root.join("doc")).unwrap();
    for (rel, text) in files {
        put(&root, rel, text);
    }
    (temp, root)
}

fn put(root: &Path, rel: &str, content: impl AsRef<[u8]>) {
    fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    fs::write(root.join(rel), content).unwrap();
}

fn p(rel: &str) -> PathBuf {
    PathBuf::from(rel)
}

/// A vision whose `Specification sections` table lists `numbers`.
fn vision(numbers: &[u32]) -> String {
    let row = |n: &u32| format!("| {n:02} | S{n} |\n");
    let rows: String = numbers.iter().map(row).collect();
    format!("# Vision\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n{rows}")
}

/// `(path, text)` of product section `n`.
fn section(n: u32) -> (&'static str, String) {
    let text = format!("# Section {n:02}\n\nBody of section {n}.\n");
    (SECTIONS[n as usize - 1], text)
}

fn plan(name: &str, implements: &str) -> String {
    format!("# Plan: {name}\n\nImplements: {implements}\n")
}

/// A metaplan at `doc/plans/demo/index.md` with one numbered entry per slug.
fn metaplan(slugs: &[&str]) -> (&'static str, String) {
    let entry = |(i, slug): (usize, &&str)| format!("## {}. E (`PLAN-{slug}.md`)\n\n", i + 1);
    let entries: String = slugs.iter().enumerate().map(entry).collect();
    (META, format!("# Metaplan\n\n{entries}"))
}

/// A vision listing 01 to 03 with every section written, plus `more`.
fn spec(more: Vec<(&'static str, String)>) -> Vec<(&'static str, String)> {
    let mut files = vec![(VISION, vision(&[1, 2, 3]))];
    files.extend((1..=3).map(section));
    files.extend(more);
    files
}

fn checklist(root: &Path) -> Checklist {
    Project::load(root)
        .unwrap()
        .feature("demo")
        .unwrap()
        .checklist
        .clone()
}

fn primary(c: &Checklist) -> Option<String> {
    c.primary.as_ref().map(Offer::id)
}

fn secondary(c: &Checklist) -> Vec<String> {
    c.secondary.iter().map(Offer::id).collect()
}

fn offers(c: &Checklist) -> Vec<String> {
    c.primary
        .iter()
        .chain(&c.secondary)
        .map(Offer::id)
        .collect()
}

fn mark(c: &Checklist, step: Step) -> Option<StepMark> {
    let found = c.steps.iter().find(|(s, _)| *s == step);
    found.map(|(_, m)| m.clone())
}

/// `(path, line)` of every problem with `code`.
fn problems(project: &Project, code: &str) -> Vec<(PathBuf, Option<usize>)> {
    let found = project.problems.iter().filter(|x| x.code == code);
    found.map(|x| (x.path.clone(), x.line)).collect()
}

type Row = (Vec<(&'static str, String)>, CurrentStep, &'static str);

/// One fixture per row of section 01's offers table: files, current step, primary offer id.
fn state_rows() -> Vec<Row> {
    use CurrentStep::{Ready, ReadyAllDone, Step as At};
    let (a, b) = (plan("A", "section_01"), plan("B", "section_02"));
    let (a_tested, b_tested) = (format!("{a}{RECORD}"), format!("{b}{RECORD}"));
    let (pa, pb) = ("doc/plans/demo/PLAN-a.md", "doc/plans/demo/PLAN-b.md");
    let meta = || metaplan(&["a", "b"]);
    let product = vec![(VISION, vision(&[1, 2, 3])), section(2)];
    let half = spec(vec![meta(), (pa, a_tested.clone()), (pb, b.clone())]);
    let tested = spec(vec![meta(), (pa, a_tested), (pb, b_tested)]);
    let done = ("doc/plans/demo/archive/DONE-PLAN-a.md", a);
    let retired = ("doc/plans/demo/archive/RETIRED-PLAN-b.md", b.clone());
    let archived = spec(vec![meta(), done, retired]);
    // Discovery needs a named document to identify a feature without a vision.
    let no_vision = vec![("doc/product/demo/note.md", "# Note\n".into())];
    vec![
        (no_vision, At(Step::Vision), "start-vision"),
        (product, At(Step::ProductSpec), "write-section:01"),
        (spec(vec![]), At(Step::Metaplan), "start-metaplan"),
        (spec(vec![meta(), (pb, b)]), At(Step::Plans), "write-plan:a"),
        (half, At(Step::PressureTest), "pressure-test:b"),
        (tested, Ready, "start-run"),
        (archived, ReadyAllDone, "refine-spec"),
    ]
}

#[test]
fn checklist_state_rows() {
    for (i, (files, current, want)) in state_rows().into_iter().enumerate() {
        let (_temp, root) = tree(&files);
        let c = checklist(&root);
        assert_eq!(c.feature, "demo");
        assert_eq!(c.current, current, "row {}", i + 1);
        assert_eq!(primary(&c).as_deref(), Some(want), "row {}", i + 1);
        assert_eq!(mark(&c, Step::Review), Some(StepMark::Optional));
        if i == 4 {
            assert!(secondary(&c).contains(&"start-run".to_string()), "{c:?}");
        }
    }
}

#[test]
fn checklist_skipped_steps() {
    let x = ("doc/plans/demo/PLAN-x.md", plan("X", "none"));
    let (_temp, root) = tree(&[
        x,
        ("doc/plans/demo/IN_PROGRESS-PLAN-y.md", plan("Y", "none")),
    ]);
    let c = checklist(&root);
    assert_eq!(c.current, CurrentStep::Step(Step::Vision));
    assert_eq!(c.state.plans_unlisted, 2);
    let tests_offered = secondary(&c).contains(&"pressure-test:x".to_string());
    assert!(tests_offered, "{:?}", secondary(&c));
    assert_eq!(mark(&c, Step::Run), Some(StepMark::Active));

    let (_temp2, root) = tree(&[section(1)]);
    let c = checklist(&root);
    assert_eq!(c.current, CurrentStep::Step(Step::Vision));
    assert_eq!(primary(&c).as_deref(), Some("start-vision"));
    let section_01 = Some(Path::new("doc/product/demo/section_01.md"));
    let unlisted = c.items.iter().filter(|i| i.status == ItemStatus::Unlisted);
    let paths: Vec<_> = unlisted.map(|i| i.path.as_deref()).collect();
    assert!(paths.contains(&section_01), "{:?}", c.items);
    let item_offers = c.items.iter().filter_map(|item| item.offer.as_ref());
    let mut offers: Vec<String> = item_offers.map(Offer::id).collect();
    offers.extend(primary(&c).into_iter().chain(secondary(&c)));
    let writes = offers.iter().filter(|id| id.starts_with("write-section"));
    assert_eq!(writes.count(), 0, "{offers:?}");
}

#[test]
fn vision_table_never_guessed() {
    let index = "# Vision\n\n## 01 — Workflow\n\nThe workflow.\n\n## Sections\n\n\
                 - 01 Workflow\n- 02 Document model\n";
    let (_temp, root) = tree(&[(VISION, index.to_string()), section(1)]);
    let project = Project::load(&root).unwrap();
    let feature = project.feature("demo").unwrap();
    assert!(feature.vision_rows.is_err(), "{:?}", feature.vision_rows);
    let fix = primary(&feature.checklist);
    assert_eq!(fix.as_deref(), Some("fix-vision-table"));
    let paths = |code| -> Vec<PathBuf> {
        let found = problems(&project, code).into_iter();
        found.map(|(path, _)| path).collect()
    };
    assert_eq!(paths("vision-table-unreadable"), [p(VISION)]);
    assert_eq!(
        paths("unlisted-section"),
        [p("doc/product/demo/section_01.md")]
    );
}

#[test]
fn implements_token_forms() {
    let forms = "# P\n\nImplements: section_07, [section_08](../../product/demo/section_08.md), \
                 section_09.md\n";
    let link = parse(forms);
    assert_eq!(link.value, Implements::Sections(vec![7, 8, 9]));
    assert_eq!(link.line, Some(2));
    assert!(link.bad_tokens.is_empty(), "{:?}", link.bad_tokens);
    assert!(link.duplicate_lines.is_empty());
    let none = parse("# P\n\nImplements: none\n").value;
    assert_eq!(none, Implements::NoneDeclared);
    assert_eq!(parse("# P\n\nNo line here.\n").value, Implements::Unmapped);
    let fenced = parse("# P\n\n```text\nImplements: section_01\n```\n");
    assert_eq!((fenced.value, fenced.line), (Implements::Unmapped, None));
    let twice = parse("# P\n\nImplements: section_01\nImplements: section_02\n");
    assert_eq!(twice.value, Implements::Sections(vec![1]));
    assert_eq!((twice.line, twice.duplicate_lines), (Some(2), vec![3]));
    let bad = parse("# P\n\nImplements: section_7x, chapter_2\n");
    assert_eq!(bad.bad_tokens, ["section_7x", "chapter_2"]);
}

#[test]
fn metaplan_entry_any_spelling() {
    let round_2 = "doc/plans/demo/round-2-metaplan.md";
    let index = "# Metaplan\n\n## 1. Delivery truth (`PLAN-02-delivery-truth.md`)\n\n\
                 ## 2. Old (`DONE-PLAN-old.md`)\n\n## Policy notes (no plan file)\n\n\
                 ## 3. Missing name\n\n## 4. Policy notes (no plan file)\n";
    let next = "# Round 2\n\n## 1. Next (`PLAN-next.md`)\n\n\
                ## 2. Delivery again (`PLAN-02-delivery-truth.md`)\n";
    let old = (
        "doc/plans/demo/archive/DONE-PLAN-old.md",
        plan("Old", "section_01"),
    );
    let truth = (
        "doc/plans/demo/PLAN-02-delivery-truth.md",
        plan("D", "section_01"),
    );
    let (meta, round) = ((META, index.to_string()), (round_2, next.to_string()));
    let (_temp, root) = tree(&[meta, round, old, truth, (VISION, vision(&[1])), section(1)]);
    let project = Project::load(&root).unwrap();
    let feature = project.feature("demo").unwrap();
    let paths: Vec<&PathBuf> = feature.metaplans.iter().map(|(path, _)| path).collect();
    assert_eq!(paths, [&p(META), &p(round_2)]);
    let first = feature.metaplans[0].1.iter().map(|e| e.slug.as_deref());
    let slugs: Vec<Option<&str>> = first.collect();
    assert_eq!(slugs, [Some("02-delivery-truth"), Some("old"), None]);
    let second = &feature.metaplans[1];
    assert_eq!(second.1.len(), 2, "{second:?}");
    let slug = "next".to_string();
    let want = Offer::WritePlan {
        feature: "demo".into(),
        metaplan: p(round_2),
        slug,
    };
    assert_eq!(feature.checklist.primary, Some(want));
    let without = problems(&project, "metaplan-entry-without-plan");
    assert_eq!(without, [(p(META), Some(9))]);
}

#[test]
fn pressure_step_never_a_gap() {
    for b in [
        "doc/plans/demo/PLAN-b.md",
        "doc/plans/demo/IN_PROGRESS-PLAN-b.md",
    ] {
        let a = ("doc/plans/demo/PLAN-a.md", plan("A", "section_01"));
        let files = spec(vec![metaplan(&["a", "b"]), a, (b, plan("B", "section_02"))]);
        let (_temp, root) = tree(&files);
        let c = checklist(&root);
        let gaps = c.items.iter().filter(|i| i.status == ItemStatus::Gap);
        let gaps: Vec<_> = gaps.collect();
        assert!(gaps.is_empty(), "{b}: {gaps:?}");
        let gap_steps = c.steps.iter().filter(|(_, m)| *m == StepMark::Gap);
        assert_eq!(gap_steps.count(), 0, "{:?}", c.steps);
        assert_eq!(c.current, CurrentStep::Step(Step::PressureTest));
        assert_eq!(mark(&c, Step::PressureTest), Some(StepMark::Optional));
        if b.contains("IN_PROGRESS") {
            assert_eq!(mark(&c, Step::Run), Some(StepMark::Active));
        }
    }
}

#[test]
fn stray_copy_is_a_problem() {
    let s = ("doc/plans/demo/PLAN-s.md", plan("S", "none"));
    let (_temp, root) = tree(&[s, ("doc/plans/demo/DONE-PLAN-s.md", plan("S", "none"))]);
    let strays = problems(&Project::load(&root).unwrap(), "stray-plan-copy");
    let paths: Vec<PathBuf> = strays.into_iter().map(|(path, _)| path).collect();
    assert_eq!(paths, [p("doc/plans/demo/PLAN-s.md")]);
}

#[test]
fn empty_section_is_a_gap() {
    for empty in ["", "\n\n   \n\t\n"] {
        let blank = ("doc/product/demo/section_02.md", empty.to_string());
        let (_temp, root) = tree(&[(VISION, vision(&[1, 2])), section(1), blank]);
        let c = checklist(&root);
        assert_eq!(c.current, CurrentStep::Step(Step::ProductSpec), "{empty:?}");
        let write = primary(&c);
        assert_eq!(write.as_deref(), Some("write-section:02"), "{empty:?}");
    }
}

#[test]
fn vision_table_controls_section_items_and_order() {
    let (_temp, root) = tree(&[(VISION, vision(&[3, 1])), section(1), section(2)]);
    let c = checklist(&root);
    let sections = c.items.iter().filter(|i| i.step == Step::ProductSpec);
    let got: Vec<_> = sections
        .map(|i| (i.path.clone(), i.status.clone()))
        .collect();
    assert_eq!(
        got,
        [
            (Some(p(SECTIONS[2])), ItemStatus::Gap),
            (Some(p(SECTIONS[0])), ItemStatus::Present),
            (Some(p(SECTIONS[1])), ItemStatus::Unlisted),
        ]
    );
    assert_eq!(c.state.sections_listed, 2);
    assert_eq!(c.state.sections_written, [1, 2]);
    assert_eq!(
        offers(&c),
        [
            "write-section:03",
            "start-metaplan",
            "add-section-to-vision:02"
        ]
    );
}

#[test]
fn pressure_offers_follow_metaplan_order_without_duplicates() {
    let files = spec(vec![
        metaplan(&["b", "a", "b"]),
        ("doc/plans/demo/PLAN-a.md", plan("A", "section_01")),
        ("doc/plans/demo/PLAN-b.md", plan("B", "section_02")),
        ("doc/plans/demo/PLAN-c.md", plan("C", "none")),
    ]);
    let (_temp, root) = tree(&files);
    let c = checklist(&root);
    assert_eq!(c.current, CurrentStep::Step(Step::PressureTest));
    assert_eq!(
        offers(&c),
        [
            "pressure-test:b",
            "start-run",
            "pressure-test:a",
            "pressure-test:c"
        ]
    );
}
