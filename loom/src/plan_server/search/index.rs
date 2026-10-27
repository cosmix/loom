//! In-memory search over the current document texts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{PoisonError, RwLock};

use super::query::{literal_count, literal_snippet, parse_query, snippet, Query};
use crate::context::config::RetrievalConfig;
use crate::context::lexical::field_tokens;
use crate::context::rank::corpus::prepare_lexical;
use crate::context::schema::KnowledgeChunk;
use crate::fs::knowledge::chunker::chunk_sections;
use crate::fs::knowledge::frontmatter::Frontmatter;
use crate::plan_server::files::texts::TextStore;
use crate::plan_server::wire::files::{SearchHit, SearchResults};
use crate::planning::project::{Project, ProjectDoc};
use crate::planning::sections::HeadingSection;
use crate::planning::watch::Batch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Product,
    Plans,
    Knowledge,
    All,
}

const SNIPPET_CHARS: usize = 160;

struct IndexedChunk {
    chunk: KnowledgeChunk,
    tokens: Vec<(String, f32)>,
    title: String,
    heading_path: Vec<String>,
    anchor: String,
    scope: Scope,
}

pub struct SearchIndex {
    documents: RwLock<BTreeMap<PathBuf, Vec<IndexedChunk>>>,
}

impl SearchIndex {
    pub fn build(project: &Project, texts: &TextStore) -> SearchIndex {
        let documents = project
            .docs
            .iter()
            .filter_map(|(path, doc)| {
                index_document(path, doc, texts).map(|chunks| (path.clone(), chunks))
            })
            .collect();
        SearchIndex {
            documents: RwLock::new(documents),
        }
    }

    pub fn apply(&self, project: &Project, texts: &TextStore, batch: &Batch) {
        let mut documents = self
            .documents
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        let paths = batch
            .added
            .iter()
            .chain(&batch.changed)
            .chain(&batch.removed)
            .chain(batch.renamed.iter().flat_map(|(from, to)| [from, to]));
        for path in paths {
            documents.remove(path);
            if let Some(chunks) = project
                .docs
                .get(path)
                .and_then(|doc| index_document(path, doc, texts))
            {
                documents.insert(path.clone(), chunks);
            }
        }
    }

    pub fn query(&self, q: &str, scope: Scope) -> SearchResults {
        let documents = self
            .documents
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let chunks: Vec<&IndexedChunk> = documents
            .values()
            .flatten()
            .filter(|chunk| scope == Scope::All || chunk.scope == scope)
            .collect();
        let query = parse_query(q);
        let mut scored = score_chunks(&chunks, &query, q);
        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let truncated = scored.len() > 50;
        let hits = scored
            .into_iter()
            .take(50)
            .map(|(_, i, terms)| {
                let entry = chunks[i];
                let body = &entry.chunk.body;
                // A quoted literal is found case-sensitively, so its snippet is too.
                let snippet = match &query {
                    Query::Literal(needle) => literal_snippet(body, needle, SNIPPET_CHARS),
                    Query::Terms(_) => snippet(body, &terms, SNIPPET_CHARS),
                };
                SearchHit {
                    path: entry.chunk.file.to_string_lossy().into_owned(),
                    title: entry.title.clone(),
                    heading_path: entry.heading_path.clone(),
                    anchor: entry.anchor.clone(),
                    snippet,
                    terms,
                }
            })
            .collect();
        SearchResults { hits, truncated }
    }
}

