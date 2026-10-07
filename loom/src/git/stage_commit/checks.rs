//! The checks of a daemon commit: the request's own fields, then the paths
//! the commit changes. Paths are read from trees, never the index: the staged
//! tree id is fixed once `write-tree` matched the request, so nothing a
//! session does to its index afterwards changes what is checked.

use std::collections::HashSet;
use std::path::Path;

use super::{git_failure, refused, CommitRefusal, CommitScope, Committer};
use crate::fs::safe_read::read_to_string_bounded;
use crate::git::branch::branch_ref;

/// Longest commit message accepted, in bytes.
const MAX_MESSAGE_BYTES: usize = 16 * 1024;

/// Largest `MERGE_HEAD` file read: git writes one object id per line.
const MAX_MERGE_HEAD_BYTES: usize = 4096;

/// A commit message the daemon writes: non-empty after trimming, at most
/// 16 KiB, no NUL byte, and no AI attribution. `loom stage commit` checks the
/// same before relaying; the daemon checks again because a session can write
/// a relay ticket without the CLI.
pub fn validate_commit_message(message: &str) -> Result<(), String> {
    if message.trim().is_empty() {
        return Err("the commit message is empty".to_string());
    }
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(format!(
            "the commit message is {} bytes; the limit is {MAX_MESSAGE_BYTES}",
            message.len()
        ));
    }
    if message.contains('\0') {
        return Err("the commit message contains a NUL byte".to_string());
    }
    match message.lines().find(|line| is_attribution(line)) {
        Some(line) => Err(format!(
            "the commit message carries AI attribution ({}); remove it",
            line.trim()
        )),
        None => Ok(()),
    }
}

/// A line that credits an AI system: a `Co-Authored-By:` or `Signed-off-by:`
/// trailer naming claude or anthropic, an Anthropic noreply address, or a
/// "generated with" line naming Claude Code. Prose that merely says claude
/// passes.
fn is_attribution(line: &str) -> bool {
    let line = line.trim().to_lowercase();
    let trailer = line.starts_with("co-authored-by:") || line.starts_with("signed-off-by:");
    let names_vendor = line.contains("claude") || line.contains("anthropic");
    let generated = line.contains("generated with")
        && ["claude code", "claude.ai", "claude.com"]
            .iter()
            .any(|name| line.contains(name));
    (trailer && names_vendor) || line.contains("noreply@anthropic") || generated
}

/// `value` is a full object id (40 or 64 lowercase hex characters), so no
/// agent-supplied string reaches git's argv unchecked.
pub(super) fn require_object_id(name: &str, value: &str) -> Result<(), CommitRefusal> {
    let hex = value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if hex && matches!(value.len(), 40 | 64) {
        Ok(())
    } else {
        Err(refused(format!(
            "{name} must be a full lowercase object id, got {value:?}"
        )))
    }
}

/// First path components that hold loom state. Compared ignoring ASCII case:
/// a case-insensitive filesystem resolves `.LOOM/work` to `.loom/work`.
const STATE_DIRS: [&str; 3] = [".loom", ".work", ".worktrees"];

/// The mode git records for a submodule (a gitlink).
const GITLINK_MODE: &str = "160000";

/// One path `diff-tree --raw` lists, with its mode in the newer tree.
struct Change {
    new_mode: String,
    path: String,
}

impl Committer<'_> {
    /// Refuse a commit of `tree` on `head` that changes a state path, adds or
    /// moves a gitlink, leaves a Knowledge scope's prefix, or changes nothing
    /// outside a merge. In a Merge scope a gitlink or state path whose entry
    /// equals MERGE_HEAD's is the target's own and passes.
    pub(super) fn require_allowed_paths(
        &self,
        scope: &CommitScope,
        head: &str,
        merge_head: Option<&str>,
        tree: &str,
    ) -> Result<(), CommitRefusal> {
        let changes = self.changes(head, tree)?;
        if changes.is_empty() && merge_head.is_none() {
            return Err(refused("nothing to commit: the staged tree equals HEAD's"));
        }
        let differs_from_merge_head: Option<HashSet<String>> = match merge_head {
            Some(id) => Some(
                self.changes(id, tree)?
                    .into_iter()
                    .map(|c| c.path)
                    .collect(),
            ),
            None => None,
        };
        for change in &changes {
            let from_merge_head = differs_from_merge_head
                .as_ref()
                .is_some_and(|differs| !differs.contains(&change.path));
            if let Some(problem) = protected(change).filter(|_| !from_merge_head) {
                return Err(refused(problem));
            }
            if let CommitScope::Knowledge { prefix, .. } = scope {
                if !Path::new(&change.path).starts_with(prefix) {
                    return Err(refused(format!(
                        "{:?} is outside {}; a knowledge commit carries only knowledge files",
                        change.path,
                        prefix.display()
                    )));
                }
            }
        }
        Ok(())
    }

    /// Every path whose entry differs between `from` and `tree`, submodules
    /// included.
    fn changes(&self, from: &str, tree: &str) -> Result<Vec<Change>, CommitRefusal> {
        let raw = self.stdout(&[
            "diff-tree",
            "-r",
            "-z",
            "--raw",
            "--no-renames",
            "--ignore-submodules=none",
            from,
            tree,
        ])?;
        Ok(parse_raw(&raw))
    }

    /// `MERGE_HEAD` lists one commit. An octopus merge lists one per merged
    /// head, and a commit taking only the first as its second parent would
    /// drop the rest. The file is read without following a symlink: git
    /// never writes one there.
    pub(super) fn require_single_merge_head(&self) -> Result<(), CommitRefusal> {
        let listed = self.checked(&[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "MERGE_HEAD",
        ])?;
        let path = Path::new(&listed);
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(refused(format!("MERGE_HEAD resolved to {listed:?}")));
        };
        let content = read_to_string_bounded(dir, Path::new(name), MAX_MERGE_HEAD_BYTES)
            .map_err(|error| refused(format!("MERGE_HEAD could not be read: {error:#}")))?;
        let heads = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        if heads > 1 {
            return Err(refused(format!(
                "MERGE_HEAD names {heads} commits; a merge commit takes one: run git merge \
                 --abort, then git merge --no-commit --no-ff <target>"
            )));
        }
        Ok(())
    }

    /// `merge_head` is the merge target's tip or an ancestor of it, when the
    /// committer was given a target. An ancestor passes because the target
    /// may have moved on since the resolver merged it.
    pub(super) fn require_merged_target(&self, merge_head: &str) -> Result<(), CommitRefusal> {
        let Some(target) = self.merge_target else {
            return Ok(());
        };
        let reference = branch_ref(target);
        let args = [
            "merge-base",
            "--is-ancestor",
            merge_head,
            reference.as_str(),
        ];
        let output = self.run(&args)?;
        match output.status.code() {
            Some(0) => Ok(()),
            Some(1) => Err(refused(format!(
                "MERGE_HEAD {merge_head} is not on {target}; a merge session commits a merge \
                 of {target} only: run git merge --abort, then git merge --no-commit --no-ff \
                 {target}"
            ))),
            _ => Err(git_failure(&args, &output)),
        }
    }
}

