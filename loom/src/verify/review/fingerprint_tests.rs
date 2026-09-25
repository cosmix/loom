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
