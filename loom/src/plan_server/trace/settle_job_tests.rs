//! `settle_job::run` over a DEMO project in a throwaway repository: a commit hook's rewrite, or a
//! change beside the commit, is reported, never marked again or amended, so the document reads as
//! changed since it was settled; an edit to the document or a brief that came first skips the
//! commit; git's words reach the settling browser alone.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tempfile::TempDir;

use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::ids::BrowserId;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::wire::trace::{SettleCommitState, SettleCommitView};
use crate::planning::sections::normalized_hash;
use crate::planning::trace::{settle, SettleState};
use crate::planning::watch::OwnWrites;

use super::route_tests::{demo, git, put, SECTION_01};
use super::settle_commit::Limits;
use super::settle_commit_tests::{hook, pre_commit, refusing_hook};
use super::settle_job::{run, BriefSeen, Outcome, SettleJob};

const PLAN: &str = "doc/plans/demo/PLAN-first.md";
const BRIEF: &str = "doc/plans/demo/briefs/first/common.md";
const HOOK_CHANGED: &str =
    "A commit hook changed the document after it was settled. Settle this version to record it.";
const CHANGED_AFTER: &str =
    "The document changed after it was settled. Settle this version to record it.";
/// Each hook run appends a line here, so a test can count them.
const RUNS: &str = "echo run >> .git/hook-runs\n";

fn context(dir: &TempDir) -> Arc<PlanContext> {
    let ctx = PlanContext::new(
        dir.path().to_path_buf(),
        ReadOnly::from_problems(Vec::new()),
        BindInfo::new("127.0.0.1:7272".parse().unwrap(), "token"),
        OwnWrites::default(),
    );
    Arc::new(ctx.unwrap())
}

/// Write `rel`'s Settled line as `POST /api/settle` does, and the job it would queue.
fn settled(dir: &TempDir, rel: &str) -> SettleJob {
    let ctx = context(dir);
    let doc = PathBuf::from(rel);
    let (feature, current, briefs, texts) = {
        let project = ctx.project();
        let (feature, traced) = project
            .features
            .iter()
            .find_map(|feature| {
                let traced = feature.trace.documents.iter().find(|d| d.path == doc)?;
                Some((feature.name.clone(), traced))
            })
            .expect("a traced document");
        let texts: Vec<Option<String>> = traced
            .bundle
            .iter()
            .map(|brief| project.text(brief).map(normalized_hash))
            .collect();
        (
            feature,
            traced.current.clone(),
            traced.bundle.clone(),
            texts,
        )
    };
    let path = ctx.root().join(rel);
    let marked = settle::mark(&std::fs::read_to_string(&path).unwrap(), &current);
    std::fs::write(&path, &marked).unwrap();
    let seen = briefs
        .iter()
        .zip(texts)
        .map(|(brief, text)| BriefSeen::of(ctx.root(), brief, text))
        .collect();
    SettleJob {
        browser: BrowserId::parse(&"a".repeat(32)).unwrap(),
        feature,
        doc,
        briefs,
        seen,
        clear: false,
        written: normalized_hash(&marked),
    }
}

/// Run `job` as the worker's job `abcd1234-1`.
fn ran(dir: &TempDir, job: &SettleJob) -> Outcome {
    run(dir.path(), "abcd1234-1", job, Limits::STANDARD)
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap()
}

fn subjects(root: &Path) -> Vec<String> {
    let log = git(root, &["log", "--format=%s"]);
    log.lines().map(str::to_owned).collect()
}

fn hook_runs(root: &Path) -> usize {
    read(root, ".git/hook-runs").lines().count()
}

/// The settle state the trace shows for `rel` now.
fn settle_state(dir: &TempDir, rel: &str) -> SettleState {
    let ctx = context(dir);
    let project = ctx.project();
    project
        .features
        .iter()
        .flat_map(|feature| &feature.trace.documents)
        .find(|doc| doc.path == Path::new(rel))
        .map(|doc| doc.settle)
        .expect("a traced document")
}

/// The view is a commit at HEAD with `reason`, the index and tree at HEAD, one settle commit.
fn assert_committed_at_head(root: &Path, view: &SettleCommitView, reason: Option<&str>) {
    assert_eq!(view.state, SettleCommitState::Committed, "{view:?}");
    assert_eq!(view.reason.as_deref(), reason, "{view:?}");
    assert_eq!(
        view.commit.as_deref(),
        Some(git(root, &["rev-parse", "HEAD"]).as_str())
    );
    assert_eq!(git(root, &["status", "--porcelain"]), "", "index and tree");
    assert_eq!(subjects(root).len(), 2, "one settle commit, never amended");
}

/// A hook that turns `from` into `to` in `rel` and adds it.
fn formatter(rel: &str, from: &str, to: &str) -> String {
    format!(
        "if grep -q '{from}' {rel}; then sed 's/{from}/{to}/' {rel} > {rel}.tmp && mv {rel}.tmp \
         {rel}; git add {rel}; fi\n{RUNS}"
    )
}

#[test]
fn a_settle_no_hook_touches_commits_with_no_reason() {
    let dir = demo(&[]);
    let job = settled(&dir, SECTION_01);
    let Outcome { settler, others } = ran(&dir, &job);
    assert_committed_at_head(dir.path(), &settler, None);
    assert_eq!(
        (settler.job.as_str(), settler.path.as_str()),
        ("abcd1234-1", SECTION_01)
    );
    assert_eq!(others, settler);
    assert_eq!(settle_state(&dir, SECTION_01), SettleState::Settled);
}

