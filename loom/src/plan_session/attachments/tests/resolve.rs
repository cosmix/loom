//! Tests of `resolve`: the project fixture, locks, hashes and refusals.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::read_fixture;
use crate::plan_session::attachments::resolve::resolve;
use crate::plan_session::attachments::{
    Attachment, AttachmentError, AttachmentInput, AttachmentKind, LockedTag, MAX_QUOTE_BYTES,
};
use crate::planning::project::Project;
use crate::planning::sections::normalized_hash;

const SUMMARY_TEXT: &str = "# Plan server\n\nThe planning server runs inside the loom daemon.\n";
const SUMMARY_PATH: &str = "doc/loom/summaries/plans/plan-server.md";

fn project() -> (TempDir, Project) {
    let dir = TempDir::new().unwrap();
    let files = [
        (
            "doc/plans/PLAN-plan-server.md",
            "# Plan server\n\n## Lifetime\nText.\n",
        ),
        (
            "doc/plans/PLAN-token-flow.md",
            "# Token flow\n\n## Stages\nText.\n",
        ),
        (
            "doc/plans/IN_PROGRESS-PLAN-running.md",
            "# Running\n\n## Goals\nGo.\n",
        ),
        ("doc/plans/DONE-PLAN-done.md", "# Done\n\n## Goals\nGone.\n"),
        (
            "doc/plans/RETIRED-PLAN-retired.md",
            "# Retired\n\n## Goals\nGone.\n",
        ),
        ("doc/plans/archive/DONE-PLAN-old.md", "# Old\n"),
        ("doc/plans/REVIEW-run.md", "# Review\n"),
        ("doc/plans/briefs/running/note.md", "# Brief\n"),
        (
            "doc/loom/knowledge/architecture.md",
            "# Architecture\n\n## Overview\nText.\n",
        ),
        (SUMMARY_PATH, SUMMARY_TEXT),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let project = Project::load(dir.path()).unwrap();
    (dir, project)
}

fn input(kind: AttachmentKind, path: &str, anchor: &str) -> AttachmentInput {
    AttachmentInput {
        kind,
        path: path.to_owned(),
        anchor: Some(anchor.to_owned()),
        lines: None,
        stage: None,
        summary_sentence: None,
        rests_on: Vec::new(),
        quote: "quoted".to_owned(),
        selected_hash: None,
    }
}

fn source(path: &str) -> AttachmentInput {
    input(AttachmentKind::Source, path, "goals")
}

fn summary(path: &str, hash: Option<String>) -> AttachmentInput {
    let mut summary = input(AttachmentKind::Summary, path, "s-3fa9c2d1");
    summary.summary_sentence = Some("s-3fa9c2d1".into());
    summary.selected_hash = hash;
    summary
}

#[test]
fn resolve_matches_the_summary_fixture_pair() {
    let (_dir, project) = project();
    let given: AttachmentInput =
        serde_json::from_str(&read_fixture("resolve-summary.input.json")).unwrap();
    let expected: Attachment =
        serde_json::from_str(&read_fixture("resolve-summary.resolved.json")).unwrap();
    assert_eq!(resolve(&project, given).unwrap(), expected);
}

#[test]
fn summary_change_is_judged_by_the_store_file_hash() {
    let (dir, project) = project();
    let selected = Some(normalized_hash(SUMMARY_TEXT));
    let unchanged = resolve(&project, summary(SUMMARY_PATH, selected.clone())).unwrap();
    assert!(!unchanged.changed_since_selected);
    // The hash is read from disk at resolve time, so a rewrite after selection shows.
    fs::write(
        dir.path().join(SUMMARY_PATH),
        "# Plan server\n\nNew sentence.\n",
    )
    .unwrap();
    let changed = resolve(&project, summary(SUMMARY_PATH, selected)).unwrap();
    assert!(changed.changed_since_selected);
    let unselected = resolve(&project, summary(SUMMARY_PATH, None)).unwrap();
    assert!(!unselected.changed_since_selected);
}

#[test]
fn a_missing_or_foreign_summary_is_not_a_document() {
    let (_dir, project) = project();
    for path in [
        "doc/loom/summaries/plans/missing.md",
        "doc/loom/summaries/plans/bad id.md",
        "doc/loom/summaries/other.md",
        "doc/plans/PLAN-plan-server.md",
    ] {
        let result = resolve(&project, summary(path, None));
        assert!(
            matches!(
                result,
                Err(AttachmentError::NotADocument | AttachmentError::Path(_))
            ),
            "{path}: {result:?}"
        );
    }
    let missing = resolve(
        &project,
        summary("doc/loom/summaries/plans/missing.md", None),
    );
    assert_eq!(missing, Err(AttachmentError::NotADocument));
}

#[test]
fn source_hash_decides_changed_since_selected() {
    let (_dir, project) = project();
    let path = "doc/plans/PLAN-plan-server.md";
    let current = project.content_hash(Path::new(path)).unwrap().to_owned();
    for (selected, changed) in [
        (Some(current), false),
        (Some("sha256:stale".to_owned()), true),
        (None, false),
    ] {
        let mut given = source(path);
        given.selected_hash = selected;
        assert_eq!(
            resolve(&project, given).unwrap().changed_since_selected,
            changed
        );
    }
}

#[test]
fn locks_follow_the_project() {
    let (_dir, project) = project();
    let cases = [
        ("doc/plans/PLAN-plan-server.md", None),
        (
            "doc/plans/IN_PROGRESS-PLAN-running.md",
            Some(LockedTag::Running),
        ),
        ("doc/plans/DONE-PLAN-done.md", Some(LockedTag::Done)),
        (
            "doc/plans/RETIRED-PLAN-retired.md",
            Some(LockedTag::Retired),
        ),
        ("doc/plans/archive/DONE-PLAN-old.md", Some(LockedTag::Done)),
        ("doc/plans/REVIEW-run.md", Some(LockedTag::Done)),
        ("doc/plans/briefs/running/note.md", Some(LockedTag::Brief)),
        (
            "doc/loom/knowledge/architecture.md",
            Some(LockedTag::Knowledge),
        ),
    ];
    for (path, locked) in cases {
        assert_eq!(
            resolve(&project, source(path)).unwrap().locked,
            locked,
            "{path}"
        );
    }
}

#[test]
fn a_stage_address_is_normalised_to_its_stage() {
    let (_dir, project) = project();
    let mut given = source("doc/plans/DONE-PLAN-done.md");
    given.stage = Some("build".into());
    let resolved = resolve(&project, given).unwrap();
    assert_eq!(resolved.anchor.as_deref(), Some("stage:build"));
    assert_eq!(resolved.stage.as_deref(), Some("build"));
}

#[test]
fn bad_paths_are_refused() {
    let (_dir, project) = project();
    for path in [
        "plans/PLAN-plan-server.md",
        "doc/../Cargo.toml",
        "doc//plans/x.md",
        "doc/plans/PLAN plan-server.md",
        "doc/plans/PLAN-plan-server.md#x",
        "doc/plans/\nPLAN.md",
        "/doc/plans/PLAN-plan-server.md",
        "",
    ] {
        let result = resolve(&project, source(path));
        assert_eq!(
            result,
            Err(AttachmentError::Path(path.to_owned())),
            "{path:?}"
        );
    }
    for anchor in ["two words", "line\nbreak", ""] {
        let given = input(
            AttachmentKind::Source,
            "doc/plans/PLAN-plan-server.md",
            anchor,
        );
        assert!(
            matches!(resolve(&project, given), Err(AttachmentError::Path(_))),
            "{anchor:?}"
        );
    }
}

#[test]
fn a_path_that_is_not_discovered_is_not_a_document() {
    let (_dir, project) = project();
    for path in ["doc/plans/PLAN-missing.md", SUMMARY_PATH] {
        assert_eq!(
            resolve(&project, source(path)),
            Err(AttachmentError::NotADocument),
            "{path}"
        );
    }
}

#[test]
fn rests_on_entries_are_checked() {
    let (_dir, project) = project();
    let good = "doc/plans/PLAN-token-flow.md#stages";
    let mut given = source("doc/plans/PLAN-plan-server.md");
    given.rests_on = vec![good.into()];
    assert_eq!(resolve(&project, given.clone()).unwrap().rests_on, [good]);
    for bad in [
        "doc/plans/PLAN-token-flow.md#stages\nforged",
        "doc/plans/PLAN-token-flow.md#a,b",
        "plans/PLAN-token-flow.md#stages",
        "doc/plans/PLAN-token-flow.md",
        "doc/plans/PLAN-token-flow.md#",
        "doc/plans/PLAN-missing.md#stages",
        "doc/../x.md#y",
    ] {
        given.rests_on = vec![good.into(), bad.into()];
        let result = resolve(&project, given.clone());
        assert_eq!(
            result,
            Err(AttachmentError::RestsOn(bad.to_owned())),
            "{bad:?}"
        );
    }
}

#[test]
fn sentence_ids_must_match_the_anchor_and_the_pattern() {
    let (_dir, project) = project();
    let mut given = summary(SUMMARY_PATH, None);
    given.summary_sentence = Some("s-0000ffff".into());
    assert_eq!(resolve(&project, given), Err(AttachmentError::SentenceId));
    for sentence in ["s-3FA9C2D1", "s-3fa9c2d", "x-3fa9c2d1", "s-3fa9c2d1a"] {
        let mut given = summary(SUMMARY_PATH, None);
        given.anchor = Some(sentence.into());
        given.summary_sentence = Some(sentence.into());
        assert_eq!(
            resolve(&project, given),
            Err(AttachmentError::SentenceId),
            "{sentence}"
        );
    }
    let mut given = summary(SUMMARY_PATH, None);
    given.summary_sentence = None;
    assert_eq!(resolve(&project, given), Err(AttachmentError::SentenceId));
    let mut given = source("doc/plans/PLAN-plan-server.md");
    given.summary_sentence = Some("s-3fa9c2d1".into());
    assert_eq!(resolve(&project, given), Err(AttachmentError::SentenceId));
}

#[test]
fn a_quote_over_16_kib_is_refused() {
    let (_dir, project) = project();
    let mut given = source("doc/plans/PLAN-plan-server.md");
    given.quote = "x".repeat(MAX_QUOTE_BYTES);
    assert!(resolve(&project, given.clone()).is_ok());
    given.quote.push('x');
    let result = resolve(&project, given);
    assert_eq!(
        result,
        Err(AttachmentError::QuoteTooLarge {
            bytes: MAX_QUOTE_BYTES + 1
        })
    );
}
