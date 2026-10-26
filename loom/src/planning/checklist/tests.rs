//! Checklist, offer, problem and refresh rules beyond the frozen contracts.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::TempDir;

use super::{CurrentStep, ItemStatus, Offer, Step, StepMark};
use crate::planning::project::Project;
use crate::planning::watch::{OwnWrites, Watcher};

const VISION: &str = "doc/product/index.md";
const META: &str = "doc/plans/index.md";
const RECORD: &str = "\n## Pressure tests\n\n- 2026-10-26: one round\n";

fn put(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn tree(files: &[(&str, String)]) -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("doc")).unwrap();
    for (rel, text) in files {
        put(temp.path(), rel, text);
    }
    temp
}

fn vision(numbers: &[u32]) -> (&'static str, String) {
    let rows: String = numbers
        .iter()
        .map(|n| format!("| {n:02} | S{n} |\n"))
        .collect();
    let head = "# Vision\n\n## Specification sections\n\n| # | Name |\n| --- | --- |\n";
    let text = format!("{head}{rows}");
    (VISION, text)
}

fn section(n: u32) -> (String, String) {
    (
        format!("doc/product/section_{n:02}.md"),
        format!("# Section {n:02}\n\nBody.\n"),
    )
}

fn plan(rel: &str, tested: bool) -> (String, String) {
    let record = if tested { RECORD } else { "" };
    (
        rel.to_string(),
        format!("# Plan\n\nImplements: none\n{record}"),
    )
}

fn metaplan(slugs: &[&str]) -> (&'static str, String) {
    let entry = |(i, slug): (usize, &&str)| format!("## {}. E (`PLAN-{slug}.md`)\n\n", i + 1);
    (
        META,
        format!(
            "# Metaplan\n\n{}",
            slugs.iter().enumerate().map(entry).collect::<String>()
        ),
    )
}

/// Vision listing 01, section 01 written, plus `more`.
fn spec(more: Vec<(String, String)>) -> Vec<(String, String)> {
    let (rel, text) = vision(&[1]);
    let mut files = vec![(rel.to_string(), text), section(1)];
    files.extend(more);
    files
}

fn load(files: &[(String, String)]) -> (TempDir, Project) {
    let pairs: Vec<(&str, String)> = files.iter().map(|(r, t)| (r.as_str(), t.clone())).collect();
    let temp = tree(&pairs);
    let project = Project::load(temp.path()).unwrap();
    (temp, project)
}

fn ids(offers: &[Offer]) -> Vec<String> {
    offers.iter().map(Offer::id).collect()
}

