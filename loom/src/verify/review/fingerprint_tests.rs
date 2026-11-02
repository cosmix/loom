//! The fingerprint's hashing, and the changes it counts in a real repository.
//! No daemon owns these repositories, so [`compute`] computes locally, as the
//! daemon does.

use super::*;
use crate::git::run_git_checked;
use crate::verify::tool_artifacts::NAMES;

#[test]
fn fingerprint_from_records_deleted_entries() {
    let fingerprint = fingerprint_from(
        "base",
        &[
            ("gone.txt".to_string(), None),
            ("empty.txt".to_string(), Some(Vec::new())),
        ],
    );

    assert_eq!(fingerprint.base, "base");
    assert_eq!(fingerprint.files["gone.txt"], "deleted");
    assert_eq!(
        fingerprint.files["empty.txt"],
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert!(fingerprint.value.starts_with("sha256:"));
}

#[test]
fn fingerprint_from_sorts_paths_before_hashing() {
    let first = ("a.txt".to_string(), Some(b"a".to_vec()));
    let second = ("z.txt".to_string(), Some(b"z".to_vec()));

    let forward = fingerprint_from("base", &[first.clone(), second.clone()]);
    let reverse = fingerprint_from("base", &[second, first]);

    assert_eq!(forward, reverse);
}

#[test]
fn behaviour_is_additive_and_does_not_change_the_content_fingerprint() {
    let before = fingerprint_from(
        "base",
        &[
            ("a.rs".into(), Some(b"fn a() {}\n".to_vec())),
            ("doc/x.md".into(), Some(b"old".to_vec())),
        ],
    );
    let after = fingerprint_from(
        "base",
        &[
            ("a.rs".into(), Some(b"fn a() {}\n".to_vec())),
            ("doc/x.md".into(), Some(b"new".to_vec())),
        ],
    );
    assert_ne!(before.value, after.value);
    assert_ne!(before.files, after.files);
    assert_eq!(before.behaviour, after.behaviour);
    assert_eq!(before.behaviour.len(), 1);
    let wire = serde_json::to_vec(&after).unwrap();
    assert_eq!(
        serde_json::from_slice::<ChangeFingerprint>(&wire).unwrap(),
        after
    );
}

#[test]
fn changed_since_includes_one_sided_paths() {
    let previous = BTreeMap::from([
        ("a.txt".to_string(), "old".to_string()),
        ("b.txt".to_string(), "same".to_string()),
    ]);
    let current = BTreeMap::from([
        ("b.txt".to_string(), "same".to_string()),
        ("c.txt".to_string(), "new".to_string()),
    ]);

    assert_eq!(changed_since(&previous, &current), ["a.txt", "c.txt"]);
}

#[test]
fn behaviour_tracks_code_additions_deletions_and_directives() {
    let before = fingerprint_from("base", &[("a.ts".into(), Some(b"run();\n".to_vec()))]);
    for content in [
        None,
        Some(b"/// <reference types=\"x\" />\nrun();\n".to_vec()),
    ] {
        let after = fingerprint_from("base", &[("a.ts".into(), content)]);
        assert_ne!(before.value, after.value);
        assert_ne!(before.behaviour, after.behaviour);
    }
    let added = fingerprint_from(
        "base",
        &[
            ("a.ts".into(), Some(b"run();\n".to_vec())),
            ("b.rs".into(), Some(b"// comment only\n".to_vec())),
        ],
    );
    assert_ne!(before.behaviour, added.behaviour);
    assert!(added.behaviour.contains_key("b.rs"));
}

#[test]
fn daemon_response_preserves_behaviour_hashes() {
    use crate::daemon::Response;

    let fingerprint = fingerprint_from("base", &[("a.rs".into(), Some(b"fn a() {}\n".to_vec()))]);
    let response = Response::ChangesObserved {
        target_branch: "main".into(),
        fingerprint: fingerprint.clone(),
    };
    let wire = serde_json::to_vec(&response).unwrap();
    match serde_json::from_slice::<Response>(&wire).unwrap() {
        Response::ChangesObserved {
            fingerprint: observed,
            ..
        } => {
            assert!(!observed.behaviour.is_empty());
            assert_eq!(observed, fingerprint);
        }
        other => panic!("expected changes, got {other:?}"),
    }
}

fn git_ok(root: &Path, args: &[&str]) -> Result<()> {
    run_git_checked(args, root)?;
    Ok(())
}

/// A repository on `main` with `files` committed, switched to `feature`.
fn repo_on_feature(root: &Path, files: &[(&str, &str)]) -> Result<()> {
    git_ok(root, &["init", "-b", "main"])?;
    git_ok(root, &["config", "user.email", "review-test@example.com"])?;
    git_ok(root, &["config", "user.name", "Review Test"])?;
    for &(path, content) in files {
        std::fs::write(root.join(path), content)?;
        git_ok(root, &["add", "-f", path])?;
    }
    git_ok(root, &["commit", "-m", "initial"])?;
    git_ok(root, &["switch", "-c", "feature"])
}

#[test]
fn fingerprint_ignores_commits() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    repo_on_feature(root, &[("file.txt", "initial")])?;

    std::fs::write(root.join("file.txt"), b"first change")?;
    let a = compute(root, "main")?;
    git_ok(root, &["commit", "-am", "x"])?;
    let b = compute(root, "main")?;
    assert_eq!(a.value, b.value);
    assert_eq!(a.files, b.files);

    std::fs::write(root.join("file.txt"), b"second change")?;
    let c = compute(root, "main")?;
    assert_ne!(b.value, c.value);
    assert_eq!(changed_since(&b.files, &c.files), ["file.txt"]);
    Ok(())
}

