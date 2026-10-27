//! In-memory search over the current document texts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{PoisonError, RwLock};

use super::query::{literal_count, parse_query, snippet, Query};
use crate::context::config::RetrievalConfig;
use crate::context::lexical::field_tokens;
use crate::context::rank::corpus::prepare_lexical;
use crate::context::schema::KnowledgeChunk;
use crate::fs::knowledge::chunker::chunk_sections;
use crate::fs::knowledge::frontmatter::Frontmatter;
use crate::plan_server::files::texts::TextStore;
use crate::plan_server::wire::files::{SearchHit, SearchResults};
use crate::planning::project::{Project, ProjectDoc};
use crate::planning::watch::Batch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Product,
    Plans,
    Knowledge,
    All,
}

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
                SearchHit {
                    path: entry.chunk.file.to_string_lossy().into_owned(),
                    title: entry.title.clone(),
                    heading_path: entry.heading_path.clone(),
                    anchor: entry.anchor.clone(),
                    snippet: snippet(&entry.chunk.body, &terms, 160),
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
    let lines: Vec<&str> = text.text.lines().collect();
    let mut h2 = doc.sections.headings.iter().enumerate().filter(|(_, h)| {
        h.level == 2
            && lines
                .get(h.span.start_line)
                .is_some_and(|line| line.starts_with("## "))
    });
    let scope = scope_for(path);
    Some(
        chunks
            .into_iter()
            .map(|chunk| {
                let (heading_path, anchor) = if !chunk.body.starts_with("## ") {
                    (Vec::new(), String::new())
                } else if let Some((index, heading)) = h2.next() {
                    (heading_path(doc, index), heading.anchor.clone())
                } else {
                    (Vec::new(), String::new())
                };
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

fn heading_path(doc: &ProjectDoc, index: usize) -> Vec<String> {
    let mut parents: Vec<&crate::planning::sections::HeadingSection> = Vec::new();
    for heading in &doc.sections.headings[..=index] {
        while parents
            .last()
            .is_some_and(|parent| parent.level >= heading.level)
        {
            parents.pop();
        }
        parents.push(heading);
    }
    parents
        .into_iter()
        .map(|heading| heading.title.clone())
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
