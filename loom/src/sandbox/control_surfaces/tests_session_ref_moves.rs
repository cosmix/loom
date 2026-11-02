//! What a session's own git does under the capsule's reflog deny. Each command
//! either finishes or refuses before touching anything: a command that moved the
//! index and the work tree and then lost the ref update leaves a tree a later
//! `git add` and `loom stage commit` would commit, reversing the stage's work.

use super::tests_ref_store_denies::{git, repo, skip, Repo};
use super::*;
use crate::git::stage_commit::{CommitRequest, CommitScope, Committer};
use crate::git::worktree::WorktreeGit;

impl Repo {
    /// `script` as a session runs it: the denies, and the wrapper's git setting.
    fn session(&self, script: &str) -> std::process::Output {
        let (name, value) = WorktreeGit::SESSION_GIT_PARAMETERS;
        let output = self
            .confined_command(script)
            .env(name, value)
            .output()
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&output.stderr).starts_with("bwrap:"),
            "{output:?}"
        );
        output
    }

    /// Two commits on the stage branch.
    fn with_stage_work(self) -> Self {
        for name in ["one", "two"] {
            std::fs::write(self.worktree.join(name), "work\n").unwrap();
            git(&self.worktree, &["add", "--", name]);
            git(&self.worktree, &["commit", "-qm", name]);
        }
        self
    }

    fn branch_tip(&self) -> String {
        rev(&self.repo, "loom/s1")
    }

    fn branch_log(&self) -> PathBuf {
        self.common.join("logs/refs/heads/loom/s1")
    }

    fn head_log_lines(&self) -> usize {
        let log = self.common.join("worktrees/s1/logs/HEAD");
        std::fs::read_to_string(log).unwrap().lines().count()
    }

    /// `git status --porcelain` of the stage worktree, tracked changes only.
    fn tracked_changes(&self) -> String {
        let args = ["status", "--porcelain", "--untracked-files=no"];
        let output = git(&self.worktree, &args);
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }
}