/// Why `change` may not be committed by a session, if it may not. The path is
/// quoted (`{:?}`): the daemon logs the reason, and a session picks the path,
/// so a newline in it must not start a log line.
fn protected(change: &Change) -> Option<String> {
    if is_state_path(&change.path) {
        return Some(format!(
            "{:?} is loom state, which a session never commits; unstage it",
            change.path
        ));
    }
    if change.new_mode == GITLINK_MODE {
        return Some(format!(
            "{:?} is a gitlink (submodule); a stage commit never adds or moves one",
            change.path
        ));
    }
    None
}

fn is_state_path(path: &str) -> bool {
    let first = path.split('/').next().unwrap_or(path);
    STATE_DIRS.iter().any(|dir| first.eq_ignore_ascii_case(dir))
}

/// Parse `diff-tree -r -z --raw` output: a `:<old mode> <new mode> <old id>
/// <new id> <status>` field, then the path (two paths for a rename or copy).
fn parse_raw(raw: &str) -> Vec<Change> {
    let mut fields = raw.split('\0').filter(|field| !field.is_empty());
    let mut changes = Vec::new();
    while let Some(meta) = fields.next() {
        let mut parts = meta.trim_start_matches(':').split(' ');
        let new_mode = parts.nth(1).unwrap_or_default().to_string();
        let status = parts.nth(2).unwrap_or_default();
        let paths = if status.starts_with(['R', 'C']) { 2 } else { 1 };
        for path in fields.by_ref().take(paths) {
            changes.push(Change {
                new_mode: new_mode.clone(),
                path: path.to_string(),
            });
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_must_be_short_non_empty_text() {
        let long = "x".repeat(MAX_MESSAGE_BYTES + 1);
        for bad in [" \n", "a\0b", long.as_str()] {
            assert!(validate_commit_message(bad).is_err(), "{bad:?}");
        }
        assert_eq!(validate_commit_message("feat(s1): a\n\nbody"), Ok(()));
    }

    #[test]
    fn a_gitlink_path_is_quoted_in_the_refusal() {
        let change = Change {
            new_mode: GITLINK_MODE.to_string(),
            path: "sub\nforged: line".to_string(),
        };

        let reason = protected(&change).unwrap();

        assert_eq!(reason.lines().count(), 1, "{reason:?}");
        assert!(
            reason.starts_with(r#""sub\nforged: line" is a gitlink"#),
            "{reason:?}"
        );
    }

    #[test]
    fn attribution_is_refused_and_plain_prose_passes() {
        for attributed in [
            "feat: a\n\nCo-Authored-By: Claude <noreply@anthropic.com>",
            "feat: a\n\nSigned-off-by: Claude",
            "feat: a\n\n  co-authored-by: anthropic bot",
            "feat: a\n\nGenerated with Claude Code",
            "feat: a\n\nsee https://claude.com/claude-code, generated with it",
            "feat: a\n\nmail noreply@anthropic.com",
        ] {
            assert!(validate_commit_message(attributed).is_err(), "{attributed}");
        }
        for plain in [
            "feat(signal): tell claude sessions to commit through loom",
            "fix: Signed-off-by: is parsed\n\nSigned-off-by: Jane Doe <jane@example.com>",
        ] {
            assert_eq!(validate_commit_message(plain), Ok(()), "{plain}");
        }
    }
}
