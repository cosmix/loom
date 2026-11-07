//! `TI-edit-<path>`: assertion statements a base test file lost, changed or
//! deleted, less those that moved verbatim into another collected test file
//! (`moves`). `TI-ratchet-<path>`: a ratchet file that loosens from base
//! (`ratchet_diff`).

use anyhow::Result;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::count::{self, matchers, BaseTestFile};
use super::moves::FileDelta;
use super::ratchet_diff::ratchet_loosens;
use super::statements::{assertion_statements, Statement};
use super::{current_sha256, EventKind, IntegrityEvent};
use crate::git::worktree::WorktreeGit;
use crate::models::dispute::truncate_to_byte_limit;
use crate::testrun::languages;
use crate::verify::contracts::changes::git;
use crate::verify::review::fingerprint::ChangeFingerprint;

/// Longest detail text kept per lost statement.
const MAX_STATEMENT_BYTES: usize = 600;

/// One delta per changed file a language profile covers, with the assertion
/// statements it lost and gained. `base_files` are the profiled files of the
/// base tree.
pub(super) fn file_deltas(
    repo: &WorktreeGit,
    changes: &ChangeFingerprint,
    base_files: &[BaseTestFile],
) -> Result<Vec<FileDelta>> {
    let at_base: HashSet<&str> = base_files.iter().map(|file| file.path.as_str()).collect();
    let mut base_listings: HashMap<String, Vec<String>> = HashMap::new();
    let mut deltas = Vec::new();
    for path in changes.files.keys() {
        let Some(profile) = languages::for_path(path) else {
            continue;
        };
        let existed_at_base = at_base.contains(path.as_str());
        let base = if existed_at_base {
            let object = format!("{}:{path}", changes.base);
            lossy(&git(repo, &["cat-file", "blob", &object])?)
        } else {
            String::new()
        };
        let head = match current_sha256(changes, path) {
            Some(_) => lossy(&count::read_current(repo.work_tree(), path)?),
            None => String::new(),
        };
        let assertion = &matchers(profile).assertion;
        let before = statements_of(&base, assertion, profile.name);
        let after = statements_of(&head, assertion, profile.name);
        let (lost, added) = (unmatched(&before, &after), unmatched(&after, &before));
        let siblings = if profile.name == "rust" && !added.is_empty() {
            rust_siblings(repo, changes, path, &mut base_listings)?
        } else {
            Vec::new()
        };
        deltas.push(FileDelta {
            path: path.clone(),
            profile,
            existed_at_base,
            base,
            head,
            lost,
            added,
            siblings,
        });
    }
    Ok(deltas)
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

pub(super) fn statements_of(content: &str, assertion: &Regex, profile: &str) -> Vec<Statement> {
    if content.is_empty() {
        return Vec::new();
    }
    assertion_statements(content, assertion, profile)
}

/// The statements of `side` whose keys `other` does not hold, as multisets:
/// each statement of `other` cancels one equal-keyed statement of `side`.
pub(super) fn unmatched(side: &[Statement], other: &[Statement]) -> Vec<Statement> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for statement in other {
        *counts.entry(statement.key.as_str()).or_default() += 1;
    }
    side.iter()
        .filter(|statement| match counts.get_mut(statement.key.as_str()) {
            Some(count) if *count > 0 => {
                *count -= 1;
                false
            }
            _ => true,
        })
        .cloned()
        .collect()
}

