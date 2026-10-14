//! The plan's write denies, enforced on a merge commit.
//!
//! A merge resolver's sandbox is released from the plan's worktree-relative
//! `deny_write` entries, because `git merge` must rewrite whatever the target
//! changed (`sandbox::merge_resolver`). What it commits into such a path is
//! checked here instead, against git's own merge of the two sides: a path git
//! merged cleanly holds exactly that result, and a path git could not merge
//! holds one side of it, ours or theirs. A deleted path is an entry too:
//! absent on a side.

use std::collections::HashSet;

use glob::{MatchOptions, Pattern};

use super::{refused, require_object_id, Change, CommitRefusal, Committer};
use crate::git::merge::{merge_tree, TreeMerge};

mod quote;
use quote::shell_path;

/// Paths named in a refusal before the rest are counted.
const LISTED_PATHS: usize = 10;

impl Committer<'_> {
    /// Refuse a merge commit whose guarded paths hold content git's merge of
    /// `head` and `merge_head` did not produce. `ours` lists the paths whose
    /// entry differs between `head` and `tree`, and `differs_from_theirs`
    /// those that differ between `merge_head` and `tree`: a conflicted path
    /// in both holds an entry neither side has. A path git merged cleanly
    /// must hold its merge result, so taking either side of it is refused as
    /// well: after a fast-forward landing that would revert the other side's
    /// change.
    pub(super) fn require_guarded_paths_carried(
        &self,
        head: &str,
        merge_head: &str,
        tree: &str,
        ours: &[Change],
        differs_from_theirs: &HashSet<String>,
    ) -> Result<(), CommitRefusal> {
        if self.guarded_paths.is_empty() {
            return Ok(());
        }
        let (merged, conflicted) = self.merge_of(head, merge_head)?;
        let conflicted_set: HashSet<&str> = conflicted.iter().map(String::as_str).collect();
        let differs_from_ours: HashSet<&str> = ours.iter().map(|c| c.path.as_str()).collect();
        let altered: Vec<String> = self
            .changes(&merged, tree)?
            .into_iter()
            .map(|change| change.path)
            .filter(|path| !conflicted_set.contains(path.as_str()) && self.is_guarded(path))
            .collect();
        let authored: Vec<String> = conflicted
            .iter()
            .filter(|path| {
                differs_from_ours.contains(path.as_str())
                    && differs_from_theirs.contains(path.as_str())
                    && self.is_guarded(path)
            })
            .cloned()
            .collect();
        if altered.is_empty() && authored.is_empty() {
            return Ok(());
        }
        Err(refused(refusal_reason(&merged, &authored, &altered)))
    }

    /// Whether the plan's `deny_write` globs cover `path`.
    fn is_guarded(&self, path: &str) -> bool {
        self.guarded_paths.iter().any(|glob| covers(glob, path))
    }

    /// Git's own merge of `head` and `merge_head`: the tree it wrote, and the
    /// paths it could not merge. The main repository computes it, never the
    /// worktree a session controls; both share one object database.
    fn merge_of(
        &self,
        head: &str,
        merge_head: &str,
    ) -> Result<(String, Vec<String>), CommitRefusal> {
        let (tree, conflicted) = match merge_tree(self.repo_root, head, merge_head) {
            Ok(TreeMerge::Clean { tree }) => (tree, Vec::new()),
            Ok(TreeMerge::Conflict { tree, paths }) => (tree, paths),
            Err(error) => {
                return Err(refused(format!(
                    "git's own merge of the two sides could not be computed: {error:#}"
                )))
            }
        };
        require_object_id("the merged tree", &tree)?;
        Ok((tree, conflicted))
    }
}

