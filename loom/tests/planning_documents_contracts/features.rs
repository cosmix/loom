use super::{p, put, tmp};
use loom::planning::checklist::Offer;
use loom::planning::discover::{discover, read_doc, DocDirState};
use loom::planning::kinds::{classify, DocKind};
use loom::planning::project::Project;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

fn problem_paths(project: &Project, code: &str) -> Vec<PathBuf> {
    project
        .problems
        .iter()
        .filter(|problem| problem.code == code)
        .map(|problem| problem.path.clone())
        .collect()
}

#[test]
fn invalid_feature_names_are_other_and_reported() {
    for name in ["Bad", "a_b", "archive", "briefs", "plans", "overview"] {
        let (_temp, root) = tmp();
        for file in ["index.md", "section_07.md", "notes.md"] {
            let rel = format!("doc/product/{name}/{file}");
            assert_eq!(classify(Path::new(&rel)), DocKind::Other, "{rel}");
            put(&root, &rel, "# Invalid feature\n");
        }
        let project = Project::load(&root).unwrap();
        assert_eq!(
            problem_paths(&project, "bad-feature-name"),
            [p(&format!("doc/product/{name}"))]
        );
        assert!(project.tree.feature_names().is_empty(), "{name}");
        assert!(project.features.is_empty(), "{name}");
    }
}

#[test]
fn invalid_plan_feature_names_are_other_and_reported() {
    // archive and briefs are structural standalone directories, not candidate features.
    for name in ["Bad", "a_b", "plans", "overview"] {
        let (_temp, root) = tmp();
        for file in ["index.md", "PLAN-x.md", "archive/DONE-PLAN-x.md"] {
            let rel = format!("doc/plans/{name}/{file}");
            assert_eq!(classify(Path::new(&rel)), DocKind::Other, "{rel}");
            put(&root, &rel, "# Invalid feature\n");
        }
        let project = Project::load(&root).unwrap();
        assert_eq!(
            problem_paths(&project, "bad-feature-name"),
            [p(&format!("doc/plans/{name}"))]
        );
        assert!(project.tree.plans.is_empty(), "{name}");
        assert!(project.features.is_empty(), "{name}");
    }
}

#[test]
fn misplaced_documents_need_features_and_offer_moves_without_a_fallback_feature() {
    let (_temp, root) = tmp();
    let misplaced = [
        "doc/plans/index.md",
        "doc/plans/round-2-metaplan.md",
        "doc/product/index.md",
        "doc/product/section_07.md",
    ];
    for rel in misplaced {
        put(&root, rel, "# Misplaced\n");
    }
    put(
        &root,
        "doc/plans/PLAN-solo.md",
        "# Solo\nImplements: none\n",
    );
    let project = Project::load(&root).unwrap();
    assert_eq!(problem_paths(&project, "needs-feature"), misplaced.map(p));
    assert_eq!(
        project.moves,
        misplaced.map(|rel| Offer::MoveIntoFeature { path: p(rel) })
    );
    assert!(project.features.is_empty());
    assert!(project.tree.feature_names().is_empty());
    assert_eq!(project.standalone, ["solo"]);
}

fn write_feature_discovery_fixture(root: &Path) -> [&'static str; 6] {
    let demo = [
        "doc/plans/demo/PLAN-x.md",
        "doc/plans/demo/archive/DONE-PLAN-x.md",
        "doc/plans/demo/index.md",
        "doc/product/demo/index.md",
        "doc/product/demo/notes.md",
        "doc/product/demo/section_07.md",
    ];
    for rel in demo {
        put(root, rel, "# Demo\n");
    }
    for rel in [
        "doc/plans/engine/PLAN-engine.md",
        "doc/product/spec/index.md",
        "doc/plans/PLAN-z.md",
        "doc/plans/archive/DONE-PLAN-a.md",
        "doc/plans/demo/briefs/x/common.md",
        "doc/plans/demo/REVIEW-PLAN-x.md",
    ] {
        put(root, rel, "# Document\n");
    }
    fs::create_dir_all(root.join("doc/product/empty")).unwrap();
    demo
}

#[test]
fn discovery_and_project_group_features_and_standalone_plans() {
    let (_temp, root) = tmp();
    let demo = write_feature_discovery_fixture(&root);
    let project = Project::load(&root).unwrap();
    let names: Vec<_> = project.tree.feature_names().into_iter().collect();
    assert_eq!(names, ["demo", "engine", "spec"]);
    let paths: Vec<_> = project
        .tree
        .feature_docs("demo")
        .map(|doc| doc.path.clone())
        .collect();
    assert_eq!(paths, demo.map(p));
    assert_eq!(
        project.tree.by_id("product/demo/index").unwrap().path,
        p("doc/product/demo/index.md")
    );
    assert_eq!(
        project.tree.by_id("product/demo/section_07").unwrap().path,
        p("doc/product/demo/section_07.md")
    );
    let features: Vec<_> = project
        .features
        .iter()
        .map(|feature| feature.name.as_str())
        .collect();
    assert_eq!(features, ["demo", "engine", "spec"]);
    assert!(project.feature("demo").is_some());
    assert!(project.feature("empty").is_none());
    assert_eq!(project.standalone, ["a", "z"]);
    assert!(project.moves.is_empty());
    assert!(problem_paths(&project, "needs-feature").is_empty());
}

#[test]
fn feature_discovery_preserves_temporary_summary_and_symlink_safety() {
    let (_temp, root) = tmp();
    for rel in [
        "doc/product/demo/index.md",
        "doc/product/demo/.hidden.md",
        "doc/product/demo/backup.md~",
        "doc/product/demo/swap.md.swp",
        "doc/plans/demo/.hidden/PLAN-x.md",
        "doc/loom/summaries/demo/product.md",
        ".env",
    ] {
        put(&root, rel, "# Demo\n");
    }
    let dir = root.join("doc/product/demo");
    symlink("index.md", dir.join("link-in.md")).unwrap();
    symlink("../../../.env", dir.join("link-out.md")).unwrap();
    symlink(
        "../../loom/summaries/demo/product.md",
        dir.join("link-summary.md"),
    )
    .unwrap();
    symlink(".", dir.join("dir-link")).unwrap();
    let tree = discover(&root).unwrap();
    let paths: Vec<_> = tree.docs.iter().map(|doc| doc.path.clone()).collect();
    assert_eq!(
        paths,
        [
            p("doc/product/demo/index.md"),
            p("doc/product/demo/link-in.md")
        ]
    );
    assert!(
        tree.get(Path::new("doc/product/demo/link-in.md"))
            .unwrap()
            .via_symlink
    );
    assert_eq!(
        read_doc(&root, Path::new("doc/product/demo/link-in.md")).unwrap(),
        "# Demo\n"
    );
    assert!(read_doc(&root, Path::new("doc/product/demo/link-out.md")).is_err());
}

#[test]
fn discovery_does_not_follow_a_symlinked_doc_root() {
    let (_temp, root) = tmp();
    put(&root, "real/product/demo/index.md", "# Demo\n");
    symlink("real", root.join("doc")).unwrap();
    let tree = discover(&root).unwrap();
    assert_eq!(tree.doc_dir, DocDirState::Symlinked);
    assert!(tree.docs.is_empty());
    assert!(tree.feature_names().is_empty());
}
