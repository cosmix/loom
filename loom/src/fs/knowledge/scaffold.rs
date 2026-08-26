//! Helpers behind `KnowledgeDir::append_target`'s scaffolding and self-heal
//! logic (loom-bugs.txt BUG 3: a new topic file got a generic `# Title` +
//! stub blurb stamped on top of content that already carried its own).
//!
//! Extracted from `dir.rs` to keep that file under the 400-line
//! maintainability limit, the same reason `splice.rs` (BUG 2) exists as its
//! own module.

use super::index;
use super::templates;
use super::types::KnowledgeTarget;

/// `(category dir name, slug)` for a tier-2 topic target, `None` for tier-1 —
/// the generic-stub scaffolding and self-heal in
/// [`super::dir::KnowledgeDir::append_target`] apply only to topics; tier-1
/// files always keep their curated template.
pub(super) fn topic_category_and_slug(target: &KnowledgeTarget) -> Option<(&str, &str)> {
    match target {
        KnowledgeTarget::Tier1(_) => None,
        KnowledgeTarget::Topic { category, slug } => Some((category.dir_name(), slug.as_str())),
    }
}

/// Whether `content` already carries its own `# ` title (after any leading
/// blank lines) — a freshly scaffolded topic must not get the generic stub
/// stamped on top of content that brings its own.
pub(super) fn has_own_title(content: &str) -> bool {
    content.trim_start().starts_with("# ")
}

pub(super) fn ensure_trailing_newline(mut s: String) -> String {
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Content for a target file that does not exist yet: verbatim when it is a
/// topic bringing its own `# ` title (no duplicate H1), otherwise the
/// scaffold with `content` appended below it, as before.
pub(super) fn new_topic_content(default: &str, content: &str, is_topic: bool) -> String {
    if is_topic && has_own_title(content) {
        return ensure_trailing_newline(content.to_string());
    }
    append_below(default, content)
}

/// Append `content` below `base`, separated by exactly one blank line.
pub(super) fn append_below(base: &str, content: &str) -> String {
    if base.ends_with('\n') {
        format!("{base}\n{content}\n")
    } else {
        format!("{base}\n\n{content}\n")
    }
}

/// If `base`'s header is EXACTLY the generic stub `append_target` stamps for
/// a nonexistent topic (first `# ` line equals `title_case(slug)`, first `> `
/// line equals `scaffold_blurb(category)`, nothing but blank lines around
/// them), strip that header and return what remains. `None` if either half
/// does not match exactly — the caller then appends as usual and changes
/// nothing, rather than risk mangling hand-shaped content.
pub(super) fn strip_stub_header(base: &str, category: &str, slug: &str) -> Option<String> {
    let expected_title = format!("# {}", index::title_case(slug));
    let expected_blurb = format!("> {}", templates::scaffold_blurb(category));
    let lines: Vec<&str> = base.lines().collect();

    if lines.first().copied() != Some(expected_title.as_str()) {
        return None;
    }
    let mut idx = 1;
    while lines.get(idx).is_some_and(|line| line.is_empty()) {
        idx += 1;
    }
    if lines.get(idx).copied() != Some(expected_blurb.as_str()) {
        return None;
    }
    idx += 1;
    while lines.get(idx).is_some_and(|line| line.is_empty()) {
        idx += 1;
    }

    let remainder = lines[idx..].join("\n");
    Some(if remainder.is_empty() {
        remainder
    } else {
        ensure_trailing_newline(remainder)
    })
}

/// Split `content` into its leading H1 title (and, if present after only
/// blank lines, the following `> ` blurb line) and whatever comes after —
/// neither half carries a trailing newline. `content` has no `# ` title:
/// returns an empty header and `content` itself (trailing newline trimmed)
/// as the body. Never matches a `## ` (or deeper) heading — `has_own_title`
/// is H1-specific, same as the rest of this module's stub-scaffolding logic.
pub(super) fn split_leading_header(content: &str) -> (String, String) {
    if !has_own_title(content) {
        return (String::new(), content.trim_end_matches('\n').to_string());
    }
    let lines: Vec<&str> = content.lines().collect();
    let mut idx = 0;
    while lines.get(idx).is_some_and(|line| line.is_empty()) {
        idx += 1;
    }
    let title_idx = idx;
    let mut blurb_idx = title_idx + 1;
    while lines.get(blurb_idx).is_some_and(|line| line.is_empty()) {
        blurb_idx += 1;
    }
    let header_end = if lines
        .get(blurb_idx)
        .is_some_and(|line| line.starts_with("> "))
    {
        blurb_idx + 1
    } else {
        title_idx + 1
    };

    let header = lines[title_idx..header_end].join("\n");
    let mut body_idx = header_end;
    while lines.get(body_idx).is_some_and(|line| line.is_empty()) {
        body_idx += 1;
    }
    let body = lines[body_idx..].join("\n");
    (header, body)
}

/// Splice `content`'s own header in place of a stripped generic stub header.
/// Only the header (title + optional blurb) is transplanted: `rest` (the
/// preserved pre-existing body) keeps its original position ahead of
/// `content`'s own body, which is appended at the true end — `update` is
/// append-only, so nothing pre-existing may be displaced below
/// newly-submitted content.
///
/// If `rest` itself still carries a duplicate real header (the shape BUG 3
/// actually produced before this fix existed: stub header, then the
/// author's own header, appended together by one buggy call), that
/// duplicate is dropped too — remediating such a file by resubmitting
/// corrected content must not leave the real title/blurb in the file twice.
pub(super) fn heal_stub_header(content: &str, rest: &str) -> String {
    let (incoming_header, incoming_body) = split_leading_header(content);
    let (_, rest_body) = split_leading_header(rest);

    let mut result = ensure_trailing_newline(incoming_header);
    if !rest_body.is_empty() {
        result = append_below(&result, &rest_body);
    }
    if !incoming_body.is_empty() {
        result = append_below(&result, &incoming_body);
    }
    result
}
