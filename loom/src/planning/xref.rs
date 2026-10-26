//! Cross-references between documents of the doc/ tree.

use std::path::{Component, Path, PathBuf};

use crate::plan::schema::extract_prefixed_paths;
use crate::planning::discover::DocTree;
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

/// Extract document references without accessing the filesystem.
pub fn references(
    doc_rel: &Path,
    text: &str,
    tree: &DocTree,
    sections: &dyn Fn(&Path) -> Option<DocSections>,
) -> Vec<Reference> {
    let mut refs = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
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
        for raw in path_tokens(&remaining) {
            let path = Path::new(&raw);
            let target = resolve(path, None, tree, sections);
            refs.push(Reference {
                form: RefForm::PathToken,
                raw,
                line: line_no,
                target,
            });
        }
        for raw in backticked_plans(&remaining) {
            let target = plan_slug(&raw)
                .and_then(|slug| tree.plan(&slug))
                .and_then(|plan| tree.get(&plan.path))
                .map(|doc| (doc.id.clone(), None));
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
/// `../`), via the shared prefixed-path extractor. Directory mentions, globs and
/// placeholders are dropped, and a `:<digits>` line suffix is stripped.
fn path_tokens(text: &str) -> Vec<String> {
    extract_prefixed_paths(text, "doc/")
        .into_iter()
        .filter(|path| !path.ends_with('/') && !path.contains(['*', '?']))
        .filter(|path| at_token_start(text, path))
        .map(|path| match path.rsplit_once(':') {
            Some((base, suffix))
                if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) =>
            {
                base.to_owned()
            }
            _ => path,
        })
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
