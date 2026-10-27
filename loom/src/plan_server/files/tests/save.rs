//! `save_document` refusals and outcomes, called in-process on a started server.

use std::net::{IpAddr, Ipv4Addr};

use tempfile::TempDir;

use super::validate::{plan, SELF_DEPENDENCY};
use crate::plan_server::authoring::{Refusal, Take};
use crate::plan_server::files::doc::{DONE_NOTICE, KNOWLEDGE_NOTICE};
use crate::plan_server::files::save::{save_document, SaveOutcome, SaveRefusal, SaveRequest};
use crate::plan_server::files::validate::NO_METADATA_NOTE;
use crate::plan_server::ids::BrowserId;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::tests::support::{serve_with, skip};
use crate::plan_server::wire::files::LockKind;
use crate::plan_server::wire::server::{MissingPrereq, PrereqKind};
use crate::plan_server::{RunningServer, ServerOptions};
use crate::planning::sections::normalized_hash;

/// A project holding `files` (repo-relative) and its started server.
fn served(files: &[(&str, &str)], read_only: ReadOnly) -> (TempDir, RunningServer) {
    let dir = tempfile::tempdir().unwrap();
    let mut all = vec![("doc/product/index.md", "# Demo\n")];
    all.extend_from_slice(files);
    for (rel, text) in all {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let server = serve_with(ServerOptions {
        root: dir.path().to_path_buf(),
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: Some(0),
        read_only,
    });
    (dir, server)
}

fn writable() -> ReadOnly {
    ReadOnly::from_problems(Vec::new())
}

fn save_as(
    server: &RunningServer,
    browser: &BrowserId,
    path: &str,
    text: &str,
    opened: &str,
) -> Result<SaveOutcome, SaveRefusal> {
    let opened_hash = normalized_hash(opened);
    let req = SaveRequest {
        path,
        text,
        opened_hash: &opened_hash,
        force: false,
    };
    save_document(&server.ctx, browser, req)
}

fn save(
    server: &RunningServer,
    path: &str,
    text: &str,
    opened: &str,
) -> Result<SaveOutcome, SaveRefusal> {
    save_as(server, &BrowserId::mint().unwrap(), path, text, opened)
}

fn read(dir: &TempDir, rel: &str) -> String {
    std::fs::read_to_string(dir.path().join(rel)).unwrap()
}

#[test]
fn a_save_writes_the_text_and_answers_its_hash() {
    if skip("a_save_writes_the_text_and_answers_its_hash") {
        return;
    }
    let (dir, server) = served(&[("doc/product/section_01.md", "# One\n")], writable());
    let result = save(
        &server,
        "product/section_01.md",
        "# One\n\nMore.\n",
        "# One\n",
    );
    let expected = SaveOutcome::Saved {
        hash: normalized_hash("# One\n\nMore.\n"),
        validation: None,
    };
    assert_eq!(result, Ok(expected));
    assert_eq!(read(&dir, "doc/product/section_01.md"), "# One\n\nMore.\n");
    server.stop().unwrap();
}

#[test]
fn a_save_is_refused_while_another_browser_holds_authoring() {
    if skip("a_save_is_refused_while_another_browser_holds_authoring") {
        return;
    }
    let (dir, server) = served(&[], writable());
    let holder = BrowserId::mint().unwrap();
    server
        .ctx
        .require_authoring(&holder, Take::TakeIfFree)
        .unwrap();
    let result = save(&server, "product/index.md", "# Mine\n", "# Demo\n");
    assert!(matches!(
        result,
        Err(SaveRefusal::Authoring(Refusal::NotHolder { .. }))
    ));
    let own = save_as(&server, &holder, "product/index.md", "# Mine\n", "# Demo\n");
    assert!(matches!(own, Ok(SaveOutcome::Saved { .. })), "{own:?}");
    assert_eq!(read(&dir, "doc/product/index.md"), "# Mine\n");
    server.stop().unwrap();
}

#[test]
fn read_only_mode_refuses_every_save() {
    if skip("read_only_mode_refuses_every_save") {
        return;
    }
    let read_only = ReadOnly::from_problems(vec![MissingPrereq {
        what: PrereqKind::Tmux,
        message: "tmux missing".into(),
        fix: "Install tmux".into(),
    }]);
    let (dir, server) = served(&[], read_only);
    let result = save(&server, "product/index.md", "# Mine\n", "# Demo\n");
    assert_eq!(result, Err(SaveRefusal::Authoring(Refusal::ReadOnlyMode)));
    assert_eq!(read(&dir, "doc/product/index.md"), "# Demo\n");
    server.stop().unwrap();
}

#[test]
fn locked_documents_are_refused_with_their_notice() {
    if skip("locked_documents_are_refused_with_their_notice") {
        return;
    }
    let running = plan(".", "true", "");
    let files = [
        ("doc/plans/DONE-PLAN-old.md", "# Old\n"),
        ("doc/plans/IN_PROGRESS-PLAN-x.md", running.as_str()),
        ("doc/plans/briefs/x/common.md", "# Common\n"),
        ("doc/loom/knowledge/INDEX.md", "# Index\n"),
    ];
    let (dir, server) = served(&files, writable());
    let refused = |path: &str, opened: &str| match save(&server, path, "# New\n", opened) {
        Err(SaveRefusal::NotEditable(lock)) => lock,
        other => panic!("{path}: {other:?}"),
    };
    let done = refused("plans/DONE-PLAN-old.md", "# Old\n");
    assert_eq!(
        (done.reason, done.notice.as_str()),
        (LockKind::Done, DONE_NOTICE)
    );
    let knowledge = refused("loom/knowledge/INDEX.md", "# Index\n");
    assert_eq!(knowledge.reason, LockKind::Knowledge);
    assert_eq!(knowledge.notice, KNOWLEDGE_NOTICE);
    let brief = refused("plans/briefs/x/common.md", "# Common\n");
    assert_eq!(brief.reason, LockKind::BriefOfLockedPlan);
    assert!(
        brief.notice.contains('x') && brief.notice.contains("is running."),
        "{brief:?}"
    );
    assert_eq!(
        brief.plan.as_deref(),
        Some("doc/plans/IN_PROGRESS-PLAN-x.md")
    );
    for (rel, text) in files {
        assert_eq!(read(&dir, rel), text, "{rel} changed");
    }
    server.stop().unwrap();
}

#[test]
fn paths_that_are_not_documents_are_refused_and_never_created() {
    if skip("paths_that_are_not_documents_are_refused_and_never_created") {
        return;
    }
    let files = [
        ("doc/loom/summaries/s.md", "# Summary\n"),
        ("doc/product/data.txt", "data\n"),
    ];
    let (dir, server) = served(&files, writable());
    assert_eq!(
        save(&server, "product/new.md", "# New\n", ""),
        Err(SaveRefusal::NotFound)
    );
    assert!(!dir.path().join("doc/product/new.md").exists());
    let summary = save(&server, "loom/summaries/s.md", "# New\n", "# Summary\n");
    assert_eq!(summary, Err(SaveRefusal::NotFound));
    assert!(matches!(
        save(&server, "../x.md", "# X\n", ""),
        Err(SaveRefusal::Path(_))
    ));
    let text_file = save(&server, "product/data.txt", "new\n", "data\n");
    assert!(
        matches!(text_file, Err(SaveRefusal::Path(_))),
        "{text_file:?}"
    );
    assert_eq!(read(&dir, "doc/product/data.txt"), "data\n");
    server.stop().unwrap();
}

#[test]
fn text_over_one_mib_is_too_large() {
    if skip("text_over_one_mib_is_too_large") {
        return;
    }
    let (dir, server) = served(&[], writable());
    let huge = "x".repeat(1_048_577);
    assert_eq!(
        save(&server, "product/index.md", &huge, "# Demo\n"),
        Err(SaveRefusal::TooLarge)
    );
    assert_eq!(read(&dir, "doc/product/index.md"), "# Demo\n");
    server.stop().unwrap();
}

#[test]
fn plan_saves_carry_validation() {
    if skip("plan_saves_carry_validation") {
        return;
    }
    let valid = plan(".", "true", "");
    let (dir, server) = served(&[("doc/plans/PLAN-x.md", valid.as_str())], writable());
    // One author: a written save holds authoring for its browser.
    let author = BrowserId::mint().unwrap();
    let prose = "# Plan X\n\nDrafting prose first.\n";
    let validation = match save_as(&server, &author, "plans/PLAN-x.md", prose, &valid) {
        Ok(SaveOutcome::Saved {
            validation: Some(validation),
            ..
        }) => validation,
        other => panic!("a plan without metadata saves: {other:?}"),
    };
    assert!(validation.valid);
    assert_eq!(validation.notes, vec![NO_METADATA_NOTE.to_owned()]);
    let warned = plan("loom", "loom/target/debug/loom --help", "");
    match save_as(&server, &author, "plans/PLAN-x.md", &warned, prose) {
        Ok(SaveOutcome::Saved {
            validation: Some(validation),
            ..
        }) => {
            assert!(!validation.warnings.structural.is_empty(), "{validation:?}");
        }
        other => panic!("warnings never block a save: {other:?}"),
    }
    let invalid = plan(".", "true", SELF_DEPENDENCY);
    let blocked = save_as(&server, &author, "plans/PLAN-x.md", &invalid, &warned);
    assert!(
        matches!(blocked, Ok(SaveOutcome::Invalid { .. })),
        "{blocked:?}"
    );
    assert_eq!(read(&dir, "doc/plans/PLAN-x.md"), warned);
    server.stop().unwrap();
}
