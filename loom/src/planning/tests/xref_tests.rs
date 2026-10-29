//! Tests for `crate::planning::xref`.

use std::path::{Path, PathBuf};

use crate::planning::discover::{DocDirState, DocTree, Document};
use crate::planning::kinds::{classify, doc_id, DocId};
use crate::planning::sections::sections_of;
use crate::planning::slug::resolve_plans;
use crate::planning::xref::{references, RefForm};

fn tree() -> DocTree {
    tree_at("doc/plans/demo/IN_PROGRESS-PLAN-a.md")
}

fn tree_at(plan_path: &str) -> DocTree {
    let paths = [
        plan_path,
        "doc/product/demo/section_01.md",
        "doc/product/demo/section_02.md",
    ];
    let docs = paths
        .iter()
        .map(|path| {
            let path = PathBuf::from(path);
            let kind = classify(&path);
            Document {
                id: doc_id(&path, &kind),
                path,
                kind,
                size: 0,
                via_symlink: false,
            }
        })
        .collect();
    DocTree {
        docs,
        plans: resolve_plans([PathBuf::from(plan_path)].into_iter()),
        doc_dir: DocDirState::Present,
    }
}

#[test]
fn resolves_and_reports_all_forms() {
    let text = "[ok](section_01.md#rules) [bad](section_01.md#missing) [absent](missing.md)\n\
        `doc/plans/PLAN-a.md:42` `doc/missing.md` `DONE-PLAN-a.md` `PLAN-missing.md`";
    let refs = references(
        Path::new("doc/product/demo/section_02.md"),
        text,
        &tree(),
        &|_| Some(sections_of("# Rules\n")),
    );
    assert_eq!(refs.len(), 7);
    assert_eq!(
        refs[0].target,
        Some((
            DocId("product/demo/section_01".into()),
            Some("rules".into())
        ))
    );
    assert!(refs[1].target.is_none());
    assert!(refs[2].target.is_none());
    assert_eq!(refs[3].form, RefForm::PathToken);
    assert_eq!(refs[3].target, Some((DocId("plan/a".into()), None)));
    assert!(refs[4].target.is_none());
    assert_eq!(refs[5].form, RefForm::PlanFile);
    assert_eq!(refs[5].target, Some((DocId("plan/a".into()), None)));
    assert!(refs[6].target.is_none());
    assert_eq!(refs[3].line, 1);
}

#[test]
fn skips_code_urls_templates_and_globs_and_resolves_self_fragments() {
    let text = "[self](#rules) [bad](#no-heading) [code](../../../loom/src/main.rs) \
        [web](https://example.com) `doc/plans/PLAN-<slug>.md` `doc/plans/*.md` \
        `doc/plans/PLAN-?.md`";
    let refs = references(
        Path::new("doc/product/demo/section_02.md"),
        text,
        &tree(),
        &|_| Some(sections_of("# Rules\n")),
    );
    assert_eq!(refs.len(), 2, "{refs:?}");
    assert_eq!(
        refs[0].target,
        Some((
            DocId("product/demo/section_02".into()),
            Some("rules".into())
        ))
    );
    assert!(refs[1].target.is_none());
}

fn path_refs(text: &str) -> Vec<String> {
    references(
        Path::new("doc/product/demo/section_02.md"),
        text,
        &tree(),
        &|_| None,
    )
    .into_iter()
    .filter(|r| r.form == RefForm::PathToken)
    .map(|r| r.raw)
    .collect()
}

#[test]
fn only_token_start_doc_paths_are_references() {
    assert!(path_refs("see `target/doc/loom/index.html` and loom/src/doc/x.rs").is_empty());
    assert!(path_refs("the `doc/product/` directory").is_empty());
    assert_eq!(
        path_refs("see ./doc/product/demo/section_01.md and (../doc/product/demo/section_02.md)"),
        [
            "doc/product/demo/section_01.md",
            "doc/product/demo/section_02.md"
        ]
    );
    let refs = references(
        Path::new("doc/product/demo/section_02.md"),
        "`doc/product/demo/section_01.md`",
        &tree(),
        &|_| None,
    );
    assert_eq!(
        refs[0].target,
        Some((DocId("product/demo/section_01".into()), None))
    );
}

fn tree_with(extra: &[&str]) -> DocTree {
    let mut tree = tree();
    for path in extra {
        let path = PathBuf::from(path);
        let kind = classify(&path);
        tree.docs.push(Document {
            id: doc_id(&path, &kind),
            path,
            kind,
            size: 0,
            via_symlink: false,
        });
    }
    tree.docs.sort_by(|a, b| a.path.cmp(&b.path));
    tree
}