fn rev(dir: &Path, revision: &str) -> String {
    let output = git(dir, &["rev-parse", revision]);
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The reported half-apply: the index and work tree moved, the ref did not, and
/// the staged result reverses the stage's work. Each form must finish whole.
#[test]
fn a_session_reset_to_a_commit_moves_the_branch_with_the_tree() {
    if skip("a_session_reset_to_a_commit_moves_the_branch_with_the_tree") {
        return;
    }
    for (script, kept) in [
        ("git reset -q HEAD~1", true),
        ("git reset -q --hard HEAD~1", false),
        ("git reset -q --hard HEAD@{1}", false),
    ] {
        let repo = repo().with_stage_work();
        let earlier = rev(&repo.worktree, "HEAD~1");

        let output = repo.session(script);

        assert!(output.status.success(), "{script}: {output:?}");
        assert_eq!(repo.branch_tip(), earlier, "{script}");
        assert_eq!(repo.tracked_changes(), "", "{script}: a half-applied reset");
        assert_eq!(repo.worktree.join("two").exists(), kept, "{script}");
    }
}

/// What a session or a merge resolver runs on its branch: every one finishes,
/// and none leaves a staged difference from the branch it moved.
#[test]
fn a_sessions_branch_moving_commands_finish_on_the_stage_branch() {
    if skip("a_sessions_branch_moving_commands_finish_on_the_stage_branch") {
        return;
    }
    let identity = "git -c user.name=T -c user.email=t@example.invalid -c commit.gpgsign=false";
    let rows: [(&str, String); 7] = [
        ("bare merge", format!("{identity} merge --no-edit main")),
        (
            "merge finished by hand",
            format!("{identity} merge --no-commit --no-ff main && {identity} commit -q --no-edit"),
        ),
        (
            "merge --continue",
            format!(
                "{identity} merge --no-commit --no-ff main && GIT_EDITOR=true {identity} merge --continue"
            ),
        ),
        (
            "commit",
            format!("echo x >x.txt; git add x.txt; {identity} commit -qm x"),
        ),
        ("amend", format!("{identity} commit -q --amend -m amended")),
        ("cherry-pick", format!("{identity} cherry-pick main")),
        ("rebase", format!("{identity} rebase -q main")),
    ];
    for (name, script) in rows {
        let repo = repo().with_stage_work();
        let before = repo.branch_tip();

        let output = repo.session(&script);

        assert!(output.status.success(), "{name}: {output:?}");
        assert_ne!(repo.branch_tip(), before, "{name}: the branch stayed");
        assert_eq!(repo.tracked_changes(), "", "{name}: left staged changes");
        assert!(!repo.branch_log().exists(), "{name}: wrote a branch reflog");
    }
}

/// Commands that make other refs, under the same setting.
#[test]
fn a_session_can_make_and_delete_its_own_branches_and_tags() {
    if skip("a_session_can_make_and_delete_its_own_branches_and_tags") {
        return;
    }
    let repo = repo();

    let output = repo.session(
        "git branch another && git branch -f another HEAD && git tag t1 && \
         git checkout -q -b other && git checkout -q loom/s1 && git branch -D another other",
    );

    assert!(output.status.success(), "{output:?}");
    assert!(!repo.common.join("logs/refs/heads/another").exists());
}

/// `git stash` forces a reflog of its own, so it cannot work here. It must stop
/// before it cleans the tree, which is what makes the refusal harmless.
#[test]
fn a_session_stash_refuses_before_it_touches_the_tree() {
    if skip("a_session_stash_refuses_before_it_touches_the_tree") {
        return;
    }
    for script in ["git stash push -m s", "git stash push -u -m s"] {
        let repo = repo();
        std::fs::write(repo.worktree.join("a.txt"), "edited\n").unwrap();
        std::fs::write(repo.worktree.join("untracked"), "kept\n").unwrap();

        let output = repo.session(script);

        assert!(!output.status.success(), "{script}: {output:?}");
        let edited = std::fs::read_to_string(repo.worktree.join("a.txt")).unwrap();
        assert_eq!(edited, "edited\n", "{script}: the stash took the edit");
        assert!(repo.worktree.join("untracked").exists(), "{script}");
    }
}

/// The daemon's own commit writes no branch log, so the session's next reset
/// still works, and `HEAD`'s log, the worktree's undo history, keeps its entry.
#[test]
fn a_pinned_commit_leaves_the_stage_branch_without_a_reflog() {
    if skip("a_pinned_commit_leaves_the_stage_branch_without_a_reflog") {
        return;
    }
    let repo = repo().with_stage_work();
    repo.denies();
    assert!(
        !repo.branch_log().exists(),
        "a capsule leaves the log there"
    );
    let lines = repo.head_log_lines();
    let pinned = WorktreeGit::pinned(&repo.repo, &repo.worktree).unwrap();
    let expected_head = rev(&repo.worktree, "HEAD");
    std::fs::write(repo.worktree.join("three"), "work\n").unwrap();
    git(&repo.worktree, &["add", "--", "three"]);
    let tree = git(&repo.worktree, &["write-tree"]);
    let request = CommitRequest {
        message: "three".to_string(),
        expected_head: expected_head.clone(),
        expected_tree: String::from_utf8_lossy(&tree.stdout).trim().to_string(),
    };
    let scope = CommitScope::StageBranch {
        stage_id: "s1".to_string(),
    };

    let commit = Committer::new(&pinned, &repo.repo)
        .commit_staged(&scope, &request)
        .unwrap();

    assert_eq!(repo.branch_tip(), commit);
    assert!(!repo.branch_log().exists(), "the daemon wrote a branch log");
    assert_eq!(
        repo.head_log_lines(),
        lines + 1,
        "HEAD's log lost the entry"
    );
    let output = repo.session("git reset -q --hard HEAD~1");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(repo.branch_tip(), expected_head);
}

/// The setting is scoped to stage worktrees: a commit in the operator's checkout
/// keeps both of its logs.
#[test]
fn a_pinned_commit_in_the_checkout_still_writes_the_operators_reflogs() {
    let repo = repo();
    let branch_log = repo.common.join("logs/refs/heads/main");
    let head_log = repo.common.join("logs/HEAD");
    let lines = |path: &Path| std::fs::read_to_string(path).unwrap().lines().count();
    let (branch_before, head_before) = (lines(&branch_log), lines(&head_log));
    let pinned = WorktreeGit::pinned_main(&repo.repo).unwrap();

    pinned
        .run_checked(&["commit", "-q", "--allow-empty", "-m", "operator"])
        .unwrap();

    assert_eq!(lines(&branch_log), branch_before + 1);
    assert_eq!(lines(&head_log), head_before + 1);
}

/// Spawning removes the session's own branch log and no other, and removes a link
/// there without touching its target.
#[test]
fn a_capsule_removes_only_its_own_branch_reflog() {
    let repo = repo();
    git(&repo.repo, &["branch", "loom/s2"]);
    let sibling = repo.common.join("logs/refs/heads/loom/s2");
    let main = repo.common.join("logs/refs/heads/main");
    assert!(repo.branch_log().is_file() && sibling.is_file());

    session_denies(&DenyInputs {
        repo_root: &repo.repo,
        state_root: &repo.repo.join(".loom").join("work"),
        worktree: None,
        executable_dirs: &[],
        plugin_entries: None,
        writable_roots: &[],
    })
    .unwrap();
    assert!(repo.branch_log().is_file(), "a checkout session removed it");

    repo.denies();
    assert!(!repo.branch_log().exists());
    assert!(
        sibling.is_file() && main.is_file(),
        "another branch lost its log"
    );

    let victim = repo.outside.join("victim");
    std::fs::write(&victim, "orig\n").unwrap();
    std::os::unix::fs::symlink(&victim, repo.branch_log()).unwrap();
    repo.denies();
    assert!(std::fs::symlink_metadata(repo.branch_log()).is_err());
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "orig\n");
}

