//! Structural validation for a candidate `.git` entry.

use std::path::Path;

/// True when `git_path` (a candidate `<root>/.git`) looks like a real git
/// directory or worktree pointer, not merely a path that happens to be
/// named `.git`.
///
/// A worktree's `.git` is a FILE containing `gitdir: <path>` - git always
/// writes that content, so existence alone is a reliable signal. A real
/// `.git` DIRECTORY, whether from `git init`, `git clone`, or a bare repo,
/// always contains a `HEAD` file immediately; an ancestor directory that
/// merely happens to be named `.git` (for example one left behind, empty,
/// by an unrelated process sharing the same OS temp root) does not. Bare
/// existence would accept both; this rejects the impostor.
pub(crate) fn is_real_git_dir(git_path: &Path) -> bool {
    if git_path.is_file() {
        return true;
    }
    git_path.is_dir() && git_path.join("HEAD").exists()
}
