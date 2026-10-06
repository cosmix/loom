//! A relayed `commit`, driven through the drain against a real repository:
//! which session may move which branch, the refusals that leave it unmoved,
//! and where a signing failure stops the work.

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::{json, Value};

use crate::fs::inbox::{write_entry, LedgerOutcome, LedgerRecord};
use crate::fs::session_files::save_session;
use crate::git::signing::tests::{fake_signer, git_in};
use crate::git::target_guard::{LEDGER_FILE, REFS_FILE};
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::StageStatus;
use crate::relay::{AgentRole, RequestKind};
use crate::verify::contracts::test_support::contract_worktree;
use crate::verify::transitions::{load_stage, update_stage};

use super::run_pass;
use super::test_support::{entry_for, fixture, FakeHost, Fixture, STAGE};

#[path = "commit_tests_signing.rs"]
mod signing;

const BRANCH: &str = "refs/heads/loom/s1";

/// The fixture's repository on `main` with the stage worktree on `loom/s1`;
/// an identity and unsigned commits are set repo-locally, because the daemon
/// runs git without this module's environment. Returns the worktree.
fn repository(fx: &Fixture) -> PathBuf {
    let worktree = contract_worktree(&fx.repo_root, STAGE);
    git_in(&fx.repo_root, &["config", "user.name", "t"]);
    git_in(&fx.repo_root, &["config", "user.email", "t@t.com"]);
    git_in(&fx.repo_root, &["config", "commit.gpgsign", "false"]);
    worktree
}

/// The stage branch and `main` one commit apart each, then `main` merged
/// into the worktree with `--no-commit`: `MERGE_HEAD` set and the merge
/// staged, as a resolver leaves it. Returns the worktree.
fn merging_repository(fx: &Fixture) -> PathBuf {
    let worktree = repository(fx);
    add_file(&worktree, "b.txt");
    git_in(
        &worktree,
        &["commit", "-q", "-m", "test(relay): stage work"],
    );
    add_file(&fx.repo_root, "c.txt");
    git_in(
        &fx.repo_root,
        &["commit", "-q", "-m", "test(relay): main work"],
    );
    git_in(&worktree, &["merge", "--no-commit", "--no-ff", "main"]);
    worktree
}

fn add_file(checkout: &Path, path: &str) {
    let file = checkout.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, format!("{path}\n")).unwrap();
    git_in(checkout, &["add", path]);
}

/// What a session that saw `checkout`'s HEAD and index relays.
fn staged_payload(checkout: &Path) -> Value {
    json!({
        "message": "test(relay): commit staged changes",
        "expected_head": git_in(checkout, &["rev-parse", "HEAD"]),
        "expected_tree": git_in(checkout, &["write-tree"]),
    })
}

fn tip(checkout: &Path, reference: &str) -> String {
    git_in(checkout, &["rev-parse", reference])
}

/// [`STAGE`] in `status`, owned by `owner`, its worktree recorded.
fn stage_in(fx: &Fixture, status: StageStatus, owner: Option<&str>) {
    fx.stage(status, owner);
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.worktree = Some(STAGE.to_string());
        Ok(())
    })
    .unwrap();
}

/// A running Stage session owning the executing [`STAGE`].
fn stage_session(fx: &Fixture) -> Session {
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    stage_in(fx, StageStatus::Executing, Some(&record.id));
    record
}

/// A running Knowledge session owning the executing [`STAGE`].
fn knowledge_session(fx: &Fixture) -> Session {
    let record = fx.record(SessionType::Knowledge, SessionStatus::Running);
    fx.stage(StageStatus::Executing, Some(&record.id));
    record
}

/// A running Merge session resolving [`STAGE`]'s merge.
fn merge_session(fx: &Fixture) -> Session {
    let mut record = fx.record(SessionType::Merge, SessionStatus::Running);
    record.merge_source_branch = Some(format!("loom/{STAGE}"));
    save_session(&record, &fx.work_dir).unwrap();
    record
}

/// Relay `body` as `record`'s commit, drain once, and return the request's
/// latest ledger row.
fn relay_commit(fx: &Fixture, host: &mut FakeHost, record: &Session, body: Value) -> LedgerRecord {
    let entry = fx.relay(record, RequestKind::Commit, body);
    drain_and_read(fx, host, &record.id, &entry.id)
}

fn drain_and_read(fx: &Fixture, host: &mut FakeHost, sid: &str, id: &str) -> LedgerRecord {
    run_pass(host, &fx.tick(Utc::now()));
    fx.ledger(sid)
        .into_iter()
        .rev()
        .find(|row| row.id == id)
        .expect("every relayed request gets a ledger row")
}

/// The row's reason, after checking it settled as `outcome`.
fn reason_of(row: &LedgerRecord, outcome: LedgerOutcome) -> String {
    assert_eq!(row.outcome, Some(outcome), "{row:?}");
    row.reason.clone().unwrap_or_default()
}

