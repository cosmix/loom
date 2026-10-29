//! Contracts for the planning server's run handoff and pressure runs: the doc-change
//! split and commit lane, workspace state, the dashboard URL, abandon and
//! finish-abandon, the run strip, the Codex configuration guard, pressure start
//! validation and Codex recovery.

#[path = "plan_handoff_contracts/drive.rs"]
mod drive;
#[path = "plan_handoff_contracts/support.rs"]
mod support;

use std::collections::BTreeSet;
use std::fs;

use drive::*;
use loom::fs::work_dir::WorkDir;
use loom::plan_server::pressure::config_guard::{changed, snapshot};
use loom::plan_server::pressure::options::validate_start;
use loom::plan_server::pressure::recover::{codex_action, CodexAction};
use loom::plan_server::run::abandon::finish_abandon;
use loom::plan_server::run::changes::split_changes;
use loom::plan_server::run::dashboard::parse_dashboard_startup;
use loom::plan_server::run::state::workspace_state;
use loom::plan_server::run::strip::build_strip;
use loom::plan_server::wire::run::{RunOffer as O, WorkspaceState as W};
use loom::planning::project::Project;
use loom::process::{process_start_time, ProcessIdentity};
use serial_test::serial;
use support::*;

#[test]
#[serial]
fn changes_split_blocks_only_tracked_outside_doc() {
    let repo = Repo::committed(&[("src.txt", "one\n"), ("doc/old.md", "# Old\n")]);
    repo.put_all(&[
        ("src.txt", "two\n"),
        ("notes.txt", "untracked\n"),
        ("doc/old.md", "# Old, edited\n"),
        ("doc/new.md", "# New\n"),
    ]);
    let mut expected = BTreeSet::from(["doc/old.md".to_string(), "doc/new.md".to_string()]);
    for i in 0..300 {
        let rel = format!("doc/many/f{i:03}.md");
        repo.put(&rel, "# Many\n");
        expected.insert(rel);
    }
    let (doc, blocking) = split_changes(&repo.root).expect("split the changes");
    assert_eq!(blocking.len(), 1, "{blocking:?}");
    assert_eq!(paths(&blocking), BTreeSet::from(["src.txt".to_string()]));
    assert_eq!(doc.len(), 302, "every doc change, none cut off");
    assert_eq!(paths(&doc), expected);
}

#[test]
#[serial]
fn commit_refuses_paths_outside_current_doc_changes() {
    let repo = Repo::committed(&[("src.txt", "one\n"), ("doc/a.md", "# A\n")]);
    repo.put_all(&[("src.txt", "two\n"), ("doc/a.md", "# A, edited\n")]);
    let before = repo.head();
    let refused: [&[&str]; 4] = [
        &["src.txt"],
        &["../x"],
        &["doc/missing.md"],
        &["doc/a.md", "src.txt"],
    ];
    for bad in refused {
        let result = commit(&repo, bad, "Edit docs");
        assert!(result.is_err(), "{bad:?} must be refused, got {result:?}");
        assert_eq!(repo.head(), before, "{bad:?} moved HEAD");
        let staged = repo.staged();
        assert!(staged.is_empty(), "{bad:?} staged {staged:?}");
    }
}

#[test]
#[serial]
fn commit_runs_the_users_pre_commit_hook() {
    let repo = Repo::new();
    repo.put("doc/a.md", "# A\n");
    let marker = repo.temp.path().join("hook-ran");
    let script = format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.display());
    repo.hook("pre-commit", &script);
    let before = repo.head();
    let code = commit(&repo, &["doc/a.md"], "Add a");
    let code = code.expect("a failing hook is an exit code, not an error");
    assert_ne!(code, 0, "the commit passed a failing pre-commit hook");
    assert!(marker.exists(), "the user's pre-commit hook never ran");
    assert_eq!(repo.head(), before);
}

#[test]
#[serial]
fn workspace_state_follows_the_plan_prefix() {
    let repo = Repo::new();
    let (state, offer, plan) = workspace_state(&repo.root).expect("no workspace");
    assert_eq!((state, offer), (W::Absent, O::Start));
    assert_eq!(plan, None);
    let cases = [
        ("doc/plans/PLAN-a.md", W::Initialized, O::StartInitialized),
        ("doc/plans/IN_PROGRESS-PLAN-a.md", W::Stopped, O::Resume),
        ("doc/plans/DONE-PLAN-a.md", W::Done, O::StartNext),
    ];
    for (source, want_state, want_offer) in cases {
        repo.put(source, "# A\n");
        repo.workspace(source);
        let (state, offer, plan) = workspace_state(&repo.root).expect(source);
        assert_eq!((state, offer), (want_state, want_offer), "{source}");
        let plan = plan.unwrap_or_else(|| panic!("{source}: no active plan"));
        assert!(plan.ends_with(source), "{source}: {plan:?}");
        fs::remove_file(repo.root.join(source)).unwrap();
    }
}

