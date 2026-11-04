//! Tests for feature-scoped summary ids and overview membership.

use super::*;
use std::fs;
use tempfile::TempDir;

fn write_files(dir: &TempDir, files: &[(&str, &str)]) {
    for (path, text) in files {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

fn project() -> (TempDir, Project) {
    let dir = TempDir::new().unwrap();
    write_files(&dir, &[
        ("doc/product/alpha/index.md", "## Specification sections\n| # | Name |\n| - | - |\n| 2 | Second |\n| 1 | First |\n| 9 | Missing |\n| 2 | Again |\n"),
        ("doc/product/alpha/section_01.md", "# First\n"),
        ("doc/product/alpha/section_02.md", "# Second\n"),
        ("doc/product/alpha/section_03.md", "# Outside\n"),
        ("doc/product/beta/index.md", "## Specification sections\n| # | Name |\n| - | - |\n| 1 | Beta first |\n"),
        ("doc/product/beta/section_01.md", "# Beta first\n"),
        ("doc/product/beta/section_02.md", "# Beta outside\n"),
        ("doc/plans/alpha/index.md", "## Named (`PLAN-archived.md`)\n## Running (`PLAN-running.md`)\n## Again (`PLAN-archived.md`)\n"),
        ("doc/plans/alpha/metaplan-02.md", "## Again (`PLAN-running.md`)\n"),
        ("doc/plans/alpha/IN_PROGRESS-PLAN-running.md", "# Running\n"),
        ("doc/plans/alpha/PLAN-running.md", "# Stray\n"),
        ("doc/plans/alpha/archive/DONE-PLAN-archived.md", "# Archived\n"),
        ("doc/plans/alpha/archive/DONE-PLAN-foreign-only.md", "# Named only by beta\n"),
        ("doc/plans/alpha/PLAN-unlisted.md", "# Unlisted\n"),
        ("doc/plans/alpha/archive/DONE-PLAN-unnamed.md", "# Unnamed\n"),
        ("doc/plans/beta/index.md", "## Done (`PLAN-done.md`)\n## Foreign (`PLAN-foreign-only.md`)\n## Foreign archived (`PLAN-archived.md`)\n## Standalone (`PLAN-solo.md`)\n"),
        ("doc/plans/beta/DONE-PLAN-done.md", "# Done\n"),
        ("doc/plans/beta/PLAN-done.md", "# Stray\n"),
        ("doc/plans/PLAN-solo.md", "# Standalone\n"),
        ("doc/plans/briefs/solo/note.md", "# Brief\n"),
        ("doc/product/index.md", "# Misplaced vision\n"),
        ("doc/product/section_01.md", "# Misplaced section\n"),
        ("doc/plans/index.md", "## Unnamed (`PLAN-unnamed.md`)\n"),
    ]);
    let project = Project::load(dir.path()).unwrap();
    (dir, project)
}

#[test]
fn only_valid_feature_overview_ids_are_overviews() {
    for id in ["overview/alpha/product", "overview/beta/engineering"] {
        assert!(is_overview(id), "{id}");
    }
    for id in [
        "product",
        "engineering",
        "alpha/product/section_01",
        "plans/product",
        "",
        "overview/Bad/product",
        "overview/alpha/other",
        "overview/alpha/product/extra",
        "overview/../product",
        "overview/plans/product",
        "overview/overview/engineering",
    ] {
        assert!(!is_overview(id), "{id}");
    }
}

type IdCase = (&'static str, Option<&'static str>);

fn document_id_cases() -> [IdCase; 19] {
    [
        (
            "doc/product/alpha/section_01.md",
            Some("alpha/product/section_01"),
        ),
        (
            "doc/product/alpha/section_03.md",
            Some("alpha/product/section_03"),
        ),
        (
            "doc/product/beta/section_01.md",
            Some("beta/product/section_01"),
        ),
        ("doc/plans/PLAN-solo.md", Some("plans/solo")),
        (
            "doc/plans/alpha/IN_PROGRESS-PLAN-running.md",
            Some("plans/running"),
        ),
        (
            "doc/plans/alpha/archive/DONE-PLAN-archived.md",
            Some("plans/archived"),
        ),
        ("doc/plans/beta/DONE-PLAN-done.md", Some("plans/done")),
        ("doc/plans/alpha/PLAN-unlisted.md", Some("plans/unlisted")),
        ("doc/plans/alpha/PLAN-running.md", None),
        ("doc/plans/beta/PLAN-done.md", None),
        ("doc/plans/alpha/archive/DONE-PLAN-foreign-only.md", None),
        ("doc/plans/alpha/archive/DONE-PLAN-unnamed.md", None),
        ("doc/plans/beta/index.md", None),
        ("doc/product/alpha/index.md", Some("alpha/product/vision")),
        ("doc/product/beta/index.md", Some("beta/product/vision")),
        ("doc/plans/briefs/solo/note.md", None),
        ("doc/product/index.md", None),
        ("doc/product/section_01.md", None),
        ("doc/plans/index.md", None),
    ]
}

#[test]
fn vision_feature_names_only_a_valid_features_vision_id() {
    assert_eq!(vision_feature("alpha/product/vision"), Some("alpha"));
    for id in [
        "",
        "product/vision",
        "Bad/product/vision",
        "alpha/product/vision/extra",
        "alpha/engineering/vision",
        "overview/alpha/product",
    ] {
        assert_eq!(vision_feature(id), None, "{id}");
        assert!(!is_vision(id), "{id}");
    }
    assert!(is_vision("beta/product/vision"));
}

#[test]
fn document_ids_use_the_winning_plans_own_feature() {
    let (_dir, project) = project();
    for (path, expected) in document_id_cases() {
        assert_eq!(
            summary_id_for(&project, Path::new(path)).as_deref(),
            expected,
            "{path}"
        );
        if let Some(id) = expected {
            assert_eq!(source_of(&project, id), Some(path.into()));
        }
    }
    assert_eq!(source_of(&project, "overview/alpha/product"), None);
    assert_eq!(source_of(&project, "product"), None);
}

#[test]
fn expected_ids_cover_two_features_and_the_standalone_shelf() {
    let (_dir, project) = project();
    let mut ids = expected_ids(&project);
    ids.sort();
    assert_eq!(
        ids,
        [
            "alpha/product/section_01",
            "alpha/product/section_02",
            "alpha/product/section_03",
            "alpha/product/vision",
            "beta/product/section_01",
            "beta/product/section_02",
            "beta/product/vision",
            "overview/alpha/engineering",
            "overview/alpha/product",
            "overview/beta/engineering",
            "overview/beta/product",
            "plans/archived",
            "plans/done",
            "plans/running",
            "plans/solo",
            "plans/unlisted",
        ]
    );
}

#[test]
fn overview_scope_follows_each_features_source_order() {
    let (_dir, project) = project();
    let alpha = Scope::of(&project, "overview/alpha/product").unwrap();
    assert_eq!(
        alpha,
        Scope {
            feature: "alpha".into(),
            product: vec![
                "alpha/product/section_02".into(),
                "alpha/product/section_01".into()
            ],
            engineering: vec!["plans/archived".into(), "plans/running".into()],
            outside: vec!["alpha/product/section_03".into(), "plans/unlisted".into()],
        }
    );
    assert_eq!(
        Scope::of(&project, "overview/alpha/engineering"),
        Some(alpha)
    );
    assert_eq!(
        scope(&project, "beta"),
        Scope {
            feature: "beta".into(),
            product: vec!["beta/product/section_01".into()],
            engineering: vec!["plans/done".into()],
            outside: vec!["beta/product/section_02".into()],
        }
    );
    for id in [
        "product",
        "overview/Bad/product",
        "overview/missing/product",
        "plans/solo",
    ] {
        assert_eq!(Scope::of(&project, id), None, "{id}");
    }
}

#[test]
fn overview_sources_may_be_empty_or_present_on_only_one_side() {
    let dir = TempDir::new().unwrap();
    write_files(
        &dir,
        &[
            (
                "doc/product/spec/index.md",
                "# Vision without a section table\n",
            ),
            ("doc/product/spec/section_01.md", "# Unlisted\n"),
            ("doc/plans/eng/index.md", "# Empty metaplan\n"),
            (
                "doc/product/lone/section_01.md",
                "# Section without a vision\n",
            ),
        ],
    );
    let project = Project::load(dir.path()).unwrap();
    let mut ids = expected_ids(&project);
    ids.sort();
    assert_eq!(
        ids,
        [
            "lone/product/section_01",
            "overview/eng/engineering",
            "overview/spec/product",
            "spec/product/section_01",
            "spec/product/vision",
        ]
    );
    assert_eq!(scope(&project, "spec").outside, ["spec/product/section_01"]);
    assert!(Scope::of(&project, "overview/eng/product")
        .unwrap()
        .product
        .is_empty());
    assert_eq!(
        scope(&project, "unknown"),
        Scope {
            feature: "unknown".into(),
            ..Scope::default()
        }
    );
}

fn assert_valid_summary_paths() {
    for id in [
        "overview/a/product",
        "overview/feature-2/engineering",
        "a/product/section_01",
        "a/product/section_123",
        "a/product/vision",
        "0/product/section_00",
        "plans/A.b_c-1",
        "plans/product",
        "overview/product/engineering",
        "engineering/product/section_01",
    ] {
        assert!(validate_id(id).is_ok(), "{id}");
        assert_eq!(
            summary_path(id),
            Path::new(STORE_DIR).join(format!("{id}.md"))
        );
    }
}

const INVALID_IDS: &[&str] = &[
    "",
    "product",
    "engineering",
    "product/section_01",
    "../x",
    "plans/../../etc",
    "plans/",
    "plans/a/b",
    "plans/product/section_01",
    "/overview/a/product",
    "overview/Bad/product",
    "overview/a/other",
    "overview/a/product/extra",
    "overview//product",
    "overview/../product",
    "overview/plans/product",
    "overview/overview/product",
    "overview/archive/product",
    "overview/briefs/product",
    "a/product/section_1",
    "a/product/section_1x",
    "a/product/vision/extra",
    "a/product/Vision",
    "a/engineering/vision",
    "product/vision",
    "Bad/product/vision",
    "overview/product/vision",
    "a/product/section_01/../../x",
    "../product/section_01",
    "a/../product/section_01",
    "a_b/product/section_01",
    "overview/product/section_01",
    "Bad/product/section_01",
    "a/product/section_０１",
    "plans/.hidden",
    "plans/a..b",
    "plans/a\\b",
    "plans/é",
];

#[test]
fn validates_ids_before_paths() {
    assert_valid_summary_paths();
    for &id in INVALID_IDS {
        assert!(validate_id(id).unwrap_err().to_string().contains(id));
        assert!(summary_path(id).as_os_str().is_empty());
    }
    assert_eq!(
        summary_path("overview/a/product"),
        PathBuf::from("doc/loom/summaries/overview/a/product.md")
    );
    assert_eq!(
        summary_path("a/product/section_01"),
        PathBuf::from("doc/loom/summaries/a/product/section_01.md")
    );
}

#[test]
fn relative_links_round_trip_and_cannot_escape() {
    let cases = [
        (
            "alpha/product/section_01",
            "../../../../product/alpha/section_01.md#a",
            "doc/product/alpha/section_01.md",
            Some("a"),
        ),
        (
            "overview/alpha/product",
            "../../alpha/product/section_01.md#s-1234abcd",
            "doc/loom/summaries/alpha/product/section_01.md",
            Some("s-1234abcd"),
        ),
        (
            "overview/alpha/engineering",
            "../../plans/a.md#s-1234abcd",
            "doc/loom/summaries/plans/a.md",
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
    let id = "overview/alpha/product";
    assert_eq!(resolve_link(id, "../../../../../../etc/passwd"), None);
    assert_eq!(resolve_link("../bad", "x"), None);
    assert_eq!(resolve_link(id, "/etc/passwd"), None);
    assert_eq!(resolve_link(id, "..\\bad"), None);
    assert_eq!(link_from_summary("product", Path::new("doc/a.md")), "");
}