/// The refusal for `authored` (conflicted paths holding neither side) and
/// `altered` (cleanly merged paths holding something other than the merge
/// result), each with the command that puts it right. One line: the daemon
/// logs it and a session picks the paths. No punctuation abuts a command: one
/// is followed by a space, a `;` or the end of the text, so a copied selection
/// never carries a stray `.` into a path word. The instruction to commit again
/// comes before the commands, and the altered paths' commands come last.
fn refusal_reason(merged: &str, authored: &[String], altered: &[String]) -> String {
    let mut ways_out = Vec::new();
    if !authored.is_empty() {
        let commands = path_commands(authored, |path| {
            format!(
                "{} (the stage branch's version) or {} (the target's)",
                restore_command("HEAD", path),
                restore_command("MERGE_HEAD", path),
            )
        });
        ways_out.push(format!(
            "Git could not merge these paths and the commit holds content neither side has \
             there: keep the stage branch's version or take the target's (a side without the \
             path removes it): {commands}"
        ));
    }
    if !altered.is_empty() {
        let commands = path_commands(altered, |path| restore_command(merged, path));
        ways_out.push(format!(
            "These paths merged cleanly, but the commit holds other content there: restore git's \
             merge result: {commands}"
        ));
    }
    format!(
        "this merge commit changes paths the plan's deny_write protects beyond the merge: a merge \
         session carries git's merge result, or one side of a path git could not merge, and \
         never authors content. Run one printed command for each path, then commit again. {}",
        ways_out.join(". ")
    )
}

/// The command that sets `path` in the index and the worktree to what
/// `source` holds, and removes it where `source` lacks it. `path` is one shell
/// word. `--literal-pathspecs` makes it match only itself: a name holding
/// `[`, `*` or `?` must not restore its neighbours as well. A resolved path
/// sits at stage 0, where `git checkout --ours` only copies the index entry
/// back, and a side without the path has nothing to check out.
fn restore_command(source: &str, path: &str) -> String {
    format!("git --literal-pathspecs restore --source={source} --staged --worktree -- {path}")
}

/// `restore_command` with `<path>` where the path goes: the shape the
/// refusal prints once per path, which the merge signal explains.
pub fn guarded_restore_command(source: &str) -> String {
    restore_command(source, "<path>")
}

/// For each of the first [`LISTED_PATHS`] of `paths`, what `command` makes of
/// the path as one shell word, then how many were left out. Every entry is on
/// one line whatever the name holds. A name decoded lossily (git's bytes were
/// not UTF-8, so it holds U+FFFD) matches nothing a command could spell, and is
/// reported instead.
fn path_commands(paths: &[String], command: impl Fn(&str) -> String) -> String {
    let shown: Vec<String> = paths
        .iter()
        .take(LISTED_PATHS)
        .map(|path| {
            if path.contains(char::REPLACEMENT_CHARACTER) {
                format!(
                    "{path:?} has a name that is not valid UTF-8, so no command can name it: \
                     report it with `loom stage block`"
                )
            } else {
                command(&shell_path(path))
            }
        })
        .collect();
    match paths.len().saturating_sub(LISTED_PATHS) {
        0 => shown.join("; "),
        more => format!("{}; and {more} more", shown.join("; ")),
    }
}

/// Whether `glob` covers `path` the way a sandbox deny rule does: it matches
/// the path or a directory above it, and a glob with no `/` also matches a
/// name at any depth. A trailing `/` names the directory (`doc/specs/` covers
/// `doc/specs/a.md`), and case is ignored, as on a case-insensitive
/// filesystem. An invalid glob covers nothing; plan validation rejects those
/// before a stage runs.
fn covers(glob: &str, path: &str) -> bool {
    let glob = glob.trim().trim_start_matches("./").trim_end_matches('/');
    let Ok(pattern) = Pattern::new(glob) else {
        return false;
    };
    let options = MatchOptions {
        case_sensitive: false,
        ..MatchOptions::new()
    };
    let directories = path.match_indices('/').map(|(at, _)| &path[..at]);
    if directories
        .chain(std::iter::once(path))
        .any(|prefix| pattern.matches_with(prefix, options))
    {
        return true;
    }
    !glob.contains('/')
        && path
            .split('/')
            .any(|name| pattern.matches_with(name, options))
}

#[cfg(test)]
mod tests {
    use super::{covers, refusal_reason, LISTED_PATHS};

    #[test]
    fn a_glob_covers_a_path_below_it_and_the_directory_itself() {
        assert!(covers("doc/specs/**", "doc/specs/product/a.md"));
        assert!(covers("doc/specs", "doc/specs/product/a.md"));
        assert!(covers("./doc/specs/**", "doc/specs/a.md"));
        assert!(covers(
            ".github/workflows/*.yml",
            ".github/workflows/ci.yml"
        ));
    }

