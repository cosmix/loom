//! `TI-edit-<path>`: assertion lines a base test file lost, removed or changed
//! with no identical line added back in that file. `TI-ratchet-<path>`: a
//! ratchet file whose content differs from base.

use anyhow::Result;
use regex::Regex;
use std::collections::{BTreeSet, HashMap};

use super::count::{matchers, BaseTestFile};
use super::{current_sha256, EventKind, IntegrityEvent};
use crate::git::worktree::WorktreeGit;
use crate::verify::contracts::changes::git;
use crate::verify::review::fingerprint::ChangeFingerprint;

/// One event per changed base test file whose diff from base loses an
/// assertion line.
pub(super) fn assertion_edits(
    repo: &WorktreeGit,
    changes: &ChangeFingerprint,
    base_files: &[BaseTestFile],
) -> Result<Vec<IntegrityEvent>> {
    let mut events = Vec::new();
    for file in base_files {
        if !changes.files.contains_key(&file.path) {
            continue;
        }
        let pathspec = format!(":(literal){}", file.path);
        let diff = git(
            repo,
            &[
                "diff",
                "--no-color",
                "--no-ext-diff",
                "--no-textconv",
                "--text",
                "--unified=0",
                &changes.base,
                "--",
                &pathspec,
            ],
        )?;
        let assertion = &matchers(file.profile).assertion;
        let diff = String::from_utf8_lossy(&diff);
        let lost = lost_assertions(&diff, assertion, file.profile.name);
        if !lost.is_empty() {
            let language = Some(file.profile.name.to_string());
            events.push(file_event(
                EventKind::AssertionEdit,
                &file.path,
                language,
                changes,
                lost,
            ));
        }
    }
    Ok(events)
}

/// One event per ratchet file the worktree changed from base.
pub(super) fn ratchet_events(
    changes: &ChangeFingerprint,
    ratchet_files: &[String],
) -> Vec<IntegrityEvent> {
    ratchet_files
        .iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|path| changes.files.contains_key(path.as_str()))
        .map(|path| file_event(EventKind::Ratchet, path, None, changes, Vec::new()))
        .collect()
}

/// The trimmed text of every removed line matching `assertion`, less one
/// occurrence per identical added line: a moved line is added back, a changed
/// or deleted one is not.
///
/// A hunk whose removed and added lines are equal once formatting is
/// normalized (see `normalize`) is a pure reformat, such as a formatter
/// wrapping a call: it contributes nothing, so the unchanged assertion is not
/// reported as lost.
fn lost_assertions(diff: &str, assertion: &Regex, profile: &str) -> Vec<String> {
    let mut removed = Vec::new();
    let mut added: HashMap<&str, usize> = HashMap::new();
    for (old, new) in hunks(diff) {
        if !old.is_empty()
            && normalize(&old.join("\n"), profile) == normalize(&new.join("\n"), profile)
        {
            continue;
        }
        removed.extend(
            old.into_iter()
                .filter(|text| assertion.is_match(text))
                .map(str::trim),
        );
        for text in new {
            *added.entry(text.trim()).or_default() += 1;
        }
    }
    removed
        .into_iter()
        .filter(|text| match added.get_mut(text) {
            Some(count) if *count > 0 => {
                *count -= 1;
                false
            }
            _ => true,
        })
        .map(str::to_string)
        .collect()
}

/// The removed and added lines (prefix stripped) of each hunk in `diff`.
fn hunks(diff: &str) -> Vec<(Vec<&str>, Vec<&str>)> {
    let mut hunks: Vec<(Vec<&str>, Vec<&str>)> = Vec::new();
    let mut in_hunk = false;
    for line in diff.lines() {
        if line.starts_with("diff ") {
            in_hunk = false;
        } else if line.starts_with("@@") {
            in_hunk = true;
            hunks.push((Vec::new(), Vec::new()));
        } else if !in_hunk {
            continue;
        } else if let (Some(text), Some(hunk)) = (line.strip_prefix('-'), hunks.last_mut()) {
            hunk.0.push(text);
        } else if let (Some(text), Some(hunk)) = (line.strip_prefix('+'), hunks.last_mut()) {
            hunk.1.push(text);
        }
    }
    hunks
}

/// `text` without whitespace outside string literals and without trailing
/// commas before a closing bracket, so wrapping and trailing-comma changes
/// compare equal. String contents stay verbatim. Python keeps trailing commas
/// because `(1,)` is a tuple, not `(1)`. Rust skips `'` as a delimiter
/// (lifetimes, char literals); raw strings and comments are not handled.
fn normalize(text: &str, profile: &str) -> String {
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
    changes: &ChangeFingerprint,
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
        current_sha256: current_sha256(changes, path),
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