#[test]
fn relative_paths_and_anchors_resolve_across_features() {
    let tree = tree_with(&[
        "doc/product/other/section_01.md",
        "doc/plans/other/PLAN-b.md",
    ]);
    let text = "[section](../other/section_01.md#rules) \
        [plan](../../plans/other/PLAN-b.md#delivery) \
        [bad](../other/section_01.md#missing) `doc/product/other/section_01.md`";
    let refs = references(
        Path::new("doc/product/demo/section_02.md"),
        text,
        &tree,
        &|path| {
            Some(sections_of(if path.starts_with("doc/plans") {
                "# Delivery\n"
            } else {
                "# Rules\n"
            }))
        },
    );
    let targets: Vec<_> = refs.into_iter().map(|reference| reference.target).collect();
    assert_eq!(
        targets,
        [
            Some((
                DocId("product/other/section_01".into()),
                Some("rules".into())
            )),
            Some((DocId("plan/b".into()), Some("delivery".into()))),
            None,
            Some((DocId("product/other/section_01".into()), None)),
        ]
    );
}

fn refs_in(text: &str) -> Vec<(RefForm, String, usize, bool)> {
    references(
        Path::new("doc/product/demo/section_02.md"),
        text,
        &tree(),
        &|_| None,
    )
    .into_iter()
    .map(|r| (r.form, r.raw, r.line, r.target.is_some()))
    .collect()
}

#[test]
fn fenced_code_yields_no_reference_and_keeps_line_numbers() {
    let body = "[x](missing.md) `doc/plans/PLAN-x.md`";
    assert!(refs_in(&format!("```text\n{body}\n```\n")).is_empty());
    assert!(refs_in(&format!("~~~\n{body}\n~~~\n")).is_empty());
    let outside = refs_in(&format!("```text\n{body}\n```\n{body}\n"));
    assert_eq!(
        outside,
        [
            (RefForm::RelativeLink, "missing.md".to_string(), 3, false),
            (
                RefForm::PathToken,
                "doc/plans/PLAN-x.md".to_string(),
                3,
                false
            ),
        ]
    );
}

#[test]
fn any_colon_suffix_is_cut_from_a_path_token() {
    for suffix in [":15-77", ":12:5", ":42"] {
        let got = refs_in(&format!("see `doc/product/demo/section_01.md{suffix}`"));
        let want = [(
            RefForm::PathToken,
            "doc/product/demo/section_01.md".to_string(),
            0,
            true,
        )];
        assert_eq!(got, want, "{suffix}");
    }
}

#[test]
fn directories_dotfiles_summaries_and_placeholders_are_not_references() {
    let tree = tree_with(&["doc/loom/knowledge/concerns/x.md"]);
    for text in [
        "`doc/plans`",
        "`doc/product`",
        "`doc/loom/knowledge`",
        "`doc/loom/knowledge/.bootstrap-receipt.json`",
        "`doc/.hidden/a.md`",
        "`doc/loom/summaries/plans/foo.md`",
        "`doc/loom/summaries`",
        "`doc/plans/$1`",
        "`doc/plans/PLAN-${slug}.md`",
        "`doc/plans/{a,b}.md`",
    ] {
        let refs = references(
            Path::new("doc/product/demo/section_02.md"),
            text,
            &tree,
            &|_| None,
        );
        let tokens: Vec<_> = refs
            .iter()
            .filter(|r| r.form == RefForm::PathToken)
            .collect();
        assert!(tokens.is_empty(), "{text}: {tokens:?}");
    }
}

#[test]
fn a_nonexistent_directory_is_still_a_broken_reference() {
    assert_eq!(
        refs_in("`doc/typo` and `doc/product/typo` and `doc/prod`"),
        [
            (RefForm::PathToken, "doc/typo".to_string(), 0, false),
            (RefForm::PathToken, "doc/product/typo".to_string(), 0, false),
            (RefForm::PathToken, "doc/prod".to_string(), 0, false),
        ]
    );
}

fn assert_plan_path_resolves(old: &str, current: &str) {
    let tree = tree_at(current);
    let text = format!("[plan](../../{old}#delivery) `{old}`");
    let refs = references(Path::new("doc/plans/reader.md"), &text, &tree, &|path| {
        assert_eq!(path, Path::new(current));
        Some(sections_of("# Delivery\n"))
    });
    let targets: Vec<_> = refs.into_iter().map(|reference| reference.target).collect();
    assert_eq!(
        targets,
        [
            Some((DocId("plan/a".into()), Some("delivery".into()))),
            Some((DocId("plan/a".into()), None)),
        ]
    );
}

#[test]
fn feature_plan_paths_resolve_after_starting() {
    assert_plan_path_resolves(
        "doc/plans/demo/PLAN-a.md",
        "doc/plans/demo/IN_PROGRESS-PLAN-a.md",
    );
}

#[test]
fn feature_plan_paths_resolve_after_archiving() {
    assert_plan_path_resolves(
        "doc/plans/demo/PLAN-a.md",
        "doc/plans/demo/archive/DONE-PLAN-a.md",
    );
}

#[test]
fn standalone_plan_paths_still_resolve() {
    assert_plan_path_resolves("doc/plans/PLAN-a.md", "doc/plans/IN_PROGRESS-PLAN-a.md");
}