    #[test]
    fn a_trailing_slash_names_the_directory() {
        assert!(covers("doc/specs/", "doc/specs/a.md"));
        assert!(covers("./doc/specs/", "doc/specs/product/a.md"));
        assert!(covers("doc/specs/**/", "doc/specs/a.md"));
        assert!(covers("doc/specs//", "doc/specs/a.md"));
        assert!(covers("build/", "web/build/out.js"));
        assert!(!covers("doc/specs/", "doc/plans/a.md"));
    }

    #[test]
    fn a_glob_without_a_slash_matches_a_name_at_any_depth() {
        assert!(covers(".env", "app/config/.env"));
        assert!(covers("*.lock", "web/bun.lock"));
    }

    #[test]
    fn a_glob_matches_ignoring_ascii_case() {
        assert!(covers("doc/specs/**", "DOC/Specs/a.md"));
        assert!(covers(
            ".GitHub/workflows/*.yml",
            ".github/workflows/CI.yml"
        ));
        assert!(covers(".ENV", "app/.env"));
        assert!(covers("*.LOCK", "web/bun.lock"));
    }

    #[test]
    fn a_glob_leaves_other_paths_and_invalid_globs_alone() {
        assert!(!covers("doc/specs/**", "doc/plans/a.md"));
        assert!(!covers("doc/specs/**", "src/doc/specs.rs"));
        assert!(!covers("a/[", "a/b"));
    }

    #[test]
    fn a_path_decoded_lossily_is_reported_instead_of_given_a_command() {
        let lossy = "doc/specs/a\u{fffd}.md".to_string();

        let reason = refusal_reason("abc", std::slice::from_ref(&lossy), &[]);

        assert!(!reason.contains("git --literal-pathspecs"), "{reason}");
        assert!(
            reason.contains(&format!("{lossy:?} has a name that is not valid UTF-8")),
            "{reason}"
        );
        assert!(reason.contains("loom stage block"), "{reason}");
        assert!(!reason.contains('\n'), "{reason}");
    }

    #[test]
    fn paths_past_the_limit_are_counted_and_not_given_a_command() {
        let altered: Vec<String> = (0..LISTED_PATHS + 3)
            .map(|n| format!("doc/specs/{n}.md"))
            .collect();

        let reason = refusal_reason("abc", &[], &altered);

        assert_eq!(reason.matches("--source=abc").count(), LISTED_PATHS);
        assert!(reason.contains("; and 3 more"), "{reason}");
        assert!(!reason.contains("doc/specs/10.md"), "{reason}");
    }

    #[test]
    fn no_punctuation_abuts_a_printed_command() {
        let authored: Vec<String> = (0..=LISTED_PATHS)
            .map(|n| format!("doc/specs/a{n}.md"))
            .collect();
        let altered = vec!["doc/specs/b.md".to_string()];
        let cases: [(&[String], &[String]); 3] =
            [(&authored, &[]), (&[], &altered), (&authored, &altered)];

        for (authored, altered) in cases {
            let reason = refusal_reason("abc", authored, altered);

            let mut checked = 0;
            for (at, marker) in reason.match_indices("-- '") {
                let word_at = at + marker.len();
                let word_end = word_at + reason[word_at..].find('\'').unwrap() + 1;
                let next = reason[word_end..].chars().next();
                assert!(
                    next.is_none_or(|ch| ch.is_whitespace() || ch == ';'),
                    "{next:?} abuts a command in: {reason}"
                );
                checked += 1;
            }
            assert!(checked > 0, "{reason}");
        }
    }

    #[test]
    fn a_name_with_a_newline_keeps_the_refusal_on_one_line() {
        let authored = vec!["doc/specs/a\nb.md".to_string()];

        let reason = refusal_reason("abc", &authored, &[]);

        assert!(reason.contains("-- $'doc/specs/a\\nb.md'"), "{reason}");
        assert_eq!(reason.lines().count(), 1, "{reason}");
    }
}
