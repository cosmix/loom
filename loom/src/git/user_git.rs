//! Git as the user runs it in their own checkout: their hooks, signing and identity apply.
//!
//! Every [`super::runner`] call switches hooks off, which suits the commands loom runs on its own
//! account. A commit the user asked for is different: skipping their checks would record what
//! their own `git commit` refuses.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Error, Result};

use super::runner::{run_git, run_git_with_env, REPO_REDIRECT_ENV};

/// Variables that change how git reads every pathspec. Inherited from loom's environment they
/// would make git read a `:(literal)` pathspec as a file name (`GIT_LITERAL_PATHSPECS`), match it
/// without case or turn its magic off, and the user's hooks would inherit them too.
const PATHSPEC_ENV: [&str; 4] = [
    "GIT_LITERAL_PATHSPECS",
    "GIT_GLOB_PATHSPECS",
    "GIT_NOGLOB_PATHSPECS",
    "GIT_ICASE_PATHSPECS",
];

/// `git` in `root` with stdin closed and no inherited variable able to point it at another
/// repository or change how it reads pathspecs; nothing else about the user's git is changed.
pub(crate) fn user_git(root: &Path) -> Command {
    let mut git = Command::new("git");
    for variable in REPO_REDIRECT_ENV.iter().chain(&PATHSPEC_ENV) {
        git.env_remove(variable);
    }
    git.current_dir(root).stdin(Stdio::null());
    git
}

/// `paths` as pathspecs git matches literally, never as globs. Not the global
/// `--literal-pathspecs`: git exports that to hooks as `GIT_LITERAL_PATHSPECS=1`, which changes
/// what a user's hook means by its own pathspecs.
pub(crate) fn literal<S: AsRef<str>>(paths: &[S]) -> Vec<String> {
    paths
        .iter()
        .map(|path| format!(":(literal){}", path.as_ref()))
        .collect()
}

/// Repository-relative `paths` as literal pathspecs, read from the top of the work tree.
fn top_literal<S: AsRef<str>>(paths: &[S]) -> Vec<String> {
    paths
        .iter()
        .map(|path| format!(":(top,literal){}", path.as_ref()))
        .collect()
}

/// Loom's own git (no hooks, `runner`) in `root` with `args`, then `paths` as [`literal`]
/// pathspecs, the pathspec variables loom inherited switched off so the literal match holds.
pub(crate) fn run_literal<S: AsRef<str>>(
    root: &Path,
    args: &[&str],
    paths: &[S],
) -> Result<Output> {
    run_pathspecs(root, args, &literal(paths))
}

/// Loom's own git in `root` with `args`, then `pathspecs` with their magic as written.
fn run_pathspecs(root: &Path, args: &[&str], pathspecs: &[String]) -> Result<Output> {
    let mut argv = args.to_vec();
    argv.extend(pathspecs.iter().map(String::as_str));
    let off = OsStr::new("0");
    let env: Vec<(&str, &OsStr)> = PATHSPEC_ENV.iter().map(|name| (*name, off)).collect();
    run_git_with_env(&argv, &env, root)
}

/// HEAD's commit, or `None` on an unborn branch (or one git cannot resolve).
pub(crate) fn head(root: &Path) -> Result<Option<String>> {
    let output = run_git(&["rev-parse", "-q", "--verify", "HEAD^{commit}"], root)?;
    let id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((output.status.success() && !id.is_empty()).then_some(id))
}

/// The index as a user's commit found it: HEAD then, and the paths the user had staged.
///
/// A commit given pathspecs has `--only` semantics: git builds a temporary index from HEAD and
/// the pathspec's paths, runs the hooks on it and commits it, then keeps the real index with the
/// pathspec's paths updated from the work tree before the hooks. A hook that rewrites and adds a
/// file updates the temporary index alone, so the real one keeps the entry from before it: a
/// staged reversal of the hook's work that the user's next commit records. [`IndexBefore::reset`]
/// corrects that, and leaves the paths the user had staged as they were.
pub(crate) struct IndexBefore {
    head: Option<String>,
    /// Repository-relative paths whose index entries differed from `head` (every entry on an
    /// unborn branch).
    staged: BTreeSet<String>,
    /// Where `root` sits under the top of the work tree (`sub/dir/`, empty at the top), which
    /// turns a `root`-relative path repository-relative.
    prefix: String,
}

/// What putting the index right after a commit left for the user to know.
pub(crate) struct IndexReset {
    /// Repository-relative paths a hook committed beside the commit's own, whose entries the user
    /// had staged before it: they are kept, and differ from the commit.
    kept: Vec<String>,
    /// Why git could not list or reset the index, when it could not.
    failed: Option<String>,
}

impl IndexBefore {
    /// Read through loom's own git before a user's commit in `root`, before anything is staged.
    pub(crate) fn read(root: &Path) -> Result<IndexBefore> {
        let head = head(root)?;
        let shown = checked(
            "git rev-parse",
            run_git(&["rev-parse", "--show-prefix"], root),
        )
        .map_err(Error::msg)?;
        let prefix = String::from_utf8(shown.stdout).context("git printed a non-UTF-8 prefix")?;
        let staged = staged(root, head.as_deref()).map_err(Error::msg)?;
        Ok(IndexBefore {
            head,
            staged: staged.into_iter().collect(),
            prefix: prefix.trim_end_matches('\n').to_owned(),
        })
    }

    /// HEAD before the commit, `None` on an unborn branch.
    pub(crate) fn head(&self) -> Option<&str> {
        self.head.as_deref()
    }

