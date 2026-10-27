//! Tests for the store.

use super::*;
use std::fs;
use tempfile::TempDir;

fn project() -> (TempDir, Project) {
    let dir = TempDir::new().unwrap();
    let files = [
        ("doc/product/index.md", "## Specification sections\n| # | Name |\n| - | - |\n| 2 | Second |\n| 1 | First |\n| 9 | Missing |\n"),
        ("doc/product/section_01.md", "# First\n"),
        ("doc/product/section_02.md", "# Second\n"),
        ("doc/product/section_03.md", "# Outside\n"),
        ("doc/plans/index.md", "## Named (`PLAN-archived.md`)\n## Running (`PLAN-running.md`)\n## Again (`PLAN-archived.md`)\n"),
        ("doc/plans/PLAN-written.md", "# Written\n"),
        ("doc/plans/IN_PROGRESS-PLAN-running.md", "# Running\n"),
        ("doc/plans/DONE-PLAN-done.md", "# Done\n"),
        ("doc/plans/RETIRED-PLAN-retired.md", "# Retired\n"),
        ("doc/plans/archive/DONE-PLAN-archived.md", "# Archived\n"),
        ("doc/plans/archive/DONE-PLAN-unnamed.md", "# Unnamed\n"),
        ("doc/plans/PLAN-done.md", "# Stray\n"),
        ("doc/plans/briefs/foo/note.md", "# Brief\n"),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let project = Project::load(dir.path()).unwrap();
    (dir, project)
}

#[test]
fn only_product_and_engineering_are_overviews() {
    assert!(is_overview("product") && is_overview("engineering"));
    for id in [
        "product/section_01",
        "plans/engineering",
        "plans/product",
        "",
    ] {
        assert!(!is_overview(id), "{id}");
    }
}

#[test]
fn document_ids_cover_store_rules() {
    let (_dir, project) = project();
    let cases = [
        ("doc/product/section_01.md", Some("product/section_01")),
        ("doc/product/section_03.md", Some("product/section_03")),
        ("doc/plans/PLAN-written.md", Some("plans/written")),
        (
            "doc/plans/IN_PROGRESS-PLAN-running.md",
            Some("plans/running"),
        ),
        ("doc/plans/DONE-PLAN-done.md", Some("plans/done")),
        ("doc/plans/RETIRED-PLAN-retired.md", Some("plans/retired")),
        (
            "doc/plans/archive/DONE-PLAN-archived.md",
            Some("plans/archived"),
        ),
        ("doc/plans/archive/DONE-PLAN-unnamed.md", None),
        ("doc/plans/PLAN-done.md", None),
        ("doc/plans/index.md", None),
        ("doc/product/index.md", None),
        ("doc/plans/briefs/foo/note.md", None),
    ];
    for (path, expected) in cases {
        assert_eq!(
            summary_id_for(&project, Path::new(path)).as_deref(),
            expected,
            "{path}"
        );
        if let Some(id) = expected {
            assert_eq!(source_of(&project, id), Some(path.into()));
        }
    }
    let ids = expected_ids(&project);
    assert!(ids.contains(&"product".into()));
    assert!(ids.contains(&"engineering".into()));
    assert!(!ids.contains(&"plans/unnamed".into()));
}

#[test]
fn overview_scope_follows_table_and_metaplan_order() {
    let (_dir, project) = project();
    let scope = scope(&project);
    assert_eq!(scope.product, ["product/section_02", "product/section_01"]);
    assert_eq!(scope.engineering, ["plans/archived", "plans/running"]);
    assert_eq!(
        scope.outside,
        [
            "plans/done",
            "plans/retired",
            "plans/written",
            "product/section_03"
        ]
    );
}

#[test]
fn validates_ids_before_paths() {
    for id in [
        "product",
        "engineering",
        "product/section_01",
        "product/section_123",
        "plans/A.b_c-1",
    ] {
        assert!(validate_id(id).is_ok(), "{id}");
    }
    for id in [
        "",
        "../x",
        "plans/../../etc",
        "plans/",
        "plans/a/b",
        "/product",
        "product/section_1",
        "product/section_1x",
        "plans/.hidden",
        "plans/a..b",
        "plans/a\\b",
        "plans/é",
    ] {
        assert!(validate_id(id).unwrap_err().to_string().contains(id));
        assert!(summary_path(id).as_os_str().is_empty());
    }
    assert_eq!(
        summary_path("plans/a"),
        PathBuf::from("doc/loom/summaries/plans/a.md")
    );
}

#[test]
fn relative_links_round_trip_and_cannot_escape() {
    let cases = [
        (
            "product/section_01",
            "../../../product/section_01.md#a",
            "doc/product/section_01.md",
            Some("a"),
        ),
        (
            "product",
            "product/section_01.md#s-1234abcd",
            "doc/loom/summaries/product/section_01.md",
            Some("s-1234abcd"),
        ),
    ];
    for (id, href, target, fragment) in cases {
        assert_eq!(
            resolve_link(id, href),
            Some((target.into(), fragment.map(str::to_owned)))
        );
        assert_eq!(
            link_from_summary(id, Path::new(target)),
            href.split('#').next().unwrap()
        );
    }
    assert_eq!(resolve_link("product", "../../../../etc/passwd"), None);
    assert_eq!(resolve_link("../bad", "x"), None);
    assert_eq!(resolve_link("product", "/etc/passwd"), None);
}