/// A session that turns the setting back on hurts only itself, loudly: the ref
/// stays and no log appears, so nothing the daemon would follow was planted.
#[test]
fn a_session_overriding_the_setting_is_refused_and_plants_nothing() {
    if skip("a_session_overriding_the_setting_is_refused_and_plants_nothing") {
        return;
    }
    let repo = repo().with_stage_work();
    let before = repo.branch_tip();
    let script = "git -c user.name=T -c user.email=t@example.invalid -c commit.gpgsign=false \
                  -c core.logAllRefUpdates=always commit -q --allow-empty -m x";

    let output = repo.session(script);

    assert!(!output.status.success(), "{output:?}");
    assert_eq!(repo.branch_tip(), before);
    assert!(std::fs::symlink_metadata(repo.branch_log()).is_err());
}

/// What a session does need, under the same denies, on its branch: staging, a
/// merge left for `loom stage commit`, giving that merge up, and unstaging.
/// `git merge --abort` and `git reset` are why `refs/` is not denied: both lock
/// the branch even when it does not move.
#[test]
fn staging_merging_and_resetting_still_work_on_the_branch_under_the_denies() {
    if skip("staging_merging_and_resetting_still_work_on_the_branch_under_the_denies") {
        return;
    }
    let repo = repo();
    let identity = "git -c user.name=T -c user.email=t@example.invalid";
    let script = format!(
        "{identity} merge --no-commit --no-ff --no-edit main >/dev/null && echo merged; \
         git merge --abort && echo aborted; \
         echo new >new.txt; git add new.txt && git write-tree >/dev/null && echo staged; \
         git reset -q && echo unstaged; git reset -q --hard && echo hard; \
         git add new.txt && echo staged-again"
    );

    let output = repo.confined(&script);

    let stdout = String::from_utf8_lossy(&output.stdout);
    for step in [
        "merged",
        "aborted",
        "staged",
        "unstaged",
        "hard",
        "staged-again",
    ] {
        assert!(stdout.contains(step), "{step}: {output:?}");
    }
    let staged = git(&repo.worktree, &["diff", "--cached", "--name-only"]);
    assert_eq!(String::from_utf8_lossy(&staged.stdout).trim(), "new.txt");
}

/// Without the wrapper's setting (an `env -i`, a tool that scrubs the variable)
/// the deny still holds: these refuse and say so, with the branch where the daemon
/// put it, and the planted-reflog path stays closed.
#[test]
fn a_session_moving_a_branch_itself_fails_loudly() {
    if skip("a_session_moving_a_branch_itself_fails_loudly") {
        return;
    }
    let repo = repo();
    std::fs::write(repo.worktree.join("a.txt"), "edited\n").unwrap();
    let before = git(&repo.repo, &["rev-parse", "loom/s1"]).stdout;

    for command in ["git stash push -m s", "git branch another"] {
        let output = repo.confined(command);
        assert!(!output.status.success(), "{command}: {output:?}");
    }

    assert_eq!(git(&repo.repo, &["rev-parse", "loom/s1"]).stdout, before);
    let edited = std::fs::read_to_string(repo.worktree.join("a.txt")).unwrap();
    assert_eq!(edited, "edited\n", "a refused stash lost the work");
}
