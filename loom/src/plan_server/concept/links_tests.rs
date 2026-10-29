//! Tests for implements links.

use super::links::*;
use crate::plan_server::wire::concept::{DocLink, ImplementsView, PlanLink, SectionLink};
use crate::planning::project::Project;
use std::fs;
use tempfile::TempDir;

fn fixture(files: &[(&str, &str)]) -> (TempDir, Project) {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("doc")).unwrap();
    for (path, content) in files {
        let path = temp.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    let project = Project::load(temp.path()).unwrap();
    (temp, project)
}

#[test]
fn unmapped_is_not_stated() {
    let (_temp, project) = fixture(&[("doc/plans/demo/PLAN-a.md", "# A\n")]);
    assert_eq!(implements_view(&project, "a"), ImplementsView::NotStated);
    assert_eq!(
        implements_view(&project, "missing"),
        ImplementsView::NotStated
    );
}

#[test]
fn none_declared_is_empty() {
    let (_temp, project) = fixture(&[("doc/plans/demo/PLAN-a.md", "# A\nImplements: none\n")]);
    assert_eq!(
        implements_view(&project, "a"),
        ImplementsView::Sections { links: vec![] }
    );
}

#[test]
fn bad_token_kept_as_written() {
    let (_temp, project) = fixture(&[
        (
            "doc/plans/demo/PLAN-a.md",
            "# A\nImplements: section_02, section_03, Bad-Token\n",
        ),
        ("doc/product/demo/section_02.md", "# Product title\n"),
    ]);
    assert_eq!(
        implements_view(&project, "a"),
        ImplementsView::Sections {
            links: vec![
                SectionLink {
                    number: Some(2),
                    name: "Product title".into(),
                    doc: Some(DocLink {
                        id: "demo/product/section_02".into(),
                        title: "Product title".into(),
                        path: "doc/product/demo/section_02.md".into(),
                        page: "demo/product/section_02".into(),
                    })
                },
                SectionLink {
                    number: Some(3),
                    name: "section_03".into(),
                    doc: None
                },
                SectionLink {
                    number: None,
                    name: "Bad-Token".into(),
                    doc: None
                },
            ]
        }
    );
}

#[test]
fn implemented_by_in_slug_order() {
    let (_temp, project) = fixture(&[
        ("doc/plans/demo/index.md", "# Plans\n\n## A (`PLAN-a.md`)\n\n## B (`PLAN-b.md`)\n\n## C (`PLAN-c.md`)\n\n## D (`PLAN-d.md`)\n"),
        ("doc/plans/demo/PLAN-b.md", "# Beta\nImplements: section_02\n"),
        (
            "doc/plans/demo/IN_PROGRESS-PLAN-a.md",
            "# Alpha\nImplements: section_02\n",
        ),
        (
            "doc/plans/demo/DONE-PLAN-c.md",
            "# Gamma\nImplements: section_02\n",
        ),
        (
            "doc/plans/demo/RETIRED-PLAN-d.md",
            "# Delta\nImplements: section_02\n",
        ),
    ]);
    assert_eq!(
        implemented_by(&project, "demo", 2),
        vec![
            PlanLink {
                slug: "a".into(),
                title: "Alpha".into(),
                state: "running".into()
            },
            PlanLink {
                slug: "b".into(),
                title: "Beta".into(),
                state: "written".into()
            },
            PlanLink {
                slug: "c".into(),
                title: "Gamma".into(),
                state: "done".into()
            },
            PlanLink {
                slug: "d".into(),
                title: "Delta".into(),
                state: "retired".into()
            },
        ]
    );
    assert!(implemented_by(&project, "demo", 99).is_empty());
}

#[test]
fn implemented_by_skips_plans_without_a_concept_page() {
    let (_temp, project) = fixture(&[
        ("doc/plans/demo/index.md", "# Plans\n\n## B (`PLAN-b.md`)\n"),
        (
            "doc/plans/demo/PLAN-a#b.md",
            "# Hash\nImplements: section_02\n",
        ),
        (
            "doc/plans/demo/archive/DONE-PLAN-c.md",
            "# Archived\nImplements: section_02\n",
        ),
        (
            "doc/plans/demo/PLAN-b.md",
            "# Beta\nImplements: section_02\n",
        ),
    ]);
    assert_eq!(
        project
            .feature("demo")
            .unwrap()
            .implemented_by
            .get(&2)
            .map(|slugs| slugs.len()),
        Some(3)
    );
    assert_eq!(
        implemented_by(&project, "demo", 2),
        vec![PlanLink {
            slug: "b".into(),
            title: "Beta".into(),
            state: "written".into()
        }]
    );
}

#[test]
fn implements_and_implemented_by_resolve_in_the_plans_own_feature() {
    let (_temp, project) = fixture(&[
        ("doc/product/demo/section_02.md", "# Demo section\n"),
        ("doc/product/other/section_02.md", "# Other section\n"),
        ("doc/plans/demo/index.md", "# Plans\n\n## A (`PLAN-a.md`)\n"),
        (
            "doc/plans/other/index.md",
            "# Plans\n\n## B (`PLAN-b.md`)\n",
        ),
        (
            "doc/plans/demo/PLAN-a.md",
            "# A\n\nImplements: section_02\n",
        ),
        (
            "doc/plans/other/PLAN-b.md",
            "# B\n\nImplements: section_02\n",
        ),
    ]);

    let ImplementsView::Sections { links } = implements_view(&project, "b") else {
        panic!("section links");
    };

    let doc = links[0].doc.as_ref().unwrap();
    assert_eq!(doc.page, "other/product/section_02");
    assert_eq!(doc.title, "Other section");
    assert_eq!(implemented_by(&project, "demo", 2)[0].slug, "a");
    assert_eq!(implemented_by(&project, "other", 2)[0].slug, "b");
}
