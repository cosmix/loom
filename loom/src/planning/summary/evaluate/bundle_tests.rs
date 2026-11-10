//! Evaluation of a plan summary over its brief bundle: a citation of a brief holds while the
//! brief's span hashes as recorded, and the summary goes incomplete when its bundle map no longer
//! matches the plan's briefs.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::cite::{cite_key, Context};
use super::fixtures::{fixture, load, metaplan, seal_document, write_file, Fixture, META};
use super::{evaluate, SummaryState};
use crate::planning::summary::format::{LinkTarget, SummaryFile};
use crate::planning::summary::hashes::SourceHashes;

const ID: &str = "plans/x";
const PX: &str = "doc/plans/demo/PLAN-x.md";
const COMMON: &str = "doc/plans/demo/briefs/x/common.md";
const Y_COMMON: &str = "doc/plans/demo/briefs/y/common.md";
const PLAN: &str = "# Plan: X\n\n## Risks\n\nSchema drift.\n";
const BRIEF: &str = "# Common\n\n## Rules\n\nRule one.\n";

/// Plans x and y of feature `demo`, each with a `common.md` brief.
fn with_briefs() -> Fixture {
    fixture(&[
        (META, &metaplan(&["PLAN-x.md", "PLAN-y.md"])),
        (PX, PLAN),
        (
            "doc/plans/demo/PLAN-y.md",
            "# Plan: Y\n\n## Risks\n\nNone.\n",
        ),
        (COMMON, BRIEF),
        (Y_COMMON, BRIEF),
    ])
}

/// plans/x sealed with one sentence citing the plan's risks.
fn sealed(f: &Fixture) -> SummaryFile {
    seal_document(&f.root, ID, &[("X watches the schema.", "risks")])
}

fn state(f: &Fixture) -> SummaryState {
    evaluate(&load(&f.root), &[]).summaries[ID].state
}

#[test]
fn removing_a_brief_leaves_the_plan_summary_incomplete() {
    let f = with_briefs();
    let file = sealed(&f);
    let keys: Vec<&str> = file.front.bundle.keys().map(String::as_str).collect();
    assert_eq!(keys, [COMMON], "the bundle holds x's brief only");
    assert_eq!(state(&f), SummaryState::Current);
    fs::remove_file(f.root.join(COMMON)).expect("remove the brief");
    assert_eq!(state(&f), SummaryState::Incomplete);
}

#[test]
fn a_plan_summary_without_its_bundle_map_is_incomplete() {
    let f = with_briefs();
    let mut file = sealed(&f);
    file.front.bundle.clear();
    write_file(&f.root, ID, &file);
    assert_eq!(state(&f), SummaryState::Incomplete);
}

#[test]
fn a_running_plans_locked_briefs_are_hashed_and_checked() {
    let running = "doc/plans/demo/IN_PROGRESS-PLAN-x.md";
    let f = fixture(&[
        (META, &metaplan(&["PLAN-x.md"])),
        (running, PLAN),
        (COMMON, BRIEF),
    ]);
    let project = load(&f.root);
    assert!(
        project.docs[Path::new(COMMON)].lock.is_some(),
        "the brief of a running plan is locked"
    );
    let file = sealed(&f);
    let recorded = file.front.bundle.get(COMMON).map(String::as_str);
    assert_eq!(recorded, project.content_hash(Path::new(COMMON)));
    assert!(recorded.is_some(), "a locked brief has a content hash");
    assert_eq!(state(&f), SummaryState::Current);
    fs::write(f.root.join(COMMON), BRIEF.replace("one", "two")).expect("edit the brief");
    assert_eq!(state(&f), SummaryState::Incomplete);
}

fn source(path: &str, anchor: &str) -> LinkTarget {
    LinkTarget::Source {
        path: path.into(),
        anchor: anchor.into(),
    }
}

/// Whether plans/x's `file` holds a citation of each target, in order.
fn holding(f: &Fixture, file: &SummaryFile, targets: &[LinkTarget]) -> Vec<bool> {
    let project = load(&f.root);
    let (hashes, done) = (SourceHashes::new(&project), BTreeMap::new());
    let ctx = Context::new(&hashes, &done, ID, file, Some(Path::new(PX)));
    targets.iter().map(|target| ctx.holds(target)).collect()
}

#[test]
fn a_brief_citation_holds_while_its_span_hashes_as_recorded() {
    let f = with_briefs();
    let mut file = sealed(&f);
    let rules = load(&f.root).docs[Path::new(COMMON)]
        .sections
        .find("rules")
        .expect("the brief's rules heading")
        .hash
        .clone();
    for brief in [COMMON, Y_COMMON] {
        file.front
            .cites
            .insert(cite_key(Path::new(brief), "rules"), rules.clone());
    }
    let targets = [
        source(COMMON, "rules"),
        source(COMMON, "missing"),
        source(Y_COMMON, "rules"),
    ];
    assert_eq!(
        holding(&f, &file, &targets),
        [true, false, false],
        "x's brief holds; a missing anchor and plan y's brief do not"
    );
    file.front
        .cites
        .insert(cite_key(Path::new(COMMON), "rules"), "sha256:old".into());
    assert_eq!(holding(&f, &file, &targets[..1]), [false], "a changed span");
}