/// An empty sandbox placeholder is not a change; content at the same kind of
/// name is. Git itself skips a FIFO.
#[test]
fn fingerprint_leaves_out_sandbox_artifacts() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    repo_on_feature(root, &[("file.txt", "initial")])?;
    nix::unistd::mkfifo(&root.join(".bashrc"), nix::sys::stat::Mode::S_IRWXU)?;
    std::fs::write(root.join(".gitconfig"), b"")?;
    std::fs::write(root.join(".profile"), b"export EDITOR=vi\n")?;
    std::fs::write(root.join("notes.txt"), b"x")?;

    let fingerprint = compute(root, "main")?;

    assert_eq!(
        fingerprint.files.keys().collect::<Vec<_>>(),
        [".profile", "notes.txt"]
    );
    Ok(())
}

/// A listed name tracked at the base stays a change even once it is empty:
/// only untracked entries are sandbox artifacts.
#[test]
fn a_listed_name_tracked_at_base_stays_a_change() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    repo_on_feature(root, &[(".gitconfig", "[core]\n")])?;
    std::fs::write(root.join(".gitconfig"), b"")?;

    let fingerprint = compute(root, "main")?;

    assert_eq!(fingerprint.files.keys().collect::<Vec<_>>(), [".gitconfig"]);
    Ok(())
}

/// The observed failure: every review round was recorded while the sandbox's
/// placeholders stood in the worktree as empty files, and completion ran once
/// they were gone. Both sides must see the same changes.
#[test]
fn placeholders_do_not_split_a_round_from_its_completion() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    repo_on_feature(root, &[("file.txt", "initial")])?;
    std::fs::write(root.join("file.txt"), b"reviewed change")?;

    for name in NAMES {
        std::fs::write(root.join(name), b"")?;
    }
    let round = compute(root, "main")?;
    for name in NAMES {
        std::fs::remove_file(root.join(name))?;
    }
    let completion = compute(root, "main")?;

    assert_eq!(round, completion);
    assert_eq!(round.files.keys().collect::<Vec<_>>(), ["file.txt"]);
    Ok(())
}

/// The base is the merge base, not the target's tip: a commit that reached the
/// target after the stage branched is not the stage's change.
#[test]
fn a_commit_on_the_target_after_the_branch_point_is_not_a_stage_change() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    repo_on_feature(root, &[("file.txt", "initial")])?;
    std::fs::write(root.join("stage.txt"), b"from the stage")?;
    git_ok(root, &["add", "stage.txt"])?;
    git_ok(root, &["commit", "-m", "the stage's commit"])?;
    git_ok(root, &["switch", "main"])?;
    std::fs::write(root.join("landed.txt"), b"from another stage")?;
    git_ok(root, &["add", "landed.txt"])?;
    git_ok(root, &["commit", "-m", "landed on main"])?;
    git_ok(root, &["switch", "feature"])?;

    let fingerprint = compute(root, "main")?;

    assert_eq!(fingerprint.files.keys().collect::<Vec<_>>(), ["stage.txt"]);
    Ok(())
}
