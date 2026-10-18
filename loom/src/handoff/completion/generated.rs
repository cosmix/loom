//! Untracked output the project's committed `.gitignore` files ignore.
//!
//! Whatever the language, a project states its generated output (`target/`,
//! `_build/`, `build/classes/`, `__pycache__/`, `node_modules/`) in the
//! `.gitignore` files it commits, so completion reads that statement instead
//! of a list of tool directories. Only rules the stage commit carries count:
//! `ls-files` without `--exclude-standard` never reads `.git/info/exclude` or
//! `core.excludesFile`, both local and unreviewed. The tracked `.gitignore`
//! files equal HEAD because completion refuses any tracked change, and an
//! untracked `.gitignore` that git read is refused as an input itself:
//! `--exclude=!.gitignore` outranks every per-directory rule, so a
//! `.gitignore` is listed as ignored only inside a directory a rule above it
//! excludes, where git never reads it.
use crate::git::worktree::WorktreeGit;
use crate::models::stage::{Stage, StageType};
use anyhow::{ensure, Context, Result};
use std::collections::{BTreeSet, HashSet};
use std::path::Path;

const IGNORED_OUTPUT: [&str; 6] = [
    "ls-files",
    "--others",
    "--ignored",
    "-z",
    "--exclude-per-directory=.gitignore",
    "--exclude=!.gitignore",
];

/// Directories per `ls-files` call, well inside any argv limit.
const PATHSPEC_BATCH: usize = 256;

/// The NUL-separated paths `git <args>` lists in the worktree.
pub(super) fn listed_paths(git: &WorktreeGit, args: &[&str]) -> Result<Vec<String>> {
    let output = git.run(args)?;
    ensure!(
        output.status.success(),
        "cannot enumerate completion inputs: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            std::str::from_utf8(path)
                .map(str::to_owned)
                .context("non-UTF-8 untracked completion input")
        })
        .collect()
}

/// Untracked paths the committed `.gitignore` files ignore. A knowledge stage
/// honours none: its diff check covers only the knowledge directory, so the
/// `.gitignore` files above it are not proven committed.
pub(super) fn ignored_output(stage: &Stage, git: &WorktreeGit) -> Result<HashSet<String>> {
    if stage.stage_type == StageType::Knowledge {
        return Ok(HashSet::new());
    }
    Ok(listed_paths(git, &IGNORED_OUTPUT)?.into_iter().collect())
}

/// Refuse ignored output a stage declaration names: the path itself, or a
/// directory above it that holds no tracked file and so is wholly generated
/// (untracked files there that no rule ignores were refused already). A
/// declaration of a source directory holding tracked files (`packages/**`)
/// covers its sources, not the output the project ignores inside it.
pub(super) fn reject_declared_output(
    stage: &Stage,
    git: &WorktreeGit,
    outputs: &[String],
) -> Result<()> {
    let mut directories = BTreeSet::new();
    for path in outputs {
        for named in super::content_inputs::named_inputs(stage, path)? {
            ensure!(named != *path, "{}", declared_output(path));
            directories.insert(named);
        }
    }
    let directories: Vec<String> = directories.into_iter().collect();
    let mut sourced = HashSet::new();
    for batch in directories.chunks(PATHSPEC_BATCH) {
        sourced.extend(tracked_ancestors(git, batch)?);
    }
    let Some(generated) = directories.iter().find(|dir| !sourced.contains(*dir)) else {
        return Ok(());
    };
    let path = outputs
        .iter()
        .find(|path| Path::new(path).starts_with(generated))
        .map_or(generated.as_str(), String::as_str);
    anyhow::bail!("{}", declared_output(path))
}

fn declared_output(path: &str) -> String {
    format!(
        "untracked completion input '{path}': the stage declares this ignored output; \
         commit it with git add -f before verification"
    )
}

/// The members of `directories` that hold at least one tracked file.
fn tracked_ancestors(git: &WorktreeGit, directories: &[String]) -> Result<HashSet<String>> {
    let pathspecs: Vec<String> = directories
        .iter()
        .map(|dir| format!(":(literal){dir}"))
        .collect();
    let mut args = vec!["ls-files", "--cached", "-z", "--"];
    args.extend(pathspecs.iter().map(String::as_str));
    let wanted: HashSet<&str> = directories.iter().map(String::as_str).collect();
    let mut sourced = HashSet::new();
    for tracked in listed_paths(git, &args)? {
        for ancestor in Path::new(&tracked).ancestors() {
            if let Some(dir) = ancestor.to_str().filter(|dir| wanted.contains(dir)) {
                sourced.insert(dir.to_owned());
            }
        }
    }
    Ok(sourced)
}
