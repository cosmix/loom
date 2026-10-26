//! Cross-references between documents of the doc/ tree.

use std::path::{Component, Path, PathBuf};

use crate::fs::knowledge::splice::fence_mask;
use crate::plan::schema::extract_prefixed_paths;
use crate::planning::discover::{DocTree, SUMMARIES};
use crate::planning::kinds::DocId;
use crate::planning::sections::DocSections;
use crate::planning::slug::plan_slug;

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
        for raw in backticked_plans(&remaining) {
            let target = plan_file_target(&raw, tree);
            refs.push(Reference {
                form: RefForm::PlanFile,
                raw,
                line: line_no,
                target,
            });
        }
    }
    refs
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
        if path.parent() != Some(Path::new("doc/plans")) {
            return None;
        }
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
    let mut result = if path.is_empty() {
        doc_rel.to_path_buf()
    } else {
        doc_rel.parent()?.to_path_buf()
    };
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => result.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            _ => return None,
        }
    }
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