#[test]
fn dashboard_url_parsed_from_print_startup() {
    let url = |lines: &[&str]| parse_dashboard_startup(lines);
    let loopback = "loom dashboard: http://127.0.0.1:7373/  (Ctrl-C to stop)";
    assert_eq!(url(&[loopback]).as_deref(), Some("http://127.0.0.1:7373/"));
    let terminals = "loom dashboard: http://127.0.0.1:7373/?token=abc123  \
                     (terminals enabled; Ctrl-C to stop)";
    let tokenized = Some("http://127.0.0.1:7373/?token=abc123");
    assert_eq!(url(&[terminals]).as_deref(), tokenized);
    assert_eq!(url(&REMOTE_STARTUP).as_deref(), tokenized);
    assert_eq!(url(&[]), None);
}

#[test]
#[serial]
fn abandon_declined_unmerged_runs_nothing() {
    let repo = Repo::in_progress_run();
    repo.branch_ahead("loom/s1", 1);
    let fake = FakeLoom::new(&repo);
    let now = branches_changed(abandon(&repo, &fake));
    assert_eq!(now, vec![("loom/s1".to_string(), 1)]);
    let calls = fake.invocations();
    assert!(calls.is_empty(), "loom ran: {calls:?}");
    assert!(repo.has(PLAN), "the plan lost its name");
}

#[test]
#[serial]
fn abandon_refuses_when_branches_change_during_stop() {
    let repo = Repo::in_progress_run();
    let fake = FakeLoom::new(&repo);
    fake.on_stop(
        "cd \"$ROOT\" || exit 1\n\
         c=$(git commit-tree -p HEAD -m stage 'HEAD^{tree}') || exit 1\n\
         git update-ref refs/heads/loom/s2 \"$c\"\n",
    );
    let now = branches_changed(abandon(&repo, &fake));
    let named = now.iter().any(|(b, ahead)| b == "loom/s2" && *ahead >= 1);
    assert!(named, "loom/s2 is not in {now:?}");
    let calls = fake.invocations();
    assert!(fake.ran("stop"), "loom stop never ran: {calls:?}");
    assert!(!fake.ran("clean"), "loom clean ran: {calls:?}");
    assert!(repo.has_branch("loom/s2"), "the stage's new commit is gone");
    assert!(repo.has(PLAN), "the plan lost its name");
}

#[test]
#[serial]
fn abandon_keeps_the_name_when_clean_leaves_state() {
    let repo = Repo::in_progress_run();
    let fake = FakeLoom::new(&repo);
    assert_refused(&abandon(&repo, &fake), "clean left .loom/work");
    let calls = fake.invocations();
    assert!(fake.ran("clean"), "loom clean never ran: {calls:?}");
    assert!(repo.has(PLAN), "the plan lost its name");
    let archived = repo.archive_entries();
    assert!(archived.is_empty(), "{archived:?}");
}

#[test]
#[serial]
fn abandon_renames_into_archive_after_clean() {
    let repo = Repo::in_progress_run();
    let fake = FakeLoom::new(&repo);
    fake.clean_deletes_work();
    let to = renamed(abandon(&repo, &fake));
    assert!(to.ends_with(RETIRED), "{to:?}");
    assert!(repo.has(RETIRED), "no retired plan in the archive");
    assert!(!repo.root.join(PLAN).exists(), "the running plan remains");
    assert_eq!(fake.clean_env().as_deref(), Some("LOOM_CLEAN_YES=1"));
}

#[test]
#[serial]
fn abandon_refuses_a_symlinked_archive() {
    let repo = Repo::committed(&[(PLAN, "# A\n")]);
    let outside = tempfile::tempdir().expect("link target");
    let archive = repo.root.join("doc/plans/archive");
    std::os::unix::fs::symlink(outside.path(), archive).unwrap();
    let runtime = repo.runtime();
    repo.write_marker(&runtime);
    let outcome = finish_abandon(&repo.root, &runtime).expect("finish abandon");
    assert_refused(&outcome, "symlinked archive");
    assert!(repo.has(PLAN), "the plan lost its name");
    let escaped: Vec<_> = fs::read_dir(outside.path()).unwrap().collect();
    assert!(
        escaped.is_empty(),
        "the rename followed the link: {escaped:?}"
    );
}

#[test]
#[serial]
fn finish_abandon_renames_a_stranded_plan() {
    let repo = Repo::committed(&[(PLAN, "# A\n")]);
    let runtime = repo.runtime();
    let outcome = finish_abandon(&repo.root, &runtime).expect("without a marker");
    assert_refused(&outcome, "no abandon marker");
    assert!(repo.has(PLAN));

    repo.write_marker(&runtime);
    repo.git(&["branch", "loom/s1"]);
    let outcome = finish_abandon(&repo.root, &runtime).expect("with a branch left");
    assert_refused(&outcome, "loom/s1 left");
    assert!(repo.has(PLAN));

    repo.git(&["branch", "-D", "loom/s1"]);
    let other_state = tempfile::tempdir().expect("another state directory");
    let outcome = {
        let _env = EnvGuard::set("LOOM_PLAN_STATE", other_state.path());
        finish_abandon(&repo.root, &runtime).expect("stranded with the marker")
    };
    let to = renamed(outcome);
    assert!(to.ends_with(RETIRED), "{to:?}");
    assert!(repo.has(RETIRED));
    let marker = repo.marker_path(&runtime);
    assert!(!marker.exists(), "the marker outlived the rename");
}