#[test]
fn a_hook_rewrite_in_the_commit_is_reported_and_reads_revised() {
    let dir = demo(&[]);
    let root = dir.path();
    pre_commit(
        root,
        &formatter(SECTION_01, "Text one\\.", "Text one, formatted."),
    );
    let job = settled(&dir, SECTION_01);
    let Outcome { settler, others } = ran(&dir, &job);
    assert_committed_at_head(root, &settler, Some(HOOK_CHANGED));
    assert_eq!(others.reason.as_deref(), Some(HOOK_CHANGED));
    let committed = git(root, &["show", &format!("HEAD:{SECTION_01}")]);
    assert!(committed.contains("Text one, formatted."), "{committed}");
    assert_eq!(hook_runs(root), 1, "the hooks ran once: no amend");
    assert_eq!(settle_state(&dir, SECTION_01), SettleState::Revised);
}

/// The commit holds what was settled, so whoever changed the file beside it, a hook or the reader
/// while the hooks ran, no hook is blamed.
#[test]
fn a_change_beside_the_commit_is_reported_without_blaming_a_hook() {
    let dir = demo(&[]);
    let root = dir.path();
    pre_commit(root, &format!("printf 'More.\\n' >> {SECTION_01}\n{RUNS}"));
    let job = settled(&dir, SECTION_01);
    let marked = read(root, SECTION_01);
    let view = ran(&dir, &job).settler;
    assert_eq!(view.state, SettleCommitState::Committed, "{view:?}");
    assert_eq!(view.reason.as_deref(), Some(CHANGED_AFTER));
    let committed = git(root, &["show", &format!("HEAD:{SECTION_01}")]);
    assert_eq!(committed, marked.trim_end());
    assert_eq!(read(root, SECTION_01), format!("{marked}More.\n"));
    assert_eq!(settle_state(&dir, SECTION_01), SettleState::Revised);
}

#[test]
fn a_plan_whose_brief_a_hook_rewrites_is_reported_and_reads_revised() {
    let dir = demo(&[(BRIEF, "# Brief\n\nShared by every worker.\n")]);
    let root = dir.path();
    pre_commit(root, &formatter(BRIEF, "every worker", "each worker"));
    let job = settled(&dir, PLAN);
    assert_eq!(job.briefs, [PathBuf::from(BRIEF)]);
    let view = ran(&dir, &job).settler;
    assert_committed_at_head(root, &view, Some(HOOK_CHANGED));
    let files = git(root, &["show", "--name-only", "--format=", "HEAD"]);
    assert_eq!(files.lines().collect::<Vec<_>>(), [PLAN, BRIEF]);
    assert_eq!(settle_state(&dir, PLAN), SettleState::Revised);
}

#[test]
fn a_rewrite_whose_commit_id_is_unknown_is_still_reported() {
    let dir = demo(&[]);
    let root = dir.path();
    hook(
        root,
        "commit-msg",
        "printf '[T-1] %s\\n' \"$(cat \"$1\")\" > \"$1.new\" && mv \"$1.new\" \"$1\"\n",
    );
    pre_commit(
        root,
        &formatter(SECTION_01, "Text one\\.", "Text one, formatted."),
    );
    let job = settled(&dir, SECTION_01);
    let Outcome { settler, others } = ran(&dir, &job);
    assert_eq!(settler.state, SettleCommitState::Committed, "{settler:?}");
    assert_eq!(settler.commit, None);
    assert_eq!(
        settler.reason.as_deref(),
        Some(
            "The commit was made, but HEAD does not show it with the settle's message, so its \
             id is not known. The document changed after it was settled. Settle this version to \
             record it."
        )
    );
    assert_eq!(
        others.reason.as_deref(),
        Some("git reported a problem with the commit")
    );
    assert_eq!(settle_state(&dir, SECTION_01), SettleState::Revised);
}

#[test]
fn an_edit_before_the_commit_skips_it() {
    let dir = demo(&[]);
    let root = dir.path();
    let job = settled(&dir, SECTION_01);
    put(
        root,
        SECTION_01,
        &format!("{}\nAn edit.\n", read(root, SECTION_01)),
    );
    let view = ran(&dir, &job).settler;
    assert_eq!(view.state, SettleCommitState::NotCommitted, "{view:?}");
    assert_eq!(
        view.reason.as_deref(),
        Some("the document changed before it could be committed; settle again")
    );
    assert_eq!(subjects(root), ["demo"]);
}

#[test]
fn a_brief_edited_before_the_commit_skips_it() {
    let dir = demo(&[(BRIEF, "# Brief\n\nShared by every worker.\n")]);
    let root = dir.path();
    let job = settled(&dir, PLAN);
    put(root, BRIEF, "# Brief\n\nShared by no one.\n");
    let view = ran(&dir, &job).settler;
    assert_eq!(view.state, SettleCommitState::NotCommitted, "{view:?}");
    assert_eq!(
        view.reason.as_deref(),
        Some("the plan's briefs changed before they could be committed; settle again")
    );
    assert_eq!(subjects(root), ["demo"]);
}

#[test]
fn a_hook_refusal_reaches_the_settling_browser_alone() {
    let dir = demo(&[]);
    refusing_hook(dir.path(), "secret hook output");
    let job = settled(&dir, SECTION_01);
    let Outcome { settler, others } = ran(&dir, &job);
    assert_eq!(settler.state, SettleCommitState::NotCommitted);
    let words = settler.reason.as_deref().expect("a reason");
    assert!(words.contains("secret hook output"), "{words}");
    assert_eq!(others.state, SettleCommitState::NotCommitted);
    assert_eq!(others.reason.as_deref(), Some("the commit failed"));
}
