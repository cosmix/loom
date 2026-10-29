//! A workspace that names no plan, and a detached HEAD, still give a usable run state.

use super::*;
use crate::plan_server::run::start::blocked_message;

#[test]
fn workspace_naming_no_plan_is_an_initialized_start_that_needs_clean() {
    let dir = repo();
    put(
        dir.path(),
        ".loom/work/config.toml",
        "[terminal]\nbackend = \"tmux\"\n",
    );
    ordered_plans(dir.path());
    assert_eq!(
        workspace_state(dir.path()).unwrap(),
        (W::Initialized, O::Start, None)
    );

    let response = get(&context(dir.path()));

    assert_eq!(response.status, 200);
    let view: RunStateView = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(view.workspace, W::Initialized);
    assert_eq!(view.offer, O::Start);
    assert_eq!(view.active_plan, None);
    assert!(view.clean_notice);
    assert_eq!(
        view.preselected.as_deref(),
        Some("doc/plans/feature/PLAN-z.md")
    );
    assert_eq!(view.abandon, None);
    assert_eq!(view.finish_abandon, None);
}

fn commit_empty(root: &Path) {
    git(
        root,
        &[
            "-c",
            "user.name=State Tests",
            "-c",
            "user.email=state@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            "initial",
        ],
    );
}

/// The state view of a workspace whose config names no plan but which holds `kept`.
fn nameless_view(kept: impl FnOnce(&Path)) -> RunStateView {
    let dir = repo();
    put(
        dir.path(),
        ".loom/work/config.toml",
        "[terminal]\nbackend = \"tmux\"\n",
    );
    ordered_plans(dir.path());
    kept(dir.path());
    state_view(&context(dir.path())).unwrap()
}

/// Starting must not offer `--clean`, which would delete the kept run state unconfirmed; the
/// reason travels as the single blocking change.
fn assert_clean_withheld(view: RunStateView, left: &str) {
    assert_eq!(view.workspace, W::Initialized);
    assert_eq!(view.offer, O::Start);
    assert_eq!(view.active_plan, None);
    assert!(!view.clean_notice);
    assert_eq!(
        view.preselected.as_deref(),
        Some("doc/plans/feature/PLAN-z.md")
    );
    assert_eq!(
        view.blocking_changes.len(),
        1,
        "{:?}",
        view.blocking_changes
    );
    let blocker = &view.blocking_changes[0];
    assert_eq!(blocker.path, ".loom/work");
    assert!(blocker.status.starts_with(left), "{}", blocker.status);
    assert!(
        blocker.status.contains("loom clean --all"),
        "{}",
        blocker.status
    );
}

#[test]
fn nameless_workspace_with_stage_files_withholds_the_clean_start() {
    let view = nameless_view(|root| {
        put(root, ".loom/work/stages/01-build.md", "# Stage\n");
    });

    assert_clean_withheld(view, "stage files remain");
}

#[test]
fn start_refusal_for_a_nameless_workspace_is_the_entrys_own_status() {
    let dir = repo();
    put(dir.path(), ".loom/work/stages/01-build.md", "# Stage\n");
    ordered_plans(dir.path());
    let view = state_view(&context(dir.path())).unwrap();

    let message = blocked_message(dir.path(), &view);

    assert_eq!(message, view.blocking_changes[0].status);
    assert!(message.contains("loom clean --all"), "{message}");
}

#[test]
fn nameless_workspace_with_session_files_withholds_the_clean_start() {
    let view = nameless_view(|root| {
        put(root, ".loom/work/sessions/session-1.md", "# Session\n");
    });

    assert_clean_withheld(view, "session files remain");
}

#[test]
fn nameless_workspace_with_a_worktree_withholds_the_clean_start() {
    let view = nameless_view(|root| fs::create_dir_all(root.join(".worktrees/build")).unwrap());

    assert_clean_withheld(view, "worktrees remain");
}

#[test]
fn nameless_workspace_with_a_loom_branch_withholds_the_clean_start() {
    let view = nameless_view(|root| {
        commit_empty(root);
        git(root, &["branch", "loom/build"]);
    });

    assert_clean_withheld(view, "loom/* branches remain");
}

#[test]
fn nameless_workspace_holding_only_a_config_and_other_branches_keeps_the_clean_start() {
    let view = nameless_view(|root| {
        commit_empty(root);
        git(root, &["branch", "feature/work"]);
    });

    assert!(view.clean_notice);
    assert_eq!(view.blocking_changes, Vec::new());
}

#[test]
fn clean_notice_stays_off_for_an_initialized_plan() {
    let dir = repo();
    workspace(dir.path(), "doc/plans/PLAN-a.md");

    let view = state_view(&context(dir.path())).unwrap();

    assert_eq!(view.active_plan.as_deref(), Some("doc/plans/PLAN-a.md"));
    assert!(!view.clean_notice);
}

#[test]
fn detached_head_names_head_as_the_branch() {
    let dir = repo();
    commit_empty(dir.path());
    git(dir.path(), &["checkout", "-q", "--detach"]);

    let response = get(&context(dir.path()));

    assert_eq!(response.status, 200);
    let view: RunStateView = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(view.branch, "HEAD");
}