#[test]
fn a_stage_commit_is_applied_and_the_ledger_carries_the_commit_id() {
    let fx = fixture();
    let worktree = repository(&fx);
    let record = stage_session(&fx);
    let head = tip(&worktree, "HEAD");
    add_file(&worktree, "a.txt");

    let row = relay_commit(&fx, &mut fx.host(true), &record, staged_payload(&worktree));

    let note = reason_of(&row, LedgerOutcome::Applied);
    let id = tip(&worktree, BRANCH);
    assert_eq!(note, format!("committed {id}"));
    assert_eq!(tip(&worktree, &format!("{id}^")), head);
}

#[test]
fn a_moved_head_is_refused_with_the_ref_unmoved() {
    let fx = fixture();
    let worktree = repository(&fx);
    let record = stage_session(&fx);
    let seen_head = tip(&worktree, "HEAD");
    add_file(&worktree, "b.txt");
    git_in(&worktree, &["commit", "-q", "-m", "test(relay): moved"]);
    let moved = tip(&worktree, BRANCH);
    add_file(&worktree, "c.txt");
    let mut payload = staged_payload(&worktree);
    payload["expected_head"] = json!(seen_head);

    let row = relay_commit(&fx, &mut fx.host(true), &record, payload);

    reason_of(&row, LedgerOutcome::Refused);
    assert_eq!(tip(&worktree, BRANCH), moved);
}

#[test]
fn a_stage_that_is_not_executing_is_refused() {
    let fx = fixture();
    let worktree = repository(&fx);
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    stage_in(&fx, StageStatus::Blocked, Some(&record.id));
    let before = tip(&worktree, BRANCH);
    add_file(&worktree, "a.txt");

    let row = relay_commit(&fx, &mut fx.host(true), &record, staged_payload(&worktree));

    let reason = reason_of(&row, LedgerOutcome::Refused);
    assert!(reason.contains("not Executing"), "{reason}");
    assert_eq!(tip(&worktree, BRANCH), before);
}

#[test]
fn a_session_that_does_not_own_the_stage_is_refused() {
    let fx = fixture();
    let worktree = repository(&fx);
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    stage_in(&fx, StageStatus::Executing, Some("session-other"));
    let before = tip(&worktree, BRANCH);
    add_file(&worktree, "a.txt");

    let row = relay_commit(&fx, &mut fx.host(true), &record, staged_payload(&worktree));

    let reason = reason_of(&row, LedgerOutcome::Refused);
    assert!(reason.contains("not the active session"), "{reason}");
    assert_eq!(tip(&worktree, BRANCH), before);
}

#[test]
fn a_contract_session_is_refused_by_the_matrix() {
    let fx = fixture();
    let worktree = repository(&fx);
    let record = fx.record(SessionType::Contract, SessionStatus::Running);
    stage_in(&fx, StageStatus::Executing, Some(&record.id));
    let before = tip(&worktree, BRANCH);
    add_file(&worktree, "a.txt");

    let row = relay_commit(&fx, &mut fx.host(true), &record, staged_payload(&worktree));

    let reason = reason_of(&row, LedgerOutcome::Refused);
    assert!(
        reason.contains("may not relay a 'commit' request"),
        "{reason}"
    );
    assert_eq!(tip(&worktree, BRANCH), before);
}

#[test]
fn a_subagent_commit_entry_is_refused_in_admit() {
    let fx = fixture();
    let worktree = repository(&fx);
    let record = stage_session(&fx);
    let before = tip(&worktree, BRANCH);
    add_file(&worktree, "a.txt");
    let mut entry = entry_for(&record, RequestKind::Commit, staged_payload(&worktree));
    entry.agent = AgentRole::Subagent;
    write_entry(&fx.work_dir, &entry).unwrap();

    let row = drain_and_read(&fx, &mut fx.host(true), &record.id, &entry.id);

    let reason = reason_of(&row, LedgerOutcome::Refused);
    assert!(reason.contains("subagent"), "{reason}");
    assert_eq!(tip(&worktree, BRANCH), before);
}

/// A Merge session's commit fails to sign with the stage in `status`.
fn merge_signing_failure_from(status: StageStatus) {
    let fx = fixture();
    let worktree = merging_repository(&fx);
    let record = merge_session(&fx);
    stage_in(&fx, status.clone(), None);
    let before = tip(&worktree, BRANCH);
    fake_signer(&fx.repo_root, true);
    let mut host = fx.host(true);

    let row = relay_commit(&fx, &mut host, &record, staged_payload(&worktree));

    let reason = reason_of(&row, LedgerOutcome::Refused);
    assert!(reason.contains("fake signer refused"), "{reason}");
    assert!(reason.contains("held for the operator"), "{reason}");
    assert_eq!(host.holds.len(), 1, "{:?}", host.holds);
    assert_eq!(host.holds[0].0, STAGE);
    assert_eq!(
        load_stage(STAGE, &fx.work_dir).unwrap().status,
        status,
        "a merge signing failure never blocks the stage"
    );
    assert_eq!(tip(&worktree, BRANCH), before);
}

