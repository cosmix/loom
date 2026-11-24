//! Cross-references between documents of the doc/ tree.

use std::path::{Component, Path, PathBuf};

use crate::fs::knowledge::splice::fence_mask;
use crate::fs::lexical::normalize_lexically;
use crate::plan::schema::extract_prefixed_paths;
use crate::planning::discover::{DocTree, SUMMARIES};
use crate::planning::kinds::DocId;
use crate::planning::metaplan::{entries, is_metaplan};
use crate::planning::sections::DocSections;
use crate::planning::slug::{plan_slug, plan_state};

/// The syntax used to name a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefForm {
    /// A Markdown link with a relative destination.
    RelativeLink,
    /// A repository-relative `doc/` path in text.
    PathToken,
    /// A backticked plan filename.
    PlanFile,
}

/// A reference and its resolved document and optional heading anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// The syntax used for this reference.
    pub form: RefForm,
    /// The destination or token as written.
    pub raw: String,
    /// Source line, zero-based.
    pub line: usize,
    /// Resolved document and heading anchor, or `None` if broken.
    pub target: Option<(DocId, Option<String>)>,
}

/// Extract document references without accessing the filesystem. Fenced code blocks hold
/// examples and templates, so their lines yield no reference; line numbers are unchanged.
pub fn references(
    doc_rel: &Path,
    text: &str,
    tree: &DocTree,
    sections: &dyn Fn(&Path) -> Option<DocSections>,
) -> Vec<Reference> {
    let lines: Vec<&str> = text.lines().collect();
    let mask = fence_mask(&lines);
    let planned = planned_entry_lines(doc_rel, text);
    let mut refs = Vec::new();
    for (line_no, line) in lines.iter().enumerate().filter(|(i, _)| !mask[*i]) {
        let (links, remaining) = links_on_line(line);
        for raw in links {
            if let Some(path) = link_path(doc_rel, &raw) {
                let anchor = raw.split_once('#').map(|(_, a)| a.to_owned());
                let target = resolve(&path, anchor, tree, sections);
                refs.push(Reference {
                    form: RefForm::RelativeLink,
                    raw,
                    line: line_no,
                    target,
                });
            }
        }
        let tokens = path_tokens(&remaining);
        for raw in tokens
            .into_iter()
            .filter(|raw| !names_no_document(raw, tree))
        {
            let target = resolve(Path::new(&raw), None, tree, sections);
            refs.push(Reference {
                form: RefForm::PathToken,
                raw,
                line: line_no,
                target,
            });
        }
        let declared = planned.contains(&line_no);
        refs.extend(plan_file_refs(&remaining, line_no, declared, tree));
    }
    refs
}

/// Backticked plan filenames on one line. On a line that declares a metaplan entry, a plan
/// that does not exist yet is skipped.
fn plan_file_refs(text: &str, line: usize, declared: bool, tree: &DocTree) -> Vec<Reference> {
    backticked_plans(text)
        .into_iter()
        .map(|raw| (plan_file_target(&raw, tree), raw))
        .filter(|(target, _)| target.is_some() || !declared)
        .map(|(target, raw)| Reference {
            form: RefForm::PlanFile,
            raw,
            line,
            target,
        })
        .collect()
}

/// Zero-based lines of a metaplan's entry headings that name a plan file. Such a heading
/// declares the plan before it is written, so its filename is no reference until the plan
/// exists; the metaplan's missing-plan gap covers it meanwhile.
fn planned_entry_lines(doc_rel: &Path, text: &str) -> Vec<usize> {
    if !is_metaplan(doc_rel) {
        return Vec::new();
    }
    entries(text)
        .into_iter()
        .filter(|entry| entry.plan_file.is_some())
        .map(|entry| entry.line)
        .collect()
}

fn plan_file_target(raw: &str, tree: &DocTree) -> Option<(DocId, Option<String>)> {
    let plan = tree.plan(&plan_slug(raw)?)?;
    tree.get(&plan.path).map(|doc| (doc.id.clone(), None))
}

