//! Tests for `loom plan check`.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use super::{human, report};
use crate::planning::checklist::Offer;
use crate::planning::project::Project;

fn put(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn project(root: &Path) -> Project {
    put(root, "doc/plans/beta/PLAN-b.md", "# Plan B\n");
    put(root, "doc/plans/IN_PROGRESS-PLAN-solo.md", "# Solo\n");
    put(
        root,
        "doc/plans/alpha/PLAN-a.md",
        "# Plan A\n\nImplements: none\n",
    );
    Project::load(root).unwrap()
}

fn keys(value: &Value) -> Vec<&str> {
    value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

#[test]
fn report_serializes_the_contract_keys() {
    let temp = tempfile::tempdir().unwrap();
    let project = project(temp.path());
    let value = serde_json::to_value(report(&project)).unwrap();

    assert_eq!(
        keys(&value),
        [
            "features",
            "moves",
            "problems",
            "root",
            "schema",
            "standalone"
        ]
    );
    assert_eq!(value["schema"], 2);
    assert_eq!(value["root"], json!(temp.path()));
    assert_eq!(keys(&value["features"][0]), ["checklist", "name"]);
    assert_eq!(value["features"][0]["name"], "alpha");
    assert_eq!(value["features"][1]["name"], "beta");
    for feature in value["features"].as_array().unwrap() {
        let checklist = &feature["checklist"];
        assert_eq!(
            keys(checklist),
            [
                "current",
                "feature",
                "items",
                "primary",
                "secondary",
                "state",
                "steps"
            ]
        );
        assert_eq!(checklist["feature"], feature["name"]);
        assert_eq!(checklist["steps"].as_array().unwrap().len(), 7);
    }
    assert_eq!(
        value["standalone"],
        json!([{
            "slug": "solo", "path": "doc/plans/IN_PROGRESS-PLAN-solo.md", "state": "running"
        }])
    );
    assert_eq!(value["moves"], json!([]));
}

#[test]
fn report_keeps_an_empty_project_free_of_features() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("doc")).unwrap();
    let project = Project::load(temp.path()).unwrap();
    let value = serde_json::to_value(report(&project)).unwrap();

    for field in ["features", "standalone", "moves", "problems"] {
        assert_eq!(value[field], json!([]), "{field}");
    }
    assert_eq!(human(&project), "standalone plans\n");
}

#[test]
fn human_output_neutralizes_escape_in_section_title() {
    let temp = tempfile::tempdir().unwrap();
    put(
        temp.path(),
        "doc/product/alpha/index.md",
        concat!(
            "# Vision\n\n## Specification sections\n\n",
            "| Number | Name |\n| --- | --- |\n| 1 | Hello\x1bworld |\n"
        ),
    );
    let project = Project::load(temp.path()).unwrap();

    let output = human(&project);

    assert!(output.contains("Hello"), "{output}");
    assert!(!output.contains('\x1b'), "{output:?}");
}

#[test]
fn human_output_groups_checklists_before_standalone_and_problems() {
    let temp = tempfile::tempdir().unwrap();
    let project = project(temp.path());

    let output = human(&project);
    let (alpha, rest) = output.split_once("beta\n").unwrap();
    let (beta, rest) = rest.split_once("standalone plans\n").unwrap();

    assert!(alpha.starts_with("alpha\n1 Vision: Gap\n"), "{output}");
    assert!(beta.starts_with("1 Vision: Gap\n"), "{output}");
    for group in [alpha, beta] {
        for want in [
            "7 Run: Optional",
            "Current: Vision",
            "Primary: start-vision",
            "Secondary: start-metaplan",
        ] {
            assert!(
                group.lines().any(|line| line == want),
                "missing {want}: {group}"
            );
        }
        assert!(!group.contains("solo"), "{group}");
    }
    assert!(alpha.contains("Secondary: pressure-test:a\n"), "{output}");
    assert!(!alpha.contains("pressure-test:b"), "{output}");
    assert!(beta.contains("Secondary: pressure-test:b\n"), "{output}");
    assert!(!beta.contains("pressure-test:a"), "{output}");
    assert_eq!(
        rest,
        concat!(
            "  solo: Running\n",
            "doc/plans/beta/PLAN-b.md: implements-missing: plan has no Implements: line\n"
        )
    );
}

#[test]
fn human_output_neutralizes_feature_names_slugs_offers_and_problems() {
    let temp = tempfile::tempdir().unwrap();
    let mut project = project(temp.path());
    project.features[0].name = "alpha\x1b\nheader".into();
    project.features[0]
        .checklist
        .secondary
        .push(Offer::PressureTest {
            feature: "alpha".into(),
            slug: "offer\x1b\nslug".into(),
        });
    project.tree.plans.get_mut("solo").unwrap().slug = "solo\x1b\nslug".into();
    project.problems[0].path = "doc/hostile\x1b\npath".into();
    project.problems[0].message = "bad\x1b\nmessage".into();

    let output = human(&project);

    assert!(!output.contains('\x1b'), "{output:?}");
    assert!(!output
        .lines()
        .any(|line| matches!(line, "header" | "slug" | "path" | "message")));
    assert!(output.contains("alpha"), "{output}");
    assert!(output.contains("solo"), "{output}");
}
