//! What the `reference-transaction` hook refuses a loom session, under real
//! git: each way a move of the target can fail the knowledge-only test, and
//! the replace refs and grafts a session can plant to make a move look like
//! one it may make.
//!
//! A session's ledger is unwritable (a directory stands in for the sandbox's
//! write denial), so the hook has no line to write and judges the move.

#[path = "target_ref_hook_support/mod.rs"]
#[allow(dead_code)]
mod support;
#[path = "test_support/mod.rs"]
mod test_support;
use std::process::Output;
use support::{commit, commit_paths, git, make_ledger_unwritable, repo, run, stderr, tip, Repo};

const KNOWLEDGE: &str = "doc/loom/knowledge/";

/// A commit of `paths` on top of `base`, made detached in the linked worktree
/// so that no branch moves.
fn detached_commit(repo: &Repo, base: &str, paths: &[&str]) -> String {
    git(&repo.wt, &["switch", "--detach", base]);
    commit_paths(&repo.wt, paths)
}

/// The paths that differ between commits `a` and `b`, as git lists them.
fn changed_paths(repo: &Repo, a: &str, b: &str) -> Vec<String> {
    let out = git(&repo.root, &["diff", "--name-only", a, b]);
    out.lines().map(str::to_string).collect()
}

/// `git merge-base --is-ancestor <ancestor> <descendant>`, outside any session.
fn is_ancestor(repo: &Repo, ancestor: &str, descendant: &str) -> Output {
    let args = ["merge-base", "--is-ancestor", ancestor, descendant];
    run(&repo.root, &args, None)
}

/// Run `git <args>` as a session in the linked worktree: the hook must refuse
/// it, git must fail, and `main` must stay where it was.
fn assert_session_refused(repo: &Repo, args: &[&str]) {
    make_ledger_unwritable(&repo.work);
    let before = tip(&repo.root);
    let out = run(&repo.wt, args, Some("k1"));
    assert!(!out.status.success(), "the hook let a session run {args:?}");
    let refusal = "loom: refusing to move refs/heads/main from loom session k1";
    assert!(stderr(&out).contains(refusal), "{}", stderr(&out));
    assert_eq!(tip(&repo.root), before);
}

/// `main` and a detached commit that each add one knowledge file on the
/// initial commit: their trees differ only under the knowledge prefix, but
/// the commit does not descend from `main`. Returns `(main, commit)`.
fn diverged_knowledge_commit(repo: &Repo) -> (String, String) {
    let target = commit(&repo.root, "doc/loom/knowledge/y.md");
    let moved = detached_commit(repo, &repo.main, &["doc/loom/knowledge/x.md"]);
    let paths = changed_paths(repo, &target, &moved);
    assert!(
        paths.iter().all(|path| path.starts_with(KNOWLEDGE)),
        "{paths:?}"
    );
    (target, moved)
}

#[test]
fn session_move_that_is_not_a_fast_forward_is_refused() {
    let repo = repo();
    let (target, moved) = diverged_knowledge_commit(&repo);
    assert!(!is_ancestor(&repo, &target, &moved).status.success());

    assert_session_refused(&repo, &["update-ref", "refs/heads/main", moved.as_str()]);
}

#[test]
fn session_commit_touching_knowledge_and_source_is_refused() {
    let repo = repo();
    let paths = ["doc/loom/knowledge/x.md", "src/evil.rs"];
    let mixed = detached_commit(&repo, &repo.main, &paths);
    // The knowledge path sorts first: checking only it would let this through.
    assert_eq!(changed_paths(&repo, &repo.main, &mixed), paths);

    assert_session_refused(&repo, &["update-ref", "refs/heads/main", mixed.as_str()]);
}

#[test]
fn session_deleting_the_target_is_refused() {
    let repo = repo();

    assert_session_refused(&repo, &["update-ref", "-d", "refs/heads/main"]);
}

#[test]
fn session_move_is_judged_on_the_real_commit_not_a_replacement() {
    let repo = repo();
    let evil = detached_commit(&repo, &repo.main, &["src/evil.rs"]);
    let decoy = detached_commit(&repo, &repo.main, &["doc/loom/knowledge/decoy.md"]);
    git(&repo.root, &["replace", &evil, &decoy]);
    // Git that honours replace refs reads `evil` as the decoy.
    let seen = changed_paths(&repo, &repo.main, &evil);
    assert_eq!(
        seen,
        ["doc/loom/knowledge/decoy.md"],
        "the replace ref has no effect"
    );

    assert_session_refused(&repo, &["update-ref", "refs/heads/main", evil.as_str()]);
}

#[test]
fn session_move_is_judged_on_the_real_parents_not_a_graft() {
    let repo = repo();
    let (target, moved) = diverged_knowledge_commit(&repo);
    let grafts = repo.root.join(".git/info/grafts");
    std::fs::write(&grafts, format!("{moved} {target}\n")).unwrap();
    // Git that honours grafts reads `moved` as a child of `main`.
    let grafted = is_ancestor(&repo, &target, &moved);
    assert!(
        grafted.status.success(),
        "the graft has no effect: {}",
        stderr(&grafted)
    );

    assert_session_refused(&repo, &["update-ref", "refs/heads/main", moved.as_str()]);
}
