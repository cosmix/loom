//! Repository-relative path readings of a journal entry's evidence and content.

use crate::context::rank_source::join_working_dir;
use crate::fs::memory::MemoryEntry;

/// Repository-relative readings of the entry's whitespace-free evidence items
/// and of the slashed tokens of its content.
pub(super) fn path_candidates(
    entry: &MemoryEntry,
    producer_dir: Option<&str>,
    root: Option<&str>,
) -> Vec<String> {
    let evidence = entry.evidence.iter().map(String::as_str);
    let evidence = evidence.filter(|item| !item.contains(char::is_whitespace));
    let tokens = entry.content.split_whitespace().map(trim_token);
    let tokens = tokens.filter(|token| token.contains('/'));
    let mut candidates: Vec<String> = Vec::new();
    for raw in evidence.chain(tokens) {
        for reading in readings(strip_line_span(raw), producer_dir, root) {
            if !candidates.contains(&reading) {
                candidates.push(reading);
            }
        }
    }
    candidates
}

/// A token without wrapping quotes, brackets, punctuation or a trailing `.`/`:`.
fn trim_token(token: &str) -> &str {
    let wrapper = |c: char| "`'\"()[]{}<>,;".contains(c);
    let mut current = token;
    loop {
        let next = current.trim_matches(wrapper);
        let next = next.trim_end_matches(['.', ':']);
        if next == current {
            return next;
        }
        current = next;
    }
}

/// `value` without a trailing `:<digits>` or `:<digits>-<digits>`.
pub(super) fn strip_line_span(value: &str) -> &str {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let Some((path, span)) = value.rsplit_once(':') else {
        return value;
    };
    let (start, end) = span.split_once('-').unwrap_or((span, "0"));
    if digits(start) && digits(end) {
        path
    } else {
        value
    }
}

/// A relative value read as repository-relative and against the producer's
/// `working_dir`: journals mix both, so one reading alone misses `src/x.rs`.
fn readings(value: &str, producer_dir: Option<&str>, root: Option<&str>) -> Vec<String> {
    if value.starts_with('/') {
        return repository_relative(value, root).into_iter().collect();
    }
    let both = [
        join_working_dir(None, value),
        join_working_dir(producer_dir, value),
    ];
    both.into_iter().flatten().collect()
}

/// An absolute path loses everything through the stage-id component after
/// `.worktrees/`, or else `root`; any other absolute path names nothing here.
fn repository_relative(value: &str, root: Option<&str>) -> Option<String> {
    if let Some((_, inside)) = value.split_once("/.worktrees/") {
        let (_, path) = inside.split_once('/')?;
        return join_working_dir(None, path);
    }
    let root = root?.trim_end_matches('/');
    let path = value.strip_prefix(root)?.strip_prefix('/')?;
    join_working_dir(None, path)
}