#[test]
#[serial]
fn commit_commits_only_ticked_paths() {
    let repo = Repo::committed(&[("src.txt", "1\n"), ("doc/x.md", "1\n"), ("doc/y.md", "1\n")]);
    repo.put("src.txt", "2\n");
    repo.git(&["add", "src.txt"]);
    repo.put_all(&[("doc/x.md", "2\n"), ("doc/y.md", "2\n")]);
    let code = commit(&repo, &["doc/x.md"], "Edit x").expect("commit doc/x.md");
    assert_eq!(code, 0);
    assert_eq!(repo.head_files(), vec!["doc/x.md"]);
    assert_eq!(
        repo.staged(),
        vec!["src.txt"],
        "the staged outside file moved"
    );
    let status = repo.status();
    assert!(status.contains(" M doc/y.md"), "{status}");

    repo.put_all(&[("doc/a*", "# Glob\n"), ("doc/ab.md", "# Ab\n")]);
    let code = commit(&repo, &["doc/a*"], "Add glob").expect("commit doc/a*");
    assert_eq!(code, 0);
    assert_eq!(repo.head_files(), vec!["doc/a*"]);
    let status = repo.status();
    assert!(status.contains("?? doc/ab.md"), "{status}");
    assert_eq!(repo.staged(), vec!["src.txt"]);
}

#[test]
#[serial]
fn run_strip_carries_no_token() {
    let repo = Repo::in_progress_run();
    let work_dir = WorkDir::new(&repo.root).expect("work dir");
    let url = Some("http://127.0.0.1:7373/?token=abc");
    let strip = build_strip(&work_dir, url).expect("build the strip");
    let tokenless = Some("http://127.0.0.1:7373/");
    assert_eq!(strip.dashboard_url.as_deref(), tokenless);
    let json = serde_json::to_string(&strip).expect("serialize the strip");
    assert!(!json.contains("token="), "{json}");
}

#[test]
#[serial]
fn codex_config_edit_is_detected() {
    let repo = Repo::committed(&[
        (".claude/settings.json", "{}\n"),
        (".gitignore", ".claude/settings.local.json\n"),
        ("doc/a.md", "# A\n"),
    ]);
    let before = snapshot(&repo.root).expect("snapshot before");
    repo.put(".claude/settings.local.json", "{\"permissions\":{}}\n");
    repo.put(".claude/agents/x.md", "# Agent\n");
    let after = snapshot(&repo.root).expect("snapshot after");
    let expected = vec![".claude/agents/x.md", ".claude/settings.local.json"];
    assert_eq!(changed(&before, &after), expected);
    repo.put("doc/a.md", "# A, edited\n");
    let later = snapshot(&repo.root).expect("snapshot after a doc edit");
    assert_eq!(changed(&after, &later), Vec::<String>::new());
}

#[test]
#[serial]
fn pressure_start_refuses_a_locked_plan() {
    let repo = Repo::committed(&[
        ("doc/plans/PLAN-a.md", "# A\n"),
        ("doc/plans/IN_PROGRESS-PLAN-b.md", "# B\n"),
        ("doc/product/index.md", "# Vision\n"),
    ]);
    let project = Project::load(&repo.root).expect("load the project");
    for plan in ["doc/plans/IN_PROGRESS-PLAN-b.md", "doc/product/index.md"] {
        let refusal = validate_start(&project, &start(plan));
        assert_eq!(refusal, Err("plan_not_written".to_string()), "{plan}");
    }
    let written = validate_start(&project, &start("doc/plans/PLAN-a.md"));
    assert_eq!(written, Ok(()));
}

#[test]
#[serial]
fn codex_terminated_only_when_identity_verifies() {
    assert!(matches!(codex_action(None), CodexAction::Nothing));

    let mut dead = sleeper();
    let (pid, start_time) = (dead.id(), process_start_time(dead.id()));
    assert!(start_time.is_some(), "no start time for a live child");
    dead.kill().unwrap();
    dead.wait().unwrap();
    let action = codex_action(Some(ProcessIdentity { pid, start_time }));
    assert!(matches!(action, CodexAction::Nothing), "dead: {action:?}");

    let mut live = sleeper();
    let (pid, start_time) = (live.id(), process_start_time(live.id()));
    assert!(start_time.is_some(), "no start time for a live child");
    let verified = codex_action(Some(ProcessIdentity { pid, start_time }));
    let bare = ProcessIdentity {
        pid,
        start_time: None,
    };
    let unverified = codex_action(Some(bare));
    live.kill().ok();
    live.wait().ok();
    assert!(matches!(verified, CodexAction::Terminate), "{verified:?}");
    assert!(
        matches!(unverified, CodexAction::LeaveAlone),
        "{unverified:?}"
    );
}