#[test]
fn a_merge_signing_failure_holds_for_the_operator() {
    for status in [StageStatus::MergeConflict, StageStatus::MergeBlocked] {
        merge_signing_failure_from(status);
    }
}

#[test]
fn a_knowledge_commit_lands_on_the_target_and_is_attested_when_guarded() {
    let fx = fixture();
    repository(&fx);
    let record = knowledge_session(&fx);
    std::fs::write(fx.work_dir.join(REFS_FILE), "ref refs/heads/main\n").unwrap();
    let head = tip(&fx.repo_root, "HEAD");
    add_file(&fx.repo_root, "doc/loom/knowledge/notes.md");

    let row = relay_commit(
        &fx,
        &mut fx.host(true),
        &record,
        staged_payload(&fx.repo_root),
    );

    let note = reason_of(&row, LedgerOutcome::Applied);
    let id = tip(&fx.repo_root, "refs/heads/main");
    assert_eq!(note, format!("committed {id}"));
    let ledger = std::fs::read_to_string(fx.work_dir.join(LEDGER_FILE)).unwrap();
    let attested = format!("attest {head} {id} refs/heads/main");
    assert!(ledger.contains(&attested), "{ledger}");
}

#[test]
fn a_knowledge_commit_outside_the_prefix_is_refused() {
    let fx = fixture();
    repository(&fx);
    let record = knowledge_session(&fx);
    let before = tip(&fx.repo_root, "refs/heads/main");
    add_file(&fx.repo_root, "src/a.rs");

    let row = relay_commit(
        &fx,
        &mut fx.host(true),
        &record,
        staged_payload(&fx.repo_root),
    );

    reason_of(&row, LedgerOutcome::Refused);
    assert_eq!(tip(&fx.repo_root, "refs/heads/main"), before);
}

#[test]
fn a_merge_session_commit_records_two_parents() {
    let fx = fixture();
    let worktree = merging_repository(&fx);
    let record = merge_session(&fx);
    stage_in(&fx, StageStatus::MergeConflict, None);
    let stage_head = tip(&worktree, "HEAD");
    let main_head = tip(&fx.repo_root, "refs/heads/main");

    let row = relay_commit(&fx, &mut fx.host(true), &record, staged_payload(&worktree));

    reason_of(&row, LedgerOutcome::Applied);
    let line = git_in(&worktree, &["rev-list", "--parents", "-n", "1", BRANCH]);
    let parents: Vec<&str> = line.split_whitespace().skip(1).collect();
    assert_eq!(parents, [stage_head.as_str(), main_head.as_str()]);
}

#[test]
fn a_merge_head_off_the_target_is_refused_with_the_ref_unmoved() {
    let fx = fixture();
    let worktree = merging_repository(&fx);
    let record = merge_session(&fx);
    stage_in(&fx, StageStatus::MergeConflict, None);
    let before = tip(&worktree, BRANCH);
    // A commit off `main` whose tree holds a gitlink, named in MERGE_HEAD and
    // staged: the path checks alone pass the gitlink as MERGE_HEAD's own.
    let head = tip(&worktree, "HEAD");
    let gitlink = format!("160000,{head},sub");
    git_in(
        &worktree,
        &["update-index", "--add", "--cacheinfo", &gitlink],
    );
    let tree = git_in(&worktree, &["write-tree"]);
    let forged = git_in(
        &worktree,
        &["commit-tree", &tree, "-p", &head, "-m", "forged"],
    );
    let merge_head = git_in(
        &worktree,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "MERGE_HEAD",
        ],
    );
    std::fs::write(merge_head, format!("{forged}\n")).unwrap();

    let row = relay_commit(&fx, &mut fx.host(true), &record, staged_payload(&worktree));

    let reason = reason_of(&row, LedgerOutcome::Refused);
    assert!(reason.contains("is not on main"), "{reason}");
    assert_eq!(tip(&worktree, BRANCH), before);
}

#[test]
fn a_replayed_commit_entry_is_applied_once() {
    let fx = fixture();
    let worktree = repository(&fx);
    let record = stage_session(&fx);
    add_file(&worktree, "a.txt");
    let entry = fx.relay(&record, RequestKind::Commit, staged_payload(&worktree));
    let mut host = fx.host(true);
    let first = drain_and_read(&fx, &mut host, &record.id, &entry.id);
    let committed = tip(&worktree, BRANCH);

    fx.plant(&record.id, &format!("{}.json", entry.id), &entry.encode());
    run_pass(&mut host, &fx.tick(Utc::now()));

    reason_of(&first, LedgerOutcome::Applied);
    let settled = fx
        .ledger(&record.id)
        .iter()
        .filter(|row| row.id == entry.id && row.outcome.is_some())
        .count();
    assert_eq!(settled, 1);
    assert_eq!(tip(&worktree, BRANCH), committed);
}
