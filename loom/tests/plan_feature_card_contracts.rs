//! Contracts for the feature card payload: slug, pitch and step counts on `ChecklistView`.

use loom::plan_server::wire::checklist::{ChecklistView, ProjectChecklistView};
use loom::planning::project::Project;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// A temporary repository root holding `files`.
fn tree(files: &[(&str, &str)]) -> TempDir {
    let temp = tempfile::Builder::new()
        .prefix("loom-card-")
        .tempdir_in(std::env::temp_dir())
        .unwrap();
    fs::create_dir_all(temp.path().join("doc")).unwrap();
    for (rel, text) in files {
        let path = temp.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    temp
}

/// The converted checklist of `feature` in the tree at `root`.
fn card(root: &Path, feature: &str) -> ChecklistView {
    let project = Project::load(root).unwrap();
    let view = ProjectChecklistView::from(&project);
    let names: Vec<_> = view.checklists.iter().map(|c| c.feature.clone()).collect();
    let found = view.checklists.into_iter().find(|c| c.feature == feature);
    found.unwrap_or_else(|| panic!("no checklist for {feature}; features: {names:?}"))
}

#[test]
fn pitch_skips_status_paragraph() {
    let vision = "# Demo\n\nStatus: agreed vision (2026-10-26).\n\nA browser app for plans.\n";
    let temp = tree(&[("doc/product/demo/index.md", vision)]);
    let view = card(temp.path(), "demo");
    assert!(view.has_product);
    assert_eq!(view.pitch.as_deref(), Some("A browser app for plans."));
}

#[test]
fn pitch_falls_back_to_metaplan() {
    let meta = "# Token Optimization Metaplan\n\nThis metaplan orders six plans.\n";
    let temp = tree(&[("doc/plans/tokens/index.md", meta)]);
    assert!(!temp.path().join("doc/product").exists());
    let view = card(temp.path(), "tokens");
    assert!(!view.has_product);
    assert_eq!(
        view.pitch.as_deref(),
        Some("This metaplan orders six plans.")
    );
}

#[test]
fn step_counts_follow_documents() {
    let vision = "# Demo\n\nA demo feature.\n\n## Specification sections\n\n\
                  | # | Section |\n| --- | --- |\n\
                  | 01 | Alpha |\n| 02 | Beta |\n| 03 | Gamma |\n";
    let temp = tree(&[
        ("doc/product/demo/index.md", vision),
        ("doc/product/demo/section_01.md", "# Alpha\n\nBody.\n"),
        ("doc/product/demo/section_02.md", "# Beta\n\nBody.\n"),
    ]);
    let view = card(temp.path(), "demo");
    assert_eq!(view.feature, "demo");
    assert!(serde_json::to_value(&view).unwrap().get("title").is_none());
    let steps: Vec<_> = view.steps.iter().map(|s| s.number).collect();
    assert_eq!(steps, [1, 2, 3, 4, 5, 6]);
    assert_eq!(view.steps[5].name, "Run");
    assert_eq!(view.primary.as_ref().unwrap().label, "Write section 03");
    let step = view.steps.iter().find(|s| s.number == 2).unwrap();
    assert_eq!(step.count.as_deref(), Some("2/3"));
    assert_eq!(step.label, "Product spec");
    let numbers: Vec<_> = view.sections.iter().map(|s| s.number.as_str()).collect();
    assert_eq!(numbers, ["01", "02", "03"]);
    let written: Vec<_> = view.sections.iter().map(|s| s.written).collect();
    assert_eq!(written, [true, true, false]);
}