    /// After a commit of `paths` (`root`-relative) that moved HEAD to `commit` (`HEAD` when its id
    /// is not known): `paths` and every other path the commit changed are reset to it, but for
    /// the ones the user had staged, which are kept and reported when they differ from it.
    pub(crate) fn reset<S: AsRef<str>>(
        &self,
        root: &Path,
        paths: &[S],
        commit: &str,
    ) -> IndexReset {
        let ours: BTreeSet<String> = paths
            .iter()
            .map(|path| format!("{}{}", self.prefix, path.as_ref()))
            .collect();
        let mut failures = Vec::new();
        let (others, staged): (Vec<String>, Vec<String>) =
            match changed(root, self.head.as_deref(), commit) {
                Ok(changed) => changed
                    .into_iter()
                    .filter(|path| !ours.contains(path))
                    .partition(|path| !self.staged.contains(path)),
                Err(failed) => {
                    failures.push(failed);
                    Default::default()
                }
            };
        let targets: Vec<&String> = ours.iter().chain(&others).collect();
        let reset = run_pathspecs(root, &["reset", "-q", "--"], &top_literal(&targets));
        if let Err(failed) = checked("git reset", reset) {
            failures.push(failed);
        }
        let kept = differing(root, commit, &staged).unwrap_or_else(|failed| {
            failures.push(failed);
            staged
        });
        IndexReset {
            kept,
            failed: (!failures.is_empty()).then(|| failures.join("; ")),
        }
    }
}

impl IndexReset {
    /// What is left for the reader, as sentences.
    pub(crate) fn notes(&self) -> Vec<String> {
        let failed = self.failed.as_ref().map(|failed| {
            format!(
                "The index could not be reset to the commit ({failed}), so it may still hold the \
                 text from before the commit hooks ran."
            )
        });
        let kept = (!self.kept.is_empty()).then(|| {
            format!(
                "A commit hook also committed {}, whose changes staged before the commit stay \
                 staged and differ from it; check them before the next commit.",
                self.kept.join(", ")
            )
        });
        failed.into_iter().chain(kept).collect()
    }
}

/// The repository-relative paths whose index entries differ from `head`, every entry on an unborn
/// branch (`None`).
fn staged(root: &Path, head: Option<&str>) -> Result<Vec<String>, String> {
    match head {
        Some(head) => cached_differences(root, head, &[]),
        None => {
            let args = ["ls-files", "-z", "--full-name", "--"];
            let listed = run_pathspecs(root, &args, &[":(top)".to_owned()]);
            names("git ls-files", listed)
        }
    }
}

/// The repository-relative paths `commit` changed since `before`, renames listed as their two
/// paths. On an unborn branch (`None`) every path reachable at `commit`: a hook may have committed
/// on top of the first commit, and a diff against `commit`'s parent would miss the first one's.
fn changed(root: &Path, before: Option<&str>, commit: &str) -> Result<Vec<String>, String> {
    match before {
        Some(before) => {
            let args = [
                "diff-tree",
                "-r",
                "--no-commit-id",
                "--name-only",
                "-z",
                "--no-renames",
                before,
                commit,
                "--",
            ];
            names("git diff-tree", run_git(&args, root))
        }
        None => {
            let args = ["ls-tree", "-r", "--name-only", "-z", "--full-tree", commit];
            names("git ls-tree", run_git(&args, root))
        }
    }
}

/// The ones among repository-relative `paths` whose index entries differ from `commit`.
fn differing(root: &Path, commit: &str, paths: &[String]) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    cached_differences(root, commit, &top_literal(paths))
}

/// The repository-relative paths, among `pathspecs` (every path when there are none), whose
/// index entries differ from `tree`.
fn cached_differences(
    root: &Path,
    tree: &str,
    pathspecs: &[String],
) -> Result<Vec<String>, String> {
    let args = [
        "diff-index",
        "--cached",
        "--name-only",
        "-z",
        "--no-renames",
        tree,
        "--",
    ];
    names("git diff-index", run_pathspecs(root, &args, pathspecs))
}

/// The NUL-separated names `step` printed, once it succeeded. A name that is not UTF-8 is left out:
/// it can never equal a path loom selected or reset, which are UTF-8 strings, so the listing's
/// other names stay usable beside a user's file named in another encoding.
fn names(step: &str, ran: Result<Output>) -> Result<Vec<String>, String> {
    let output = checked(step, ran)?;
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .filter_map(|name| String::from_utf8(name.to_vec()).ok())
        .collect())
}

/// `ran`'s output when `step` succeeded, else why not: the last line git printed, or why it could
/// not run.
fn checked(step: &str, ran: Result<Output>) -> Result<Output, String> {
    match ran {
        Ok(output) if output.status.success() => Ok(output),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let last = stderr.lines().map(str::trim).rfind(|line| !line.is_empty());
            Err(format!("{step} failed: {}", last.unwrap_or("no output")))
        }
        Err(error) => Err(format!("{step} could not run: {}", error.root_cause())),
    }
}

#[cfg(test)]
#[path = "user_git_tests.rs"]
mod index_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pathspec_variables_from_loom_are_not_passed_on() {
        let git = user_git(Path::new("/"));
        for variable in PATHSPEC_ENV.iter().chain(&REPO_REDIRECT_ENV) {
            let removed = git
                .get_envs()
                .any(|(key, value)| key == OsStr::new(variable) && value.is_none());
            assert!(removed, "{variable} must be removed");
        }
    }

    #[test]
    fn literal_pathspecs_carry_the_magic_prefix() {
        assert_eq!(
            literal(&["doc/a*.md", "doc/[b].md"]),
            [":(literal)doc/a*.md", ":(literal)doc/[b].md"]
        );
        assert_eq!(top_literal(&["sub/a*.md"]), [":(top,literal)sub/a*.md"]);
    }
}