fn owned(pair: (&'static str, String)) -> (String, String) {
    (pair.0.to_string(), pair.1)
}

#[test]
fn offer_ids_targets_and_json() {
    let write = Offer::WritePlan {
        metaplan: PathBuf::from(META),
        slug: "a".into(),
    };
    assert_eq!(write.id(), "write-plan:a");
    assert_eq!(write.target(), Some(PathBuf::from("doc/plans/PLAN-a.md")));
    let section = Offer::AddSectionToVision { number: 5 };
    assert_eq!(section.id(), "add-section-to-vision:05");
    assert_eq!(
        section.target(),
        Some(PathBuf::from("doc/product/section_05.md"))
    );
    assert_eq!(Offer::FixVisionTable.target(), Some(PathBuf::from(VISION)));
    assert_eq!(Offer::StartVision.target(), None);
    assert_eq!(Offer::NewMetaplanRound.id(), "new-metaplan-round");
    let value = serde_json::to_value(&write).unwrap();
    let want = json!({"id": "write-plan:a", "kind": "write-plan", "metaplan": META,
        "slug": "a", "target": "doc/plans/PLAN-a.md"});
    assert_eq!(value, want);
    let start = serde_json::to_value(Offer::StartRun).unwrap();
    assert_eq!(
        start,
        json!({"id": "start-run", "kind": "start-run", "target": null})
    );
}

#[test]
fn checklist_json_shape() {
    let files = [
        owned(metaplan(&["a"])),
        plan("doc/plans/IN_PROGRESS-PLAN-a.md", false),
    ];
    let (_temp, project) = load(&spec(files.to_vec()));
    let value = serde_json::to_value(&project.checklist).unwrap();
    assert_eq!(value["current"], json!("ready"));
    assert_eq!(value["steps"][6], json!({"step": "run", "mark": "active"}));
    assert_eq!(value["steps"].as_array().map(Vec::len), Some(7));
    assert_eq!(value["state"]["plans_by_state"], json!({"running": 1}));
    let locked = json!(["doc/plans/IN_PROGRESS-PLAN-a.md"]);
    assert_eq!(value["state"]["locked_files"], locked);
    let step = serde_json::to_value(CurrentStep::Step(Step::ProductSpec)).unwrap();
    assert_eq!(step, json!("product_spec"));
}

#[test]
fn ready_rows_secondaries() {
    let tested = spec(vec![
        owned(metaplan(&["a"])),
        plan("doc/plans/PLAN-a.md", true),
    ]);
    let (_temp, project) = load(&tested);
    let c = &project.checklist;
    assert_eq!(c.current, CurrentStep::Ready);
    assert_eq!(ids(&c.secondary), ["refine-spec"]);
    let done = plan("doc/plans/archive/DONE-PLAN-a.md", true);
    let (_temp, project) = load(&spec(vec![owned(metaplan(&["a"])), done]));
    let c = &project.checklist;
    assert_eq!(c.current, CurrentStep::ReadyAllDone);
    assert_eq!(ids(&c.secondary), ["new-metaplan-round"]);
    assert_eq!(c.steps[6], (Step::Run, StepMark::Done));
}

#[test]
fn run_is_optional_until_every_entry_is_finished() {
    let files = vec![
        owned(metaplan(&["a", "b"])),
        plan("doc/plans/archive/DONE-PLAN-a.md", true),
        plan("doc/plans/PLAN-b.md", true),
    ];
    let (_temp, project) = load(&spec(files));
    assert_eq!(project.checklist.steps[6], (Step::Run, StepMark::Optional));
    assert_eq!(project.checklist.current, CurrentStep::Ready);
}

#[test]
fn unreadable_table_offers_fix_and_unlisted_sections() {
    let vision = (
        VISION.to_string(),
        "# Vision\n\n## Specification sections\n\nNone.\n".into(),
    );
    let (_temp, project) = load(&[vision, section(2)]);
    let c = &project.checklist;
    assert_eq!(c.current, CurrentStep::Step(Step::ProductSpec));
    assert_eq!(c.primary, Some(Offer::FixVisionTable));
    assert_eq!(
        ids(&c.secondary),
        ["start-metaplan", "add-section-to-vision:02"]
    );
    let gap = c
        .items
        .iter()
        .find(|i| i.step == Step::ProductSpec && i.status == ItemStatus::Gap);
    assert_eq!(
        gap.and_then(|i| i.offer.clone()),
        Some(Offer::FixVisionTable)
    );
    let line = project
        .problems
        .iter()
        .find(|p| p.code == "vision-table-unreadable");
    assert_eq!(line.map(|p| p.line), Some(Some(3)));
}

#[test]
fn table_order_decides_the_first_missing_section() {
    let (rel, text) = vision(&[3, 1]);
    let (_temp, project) = load(&[(rel.to_string(), text)]);
    let c = &project.checklist;
    assert_eq!(c.primary, Some(Offer::WriteSection { number: 3 }));
    assert_eq!(ids(&c.secondary), ["write-section:01", "start-metaplan"]);
}

#[test]
fn add_plan_to_metaplan_only_with_a_metaplan() {
    let z = plan("doc/plans/PLAN-z.md", true);
    let with = spec(vec![
        owned(metaplan(&["a"])),
        plan("doc/plans/PLAN-a.md", true),
        z.clone(),
    ]);
    let (_temp, project) = load(&with);
    assert!(ids(&project.checklist.secondary).contains(&"add-plan-to-metaplan:z".into()));
    let (_temp, project) = load(&spec(vec![z]));
    let c = &project.checklist;
    let mut all = ids(&c.secondary);
    all.extend(c.primary.iter().map(Offer::id));
    assert!(
        !all.iter()
            .any(|id| id.starts_with("add-plan") || id == "start-run"),
        "{all:?}"
    );
    let unlisted = c.items.iter().find(|i| i.status == ItemStatus::Unlisted);
    assert_eq!(unlisted.map(|i| i.offer.is_none()), Some(true));
}

#[test]
fn untested_plans_follow_metaplan_then_file_name_order() {
    let mut files = vec![owned(metaplan(&["m"])), plan("doc/plans/PLAN-m.md", false)];
    files.extend([
        plan("doc/plans/PLAN-b.md", false),
        plan("doc/plans/PLAN-a.md", false),
    ]);
    let (_temp, project) = load(&spec(files));
    let c = &project.checklist;
    assert_eq!(c.primary, Some(Offer::PressureTest { slug: "m".into() }));
    let want = ["start-run", "pressure-test:a", "pressure-test:b"];
    assert_eq!(ids(&c.secondary)[..3], want);
    let untested = c
        .items
        .iter()
        .filter(|i| i.status == ItemStatus::Untested)
        .count();
    assert_eq!(untested, 3);
}

#[test]
fn locked_documents_report_no_problems() {
    let running = (
        "doc/plans/IN_PROGRESS-PLAN-r.md".to_string(),
        "No title.\n".to_string(),
    );
    let stray = (
        "doc/plans/PLAN-r.md".to_string(),
        "# Plan\n\nImplements: none\n".into(),
    );
    let (_temp, project) = load(&[running, stray]);
    let codes: Vec<(&str, &Path)> = project
        .problems
        .iter()
        .map(|p| (p.code, p.path.as_path()))
        .collect();
    assert_eq!(
        codes,
        [("stray-plan-copy", Path::new("doc/plans/PLAN-r.md"))]
    );
}

#[test]
fn title_is_first_h1_else_file_name() {
    let note = (
        "doc/product/note.md".to_string(),
        "## Only level two\n".to_string(),
    );
    let plan = (
        "doc/plans/PLAN-p.md".into(),
        "Intro.\n\n# Plan P\n\nImplements: none\n".into(),
    );
    let (_temp, project) = load(&[note, plan]);
    assert_eq!(
        project.docs[Path::new("doc/product/note.md")].title,
        "note.md"
    );
    assert_eq!(
        project.docs[Path::new("doc/plans/PLAN-p.md")].title,
        "Plan P"
    );
    let missing: Vec<_> = project
        .problems
        .iter()
        .filter(|p| p.code == "missing-title")
        .collect();
    assert_eq!(missing.len(), 1, "{missing:?}");
    let want = (Path::new("doc/plans/PLAN-p.md"), Some(1));
    assert_eq!((missing[0].path.as_path(), missing[0].line), want);
}

/// Applies `change` under the root, delivers the watcher's batch and compares with a load.
fn refresh_after(change: impl Fn(&Path)) {
    let files = spec(vec![
        section(2),
        owned(metaplan(&["a"])),
        plan("doc/plans/PLAN-a.md", false),
    ]);
    let pairs: Vec<(&str, String)> = files.iter().map(|(r, t)| (r.as_str(), t.clone())).collect();
    let temp = tree(&pairs);
    let root = temp.path();
    let mut project = Project::load(root).unwrap();
    let mut watcher = Watcher::new(root.to_path_buf(), OwnWrites::default());
    let initial = watcher.poll().unwrap().expect("initial batch");
    project.refresh(&initial).unwrap();
    change(root);
    let batch = (0..4)
        .find_map(|_| watcher.poll().unwrap())
        .expect("a batch");
    project.refresh(&batch).unwrap();
    assert_eq!(project, Project::load(root).unwrap());
}

#[test]
fn refresh_matches_load_per_change() {
    refresh_after(|root| {
        put(
            root,
            "doc/product/section_01.md",
            "# Section 01\n\n## New\n",
        )
    });
    refresh_after(|root| {
        let plans = root.join("doc/plans");
        fs::rename(plans.join("PLAN-a.md"), plans.join("IN_PROGRESS-PLAN-a.md")).unwrap();
    });
    refresh_after(|root| fs::remove_file(root.join("doc/product/section_01.md")).unwrap());
    refresh_after(|root| {
        let product = root.join("doc/product");
        fs::rename(product.join("section_02.md"), product.join("section_07.md")).unwrap();
    });
    refresh_after(|root| {
        let link = "# Section 02\n\nSee [one](section_01.md#section-01).\n";
        put(root, "doc/product/section_02.md", link);
        put(root, "doc/product/section_01.md", "# Renamed\n");
    });
}