/// Whether a path token names something that is never a single document: the summary store,
/// a dotfile (discovery skips both) or a directory that has documents under it.
fn names_no_document(raw: &str, tree: &DocTree) -> bool {
    let path = Path::new(raw);
    let dotted = path.components().any(|part| match part {
        Component::Normal(name) => name.to_str().is_some_and(|name| name.starts_with('.')),
        _ => false,
    });
    path.starts_with(SUMMARIES) || dotted || tree.has_dir(path)
}

fn resolve(
    path: &Path,
    anchor: Option<String>,
    tree: &DocTree,
    sections: &dyn Fn(&Path) -> Option<DocSections>,
) -> Option<(DocId, Option<String>)> {
    let doc = tree.get(path).or_else(|| {
        plan_state(path)?;
        let slug = plan_slug(path.file_name()?.to_str()?)?;
        tree.get(&tree.plan(&slug)?.path)
    })?;
    if let Some(ref value) = anchor {
        let headings = sections(&doc.path)?;
        if !headings
            .headings
            .iter()
            .any(|heading| heading.anchor == *value)
        {
            return None;
        }
    }
    Some((doc.id.clone(), anchor))
}

fn link_path(doc_rel: &Path, raw: &str) -> Option<PathBuf> {
    let path = raw.split('#').next().unwrap_or_default();
    if path.starts_with('/') || path.starts_with("//") || path.contains(':') {
        return None;
    }
    let joined = if path.is_empty() {
        doc_rel.to_path_buf()
    } else {
        doc_rel.parent()?.join(path)
    };
    // A link climbing above the repository root names no document.
    let result = normalize_lexically(&joined)?;
    result.starts_with("doc").then_some(result)
}

fn links_on_line(line: &str) -> (Vec<String>, String) {
    let mut links = Vec::new();
    let mut remaining = String::new();
    let mut rest = line;
    while let Some(start) = rest.find('[') {
        remaining.push_str(&rest[..start]);
        let candidate = &rest[start..];
        let Some(mid) = candidate.find("](") else {
            remaining.push_str(candidate);
            return (links, remaining);
        };
        let Some(end) = candidate[mid + 2..].find(')') else {
            remaining.push_str(candidate);
            return (links, remaining);
        };
        let end = mid + 2 + end;
        links.push(candidate[mid + 2..end].to_owned());
        remaining.push(' ');
        rest = &candidate[end + 1..];
    }
    remaining.push_str(rest);
    (links, remaining)
}

/// `doc/` paths in `text` that start at a token start (optionally after `./` or
/// `../`), via the shared prefixed-path extractor. Everything from the first `:` on (a line
/// or range suffix) is cut; directory mentions, globs and `$`/`{}` placeholders are dropped.
fn path_tokens(text: &str) -> Vec<String> {
    extract_prefixed_paths(text, "doc/")
        .into_iter()
        .filter(|path| at_token_start(text, path))
        .map(|path| match path.split_once(':') {
            Some((base, _)) => base.to_owned(),
            None => path,
        })
        .filter(|path| !path.ends_with('/') && !path.contains(['*', '?', '$', '{', '}']))
        .collect()
}

/// Whether `path` occurs in `text` right after a token boundary, ignoring any
/// `./` or `../` segments in front of it. The extractor cuts a token at the
/// `doc/` match, so `target/doc/x` and `src/doc/x` need this check.
fn at_token_start(text: &str, path: &str) -> bool {
    text.match_indices(path).any(|(idx, _)| {
        let mut before = &text[..idx];
        while let Some(rest) = before
            .strip_suffix("../")
            .or_else(|| before.strip_suffix("./"))
        {
            before = rest;
        }
        before.chars().next_back().is_none_or(|c| {
            c.is_whitespace()
                || matches!(
                    c,
                    '(' | ')' | '[' | ']' | '"' | '\'' | ',' | ';' | '|' | '`'
                )
        })
    })
}

fn backticked_plans(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut parts = text.split('`');
    while parts.next().is_some() {
        let Some(token) = parts.next() else {
            break;
        };
        if plan_slug(token).is_some() && !token.contains(['*', '?', '<', '>']) {
            result.push(token.to_owned());
        }
    }
    result
}

#[cfg(test)]
#[path = "xref_link_tests.rs"]
mod link_tests;