/// The `.rs` paths in `path`'s directory at head: the base tree's listing
/// (cached per directory in `base_listings`) and the changed paths there,
/// less the deleted ones.
fn rust_siblings(
    repo: &WorktreeGit,
    changes: &ChangeFingerprint,
    path: &str,
    base_listings: &mut HashMap<String, Vec<String>>,
) -> Result<Vec<String>> {
    let dir = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    if !base_listings.contains_key(dir) {
        let spec = format!("{dir}/");
        let mut args = vec!["ls-tree", "--name-only", "-z", "--full-tree", &changes.base];
        if !dir.is_empty() {
            args.extend(["--", &spec]);
        }
        let listing = git(repo, &args)?;
        let names = listing
            .split(|byte| *byte == 0)
            .filter_map(|name| std::str::from_utf8(name).ok())
            .filter(|name| name.ends_with(".rs"))
            .map(str::to_string)
            .collect();
        base_listings.insert(dir.to_string(), names);
    }
    let in_dir = |name: &str| name.rsplit_once('/').map_or("", |(dir, _)| dir) == dir;
    let changed = changes
        .files
        .keys()
        .filter(|name| name.ends_with(".rs") && in_dir(name));
    Ok(base_listings[dir]
        .iter()
        .chain(changed)
        .filter(|name| {
            current_sha256(changes, name).is_some() || !changes.files.contains_key(*name)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .cloned()
        .collect())
}

/// One event per changed base test file that kept lost statements after the
/// cancellations. `partners` names, per file, the files that cancelled some
/// of its statements; they join its pin ([`pinned_sha256`]).
pub(super) fn assertion_events(
    changes: &ChangeFingerprint,
    deltas: &[FileDelta],
    partners: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<IntegrityEvent> {
    deltas
        .iter()
        .filter(|delta| delta.existed_at_base && !delta.lost.is_empty())
        .map(|delta| {
            let detail = delta
                .lost
                .iter()
                .map(|statement| truncate_to_byte_limit(&statement.text, MAX_STATEMENT_BYTES))
                .collect();
            let pin = pinned_sha256(changes, &delta.path, partners.get(&delta.path));
            let language = Some(delta.profile.name.to_string());
            file_event(EventKind::AssertionEdit, &delta.path, language, pin, detail)
        })
        .collect()
}

/// The sha256 an accepted edit event is pinned to: the file's own, `None`
/// for a deleted file. A file with cancelling partners is pinned to the sha256
/// of the text `<own sha>\n` followed by `<partner path>\t<partner sha>\n`
/// per partner in path order, where a deleted file's sha is the literal
/// `deleted`, so an edit to a partner invalidates the acceptance.
fn pinned_sha256(
    changes: &ChangeFingerprint,
    path: &str,
    partners: Option<&BTreeSet<String>>,
) -> Option<String> {
    let own = current_sha256(changes, path);
    let Some(partners) = partners.filter(|partners| !partners.is_empty()) else {
        return own;
    };
    let sha = |path: &str| current_sha256(changes, path).unwrap_or_else(|| "deleted".to_string());
    let mut text = format!("{}\n", sha(path));
    for partner in partners {
        text.push_str(&format!("{partner}\t{}\n", sha(partner)));
    }
    Some(hex::encode(Sha256::digest(text.as_bytes())))
}

/// One event per listed ratchet file the worktree changed in a way that
/// loosens it (`ratchet_loosens`). A head file that cannot be read counts as
/// loosened.
pub(super) fn ratchet_events(
    repo: &WorktreeGit,
    changes: &ChangeFingerprint,
    ratchet_files: &[String],
) -> Result<Vec<IntegrityEvent>> {
    let mut events = Vec::new();
    for path in ratchet_files.iter().collect::<BTreeSet<_>>() {
        if !changes.files.contains_key(path.as_str()) {
            continue;
        }
        let head = current_sha256(changes, path)
            .and_then(|_| count::read_current(repo.work_tree(), path).ok());
        let base = base_blob(repo, &changes.base, path)?;
        if ratchet_loosens(base.as_deref(), head.as_deref()) {
            let pin = current_sha256(changes, path);
            events.push(file_event(EventKind::Ratchet, path, None, pin, Vec::new()));
        }
    }
    Ok(events)
}

/// The content of the regular file `path` in the tree of `base`; `None` when
/// the tree has no regular file there.
fn base_blob(repo: &WorktreeGit, base: &str, path: &str) -> Result<Option<Vec<u8>>> {
    let listing = git(repo, &["ls-tree", "-z", "--full-tree", base, "--", path])?;
    let regular = listing.split(|byte| *byte == 0).any(|entry| {
        std::str::from_utf8(entry)
            .ok()
            .and_then(|entry| entry.split_once('\t'))
            .is_some_and(|(meta, name)| {
                name == path && matches!(meta.split(' ').next(), Some("100644" | "100755"))
            })
    });
    if !regular {
        return Ok(None);
    }
    git(repo, &["cat-file", "blob", &format!("{base}:{path}")]).map(Some)
}

/// `text` without whitespace outside string literals and without trailing
/// commas before a closing bracket, so wrapping and trailing-comma changes
/// compare equal. String contents stay verbatim. Python keeps trailing commas
/// because `(1,)` is a tuple, not `(1)`. Rust skips `'` as a delimiter
/// (lifetimes, char literals); raw strings and comments are not handled.
pub(super) fn normalize(text: &str, profile: &str) -> String {
    let quotes: &[char] = if profile == "rust" {
        &['"', '`']
    } else {
        &['"', '`', '\'']
    };
    let mut out = String::new();
    let mut open: Option<char> = None;
    let mut escaped = false;
    for ch in text.chars() {
        if let Some(quote) = open {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                open = None;
            }
        } else if quotes.contains(&ch) {
            open = Some(ch);
            out.push(ch);
        } else if !ch.is_whitespace() {
            if matches!(ch, ')' | ']' | '}') && profile != "python" && out.ends_with(',') {
                out.pop();
            }
            out.push(ch);
        }
    }
    out
}

fn file_event(
    kind: EventKind,
    path: &str,
    language: Option<String>,
    current_sha256: Option<String>,
    detail: Vec<String>,
) -> IntegrityEvent {
    let prefix = match kind {
        EventKind::Ratchet => "TI-ratchet",
        _ => "TI-edit",
    };
    IntegrityEvent {
        id: format!("{prefix}-{path}"),
        kind,
        language,
        path: Some(path.to_string()),
        base: None,
        current: None,
        current_sha256,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn python_keeps_trailing_commas_other_languages_drop_them() {
        assert_ne!(
            normalize("assert f(1,) == (1,)", "python"),
            normalize("assert f(1) == (1)", "python")
        );
        assert_eq!(
            normalize("f(\n  1,\n)", "javascript"),
            normalize("f(1)", "javascript")
        );
    }

    #[test]
    fn string_contents_are_kept_verbatim() {
        assert_ne!(normalize("f(\"a b\")", "go"), normalize("f(\"ab\")", "go"));
        assert_eq!(normalize("f('a b', 1,)", "javascript"), "f('a b',1)");
        assert_eq!(normalize("f<'a>(x, 'b')", "rust"), "f<'a>(x,'b')");
    }
}
