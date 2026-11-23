use super::*;
use crate::git::merge::checkout_files::displace_files;
use crate::git::merge::test_support::{init_repo, rev, stage_branch};
use crate::test_git::{commit_file, git};

fn overlap(paths: &[&str]) -> MergeBlock {
    MergeBlock::UncommittedOverlap {
        paths: paths.iter().map(|p| p.to_string()).collect(),
    }
}

#[test]
fn git_messages_that_list_paths_become_an_overlap() {
    let cases = [
        (
            "error: Your local changes to the following files would be overwritten by merge:\n\ta\n\tb c\nPlease commit your changes or stash them before you merge.\nAborting\n",
            overlap(&["a", "b c"]),
        ),
        (
            "error: The following untracked working tree files would be overwritten by merge:\n\tb\nPlease move or remove them before you merge.\nAborting\n",
            overlap(&["b"]),
        ),
        (
            "error: The following untracked working tree files would be removed by merge:\n\tt\nPlease move or remove them before you merge.\nAborting\n",
            overlap(&["t"]),
        ),
        (
            "error: Updating the following directories would lose untracked files in them:\n\td\n\nAborting\n",
            overlap(&["d"]),
        ),
    ];
    for (stderr, expected) in cases {
        assert_eq!(refused_block(stderr), expected, "{stderr}");
    }
}

#[test]
fn a_message_without_paths_becomes_a_one_line_detail() {
    let block =
        refused_block("hint: Diverging\nhint:\nfatal: Not possible to fast-forward, aborting.\n");
    assert_eq!(
        block,
        MergeBlock::FastForwardRefused {
            detail: "hint: Diverging hint: fatal: Not possible to fast-forward, aborting."
                .to_string()
        }
    );

    let MergeBlock::FastForwardRefused { detail } = refused_block(&"word ".repeat(200)) else {
        panic!("expected FastForwardRefused");
    };
    assert_eq!(detail.chars().count(), DETAIL_LIMIT);
}

#[test]
fn a_refusal_naming_paths_is_an_overlap_and_changes_nothing() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "branch")]);
    let (old, new) = (rev(root, "main"), rev(root, "loom/s1"));
    std::fs::write(root.join("b.txt"), "mine").unwrap();

    let advance = fast_forward(root, "main", (&old, &new), Displaced::default());

    assert_eq!(advance, Advance::Blocked(overlap(&["b.txt"])));
    assert_eq!(rev(root, "main"), old);
    assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "mine");
}

#[test]
fn a_refusal_after_the_target_moved_is_target_moved() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "branch")]);
    let (old, new) = (rev(root, "main"), rev(root, "loom/s1"));
    commit_file(root, "c.txt", "moved", "main moves");

    let advance = fast_forward(root, "main", (&old, &new), Displaced::default());

    assert_eq!(advance, Advance::Blocked(MergeBlock::TargetMoved));
}

/// The refusal comes before git touches anything. Under `WorktreeGit::pinned_main`
/// git would lock a private index, so it would move the target and update the
/// files, and only publishing the index afterwards would find the operator's lock.
#[test]
fn a_held_index_lock_refuses_and_moves_nothing() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "branch")]);
    let (old, new) = (rev(root, "main"), rev(root, "loom/s1"));
    std::fs::write(root.join(".git/index.lock"), "").unwrap();

    let advance = fast_forward(root, "main", (&old, &new), Displaced::default());

    assert_eq!(rev(root, "main"), old);
    assert!(!root.join("b.txt").exists());
    let Advance::Blocked(MergeBlock::FastForwardRefused { detail }) = advance else {
        panic!("expected FastForwardRefused, got {advance:?}");
    };
    assert!(detail.contains("index.lock"), "{detail}");
}

/// Git matches `includeIf "gitdir:"` against the git directory's path, so a
/// private `GIT_DIR` would lose this setting and write other bytes to the checkout.
#[test]
fn an_include_for_this_git_directory_shapes_the_files_the_fast_forward_writes() {
    let repo = init_repo();
    let root = repo.path().canonicalize().unwrap();
    stage_branch(&root, "s1", &[("b.txt", "line\n")]);
    let (old, new) = (rev(&root, "main"), rev(&root, "loom/s1"));
    let include = root.join(".git/operator-include");
    std::fs::write(&include, "[core]\n\tautocrlf = true\n").unwrap();
    let key = format!("includeIf.gitdir:{}/.git.path", root.display());
    git(&root, &["config", &key, include.to_str().unwrap()]);

    let advance = fast_forward(&root, "main", (&old, &new), Displaced::default());

    assert_eq!(advance, Advance::Advanced);
    assert_eq!(
        std::fs::read_to_string(root.join("b.txt")).unwrap(),
        "line\r\n"
    );
}

#[test]
fn a_refusal_without_paths_is_fast_forward_refused() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "branch")]);
    let new = rev(root, "loom/s1");
    commit_file(root, "c.txt", "diverges", "main diverges");
    let old = rev(root, "main");

    let advance = fast_forward(root, "main", (&old, &new), Displaced::default());

    let Advance::Blocked(MergeBlock::FastForwardRefused { detail }) = advance else {
        panic!("expected FastForwardRefused, got {advance:?}");
    };
    assert!(detail.contains("fast-forward"), "{detail}");
    assert_eq!(rev(root, "main"), old);
    assert_eq!(git(root, &["status", "--porcelain"]).trim(), "");
}

#[test]
fn a_refused_fast_forward_puts_displaced_files_back() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("a.txt", "branch a")]);
    let (old, new) = (rev(root, "main"), rev(root, "loom/s1"));
    std::fs::write(root.join("a.txt"), "branch a").unwrap();
    let displaced = displace_files(
        root,
        "s1",
        &rev(root, "loom/s1^{tree}"),
        &["a.txt".to_string()],
    )
    .unwrap();
    assert!(!root.join("a.txt").exists());
    let _guard = failpoint::inject(failpoint::Failures {
        fast_forward: true,
        ..Default::default()
    });

    let advance = fast_forward(root, "main", (&old, &new), displaced);

    assert!(
        matches!(
            advance,
            Advance::Blocked(MergeBlock::FastForwardRefused { .. })
        ),
        "{advance:?}"
    );
    assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"branch a");
    assert_eq!(rev(root, "main"), old);
}
