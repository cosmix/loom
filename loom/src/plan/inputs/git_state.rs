//! What git says about plan inputs: the objects HEAD records, the paths with changes, and the
//! objects the working tree holds. Each call answers every path it is given at once. Paths go in
//! and come out relative to `root`, which may be a subdirectory of the repository.

use anyhow::{ensure, Context, Result};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::Path;

use crate::git::runner::{run_git_checked, run_git_records, run_git_with_env};

/// The object HEAD records at each of `pathspecs`, by path. `ls-tree` prints paths relative to
/// the directory it runs in, as the pathspecs are.
pub(super) fn committed_objects(
    root: &Path,
    pathspecs: &[String],
) -> Result<HashMap<String, String>> {
    let mut args = vec!["ls-tree", "-z", "HEAD", "--"];
    args.extend(pathspecs.iter().map(String::as_str));
    let listing = run_git_records(&args, root)?;
    let entries = listing
        .split('\0')
        .filter_map(|entry| entry.split_once('\t'));
    let objects = entries.filter_map(|(meta, path)| {
        let object = meta.split_whitespace().nth(2)?;
        Some((path.to_owned(), object.to_owned()))
    });
    Ok(objects.collect())
}

/// Every path `git status` reports under `pathspecs`, both sides of a rename or copy. A path
/// outside `root` is left out: no input can be there.
pub(super) fn changed_paths(root: &Path, pathspecs: &[String]) -> Result<HashSet<String>> {
    // Porcelain paths are relative to the repository's top level; `root` may be below it.
    let prefix = run_git_checked(&["rev-parse", "--show-prefix"], root)?;
    let status = status_records(root, pathspecs)?;
    let mut records = status.split('\0').filter(|record| !record.is_empty());
    let mut changed = HashSet::new();
    while let Some(record) = records.next() {
        let (code, path) = record
            .split_at_checked(3)
            .context("malformed git status record")?;
        changed.extend(path.strip_prefix(prefix.as_str()).map(str::to_owned));
        if code.contains(['R', 'C']) {
            let source = records
                .next()
                .and_then(|from| from.strip_prefix(prefix.as_str()));
            changed.extend(source.map(str::to_owned));
        }
    }
    Ok(changed)
}

/// `git status --porcelain=v1 -z --untracked-files=all` under `pathspecs`, run with
/// `GIT_OPTIONAL_LOCKS=0`: refreshing the index would take `.git/index.lock` and block the
/// operator's own git every time the server revalidates.
fn status_records(root: &Path, pathspecs: &[String]) -> Result<String> {
    let mut args = vec![
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--",
    ];
    args.extend(pathspecs.iter().map(String::as_str));
    let env = [("GIT_OPTIONAL_LOCKS", OsStr::new("0"))];
    let output = run_git_with_env(&args, &env, root)?;
    ensure!(
        output.status.success(),
        "git status failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    String::from_utf8(output.stdout).context("git records contain a non-UTF-8 path")
}

/// The object id each of `paths` hashes to as it is in the working tree, by path.
pub(super) fn working_objects<'p>(
    root: &Path,
    paths: &[&'p str],
) -> Result<HashMap<&'p str, String>> {
    if paths.is_empty() {
        return Ok(HashMap::new());
    }
    let mut args = vec!["hash-object", "--"];
    args.extend(paths.iter().copied());
    let hashed = run_git_checked(&args, root)?;
    let objects: Vec<&str> = hashed.lines().collect();
    ensure!(
        objects.len() == paths.len(),
        "git hash-object answered {} of {} paths",
        objects.len(),
        paths.len()
    );
    let pairs = paths
        .iter()
        .copied()
        .zip(objects.into_iter().map(str::to_owned));
    Ok(pairs.collect())
}