fn score_chunks(
    chunks: &[&IndexedChunk],
    query: &Query,
    raw: &str,
) -> Vec<(f32, usize, Vec<String>)> {
    match query {
        Query::Literal(needle) => chunks
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| {
                let count = literal_count(&entry.chunk.body, needle);
                (count > 0).then(|| (count as f32, i, vec![needle.clone()]))
            })
            .collect(),
        Query::Terms(terms) if !terms.is_empty() && !chunks.is_empty() => {
            let corpus = prepare_lexical(
                terms,
                chunks.iter().map(|c| c.tokens.clone()).collect(),
                raw,
                &RetrievalConfig::default(),
            );
            chunks
                .iter()
                .enumerate()
                .filter_map(|(i, entry)| {
                    let (score, _) = corpus.score(chunks.len() as f32, i);
                    let matched: Vec<String> = terms
                        .iter()
                        .filter(|term| entry.tokens.iter().any(|(token, _)| token == *term))
                        .cloned()
                        .collect();
                    (score > 0.0).then_some((score, i, matched))
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

fn index_document(path: &Path, doc: &ProjectDoc, texts: &TextStore) -> Option<Vec<IndexedChunk>> {
    if path.starts_with("doc/loom/summaries") {
        return None;
    }
    let text = texts.current(path)?;
    let chunks = chunk_sections(path, &text.text, &Frontmatter::default());
    let places = places(&chunks, doc, &text.text);
    let scope = scope_for(path);
    Some(
        chunks
            .into_iter()
            .zip(places)
            .map(|(chunk, (heading_path, anchor))| {
                let tokens = field_tokens(&chunk);
                IndexedChunk {
                    chunk,
                    tokens,
                    title: doc.title.clone(),
                    heading_path,
                    anchor,
                    scope,
                }
            })
            .collect(),
    )
}

/// The heading path and anchor of the section a chunk belongs to.
type Place = (Vec<String>, String);

/// The place of every chunk, in chunk order. A chunk that starts `## ` takes the next level-2
/// heading of the parsed sections whose line carries the chunk's heading text, never the next
/// one by position: the chunker closes a fence on any fence line of the same character while
/// `sections_of` closes it only on one at least as long as the opener, so a `## ` line inside a
/// nested fence starts a chunk that has no heading, and pairing by position would hand it the
/// anchor of the heading after it, and every later one the anchor before theirs. A chunk with
/// no heading of its own, like the preamble, takes the place of the chunk before it.
fn places(chunks: &[KnowledgeChunk], doc: &ProjectDoc, text: &str) -> Vec<Place> {
    let lines: Vec<&str> = text.lines().collect();
    let headings = &doc.sections.headings;
    let paths = heading_paths(headings);
    let level_two: Vec<usize> = (0..headings.len())
        .filter(|&i| headings[i].level == 2 && heading_line(&lines, &headings[i]).is_some())
        .collect();
    let mut next = 0;
    let mut place: Place = (Vec::new(), String::new());
    chunks
        .iter()
        .map(|chunk| {
            if chunk.body.starts_with("## ") {
                let own = Some(chunk.heading.as_str());
                let found = level_two[next..]
                    .iter()
                    .position(|&i| heading_line(&lines, &headings[i]) == own);
                if let Some(offset) = found {
                    let index = level_two[next + offset];
                    next += offset + 1;
                    place = (paths[index].clone(), headings[index].anchor.clone());
                }
            }
            place.clone()
        })
        .collect()
}

/// The text of a `## ` heading's own line, trimmed as the chunker trims it; `None` when the
/// heading is not written with `## ` (a setext heading is not a chunk boundary).
fn heading_line<'a>(lines: &[&'a str], heading: &HeadingSection) -> Option<&'a str> {
    let line = lines.get(heading.span.start_line)?;
    Some(line.strip_prefix("## ")?.trim())
}

/// The heading path of every heading (the titles of its enclosing headings, then its own), in
/// one pass: building each path from the first heading would be quadratic in the headings, and
/// the watcher thread holds the index's write lock meanwhile.
fn heading_paths(headings: &[HeadingSection]) -> Vec<Vec<String>> {
    let mut parents: Vec<&HeadingSection> = Vec::new();
    headings
        .iter()
        .map(|heading| {
            while parents
                .last()
                .is_some_and(|parent| parent.level >= heading.level)
            {
                parents.pop();
            }
            parents.push(heading);
            parents.iter().map(|parent| parent.title.clone()).collect()
        })
        .collect()
}

fn scope_for(path: &Path) -> Scope {
    if path.starts_with("doc/product") {
        Scope::Product
    } else if path.starts_with("doc/plans") {
        Scope::Plans
    } else if path.starts_with("doc/loom/knowledge") {
        Scope::Knowledge
    } else {
        Scope::All
    }
}
