//! Tests for `crate::planning::xref`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::planning::discover::{DocDirState, DocTree, Document};
use crate::planning::kinds::{classify, doc_id, DocId};
use crate::planning::sections::sections_of;
use crate::planning::slug::{PlanEntry, PlanState};
use crate::planning::xref::{references, RefForm};

fn tree() -> DocTree {
    let paths = [
        "doc/plans/IN_PROGRESS-PLAN-a.md",
        "doc/product/section_01.md",
        "doc/product/section_02.md",
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
    let plan = PlanEntry {
        slug: "a".into(),
        path: paths[0].into(),
        state: PlanState::Running,
        strays: vec![],
    };
    DocTree {
        docs,
        plans: BTreeMap::from([("a".into(), plan)]),
        doc_dir: DocDirState::Present,
    }
}

#[test]
fn resolves_and_reports_all_forms() {
    let text = "[ok](section_01.md#rules) [bad](section_01.md#missing) [absent](missing.md)\n\
        `doc/plans/PLAN-a.md:42` `doc/missing.md` `DONE-PLAN-a.md` `PLAN-missing.md`";
    let refs = references(
        Path::new("doc/product/section_02.md"),
        text,
        &tree(),
        &|_| Some(sections_of("# Rules\n")),
    );
    assert_eq!(refs.len(), 7);
    assert_eq!(
        refs[0].target,
        Some((DocId("product/section_01".into()), Some("rules".into())))
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
    let text = "[self](#rules) [bad](#no-heading) [code](../../loom/src/main.rs) \
        [web](https://example.com) `doc/plans/PLAN-<slug>.md` `doc/plans/*.md` \
        `doc/plans/PLAN-?.md`";
    let refs = references(
        Path::new("doc/product/section_02.md"),
        text,
        &tree(),
        &|_| Some(sections_of("# Rules\n")),
    );
    assert_eq!(refs.len(), 2, "{refs:?}");
    assert_eq!(
        refs[0].target,
        Some((DocId("product/section_02".into()), Some("rules".into())))
    );
    assert!(refs[1].target.is_none());
}

fn path_refs(text: &str) -> Vec<String> {
    references(
        Path::new("doc/product/section_02.md"),
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
        path_refs("see ./doc/product/section_01.md and (../doc/product/section_02.md)"),
        ["doc/product/section_01.md", "doc/product/section_02.md"]
    );
    let refs = references(
        Path::new("doc/product/section_02.md"),
        "`doc/product/section_01.md`",
        &tree(),
        &|_| None,
    );
    assert_eq!(
        refs[0].target,
        Some((DocId("product/section_01".into()), None))
    );
}
