//! Page bodies: a summary file's blocks with citations, marks, source anchors and plan links.

use std::path::{Path, PathBuf};

use crate::plan_server::concept::key::colour_of;
use crate::plan_server::concept::links::implemented_by;
use crate::plan_server::concept::page::{page_key_of, PageContext};
use crate::plan_server::concept::state::out_of_date_reason;
use crate::plan_server::wire::concept::{
    Block, Citation, DocLink, KeyChip, Mark, Sentence, SourceAnchor,
};
use crate::planning::kinds::DocKind;
use crate::planning::project::Project;
use crate::planning::summary::evaluate::{sentences_of, Evaluation, SummaryState};
use crate::planning::summary::format::{self, LinkTarget, SummaryFile};
use crate::planning::summary::store::{is_overview, source_of, summary_id_for};

/// Which kind of summary a body belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    /// An overview: its sentences cite sentences of document summaries (level 0).
    Overview,
    /// A document summary: its sentences cite the source document (level 1).
    Document,
}

struct Cx<'a> {
    project: &'a Project,
    eval: &'a Evaluation,
    chips: &'a [KeyChip],
    level: Level,
    ctx: &'a PageContext,
}

/// A cited document and whether one of its cited sentences no longer exists.
struct Cited {
    path: PathBuf,
    missing: bool,
}

pub(crate) fn blocks(
    project: &Project,
    file: &SummaryFile,
    eval: &Evaluation,
    chips: &[KeyChip],
    level: Level,
    ctx: &PageContext,
) -> Vec<Block> {
    let cx = Cx {
        project,
        eval,
        chips,
        level,
        ctx,
    };
    let section = match level {
        Level::Document => section_number(project, &file.front.of),
        Level::Overview => None,
    };
    file.blocks
        .iter()
        .map(|block| match block {
            format::Block::Heading { level, text, .. } => Block::Heading {
                level: *level,
                text: text.clone(),
            },
            format::Block::Paragraph(sentences) => Block::Paragraph {
                sentences: sentences
                    .iter()
                    .enumerate()
                    .map(|(index, one)| sentence(&cx, file, one, section.filter(|_| index == 0)))
                    .collect(),
            },
        })
        .collect()
}

fn section_number(project: &Project, of: &str) -> Option<u32> {
    match project.tree.get(Path::new(of))?.kind {
        DocKind::ProductSection { number } => Some(number),
        _ => None,
    }
}

fn sentence(cx: &Cx, file: &SummaryFile, one: &format::Sentence, number: Option<u32>) -> Sentence {
    let citations = citations(&one.links);
    let (rests_on, marks) = if citations.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        match cx.level {
            Level::Document => document_level(cx, file, &citations),
            Level::Overview => overview_level(cx, &citations),
        }
    };
    Sentence {
        id: one.id.clone(),
        text: one.text.clone(),
        marks,
        citations,
        rests_on,
        plans: number
            .map(|number| implemented_by(cx.project, number))
            .unwrap_or_default(),
    }
}

fn citations(links: &[format::Link]) -> Vec<Citation> {
    links
        .iter()
        .filter_map(|link| match &link.resolved {
            LinkTarget::Sentence { summary, id } => Some(Citation::Sentence {
                summary: summary.clone(),
                id: id.clone(),
            }),
            LinkTarget::Source { path, anchor } => Some(Citation::Source {
                path: path.to_string_lossy().into_owned(),
                anchor: anchor.clone(),
            }),
            LinkTarget::Unresolved(_) => None,
        })
        .collect()
}

fn document_level(
    cx: &Cx,
    file: &SummaryFile,
    citations: &[Citation],
) -> (Vec<SourceAnchor>, Vec<Mark>) {
    let mut anchors = Vec::new();
    for citation in citations {
        if let Citation::Source { path, anchor } = citation {
            push_anchor(&mut anchors, path, anchor);
        }
    }
    let own = Cited {
        path: PathBuf::from(&file.front.of),
        missing: false,
    };
    (anchors, mark(cx, &own).into_iter().collect())
}

fn overview_level(cx: &Cx, citations: &[Citation]) -> (Vec<SourceAnchor>, Vec<Mark>) {
    let mut anchors = Vec::new();
    let mut cited: Vec<Cited> = Vec::new();
    for citation in citations {
        let (path, missing) = match citation {
            Citation::Source { path, anchor } => {
                push_anchor(&mut anchors, path, anchor);
                (Some(PathBuf::from(path)), false)
            }
            Citation::Sentence { summary, id } => {
                let found = cited_sentence(cx.eval, summary, id);
                for link in found.iter().flat_map(|one| &one.links) {
                    if let LinkTarget::Source { path, anchor } = &link.resolved {
                        push_anchor(&mut anchors, &path.to_string_lossy(), anchor);
                    }
                }
                (summary_source(cx, summary), found.is_none())
            }
        };
        let Some(path) = path else { continue };
        match cited.iter_mut().find(|one| one.path == path) {
            Some(one) => one.missing |= missing,
            None => cited.push(Cited { path, missing }),
        }
    }
    (
        anchors,
        cited.iter().filter_map(|one| mark(cx, one)).collect(),
    )
}

fn push_anchor(anchors: &mut Vec<SourceAnchor>, path: &str, anchor: &str) {
    if !anchors.iter().any(|a| a.path == path && a.anchor == anchor) {
        anchors.push(SourceAnchor {
            path: path.to_owned(),
            anchor: anchor.to_owned(),
        });
    }
}

fn cited_sentence<'a>(
    eval: &'a Evaluation,
    summary: &str,
    id: &str,
) -> Option<&'a format::Sentence> {
    let file = eval.summaries.get(summary)?.file.as_ref()?;
    sentences_of(file).find(|one| one.id == id)
}

fn summary_source(cx: &Cx, summary: &str) -> Option<PathBuf> {
    let known = cx.eval.summaries.get(summary)?.source.clone();
    known.or_else(|| source_of(cx.project, summary))
}

/// The mark of a cited document: its colour in the scope (0 outside) and, for a document with
/// a summary, that summary's out-of-date reason. A cited sentence that no longer exists dashes
/// the mark whatever the summary's state.
fn mark(cx: &Cx, cited: &Cited) -> Option<Mark> {
    let doc = doc_link(cx.project, &cited.path)?;
    // The vision and metaplans carry the overview's id but have no summary of their own.
    let out_of_date = if is_overview(&doc.id) {
        None
    } else {
        let state = cx.eval.summaries.get(&doc.id).map(|summary| summary.state);
        let missing = cited.missing.then_some(SummaryState::Stale);
        state
            .and_then(|state| out_of_date_reason(&doc.id, state, cx.ctx))
            .or_else(|| missing.and_then(|state| out_of_date_reason(&doc.id, state, cx.ctx)))
    };
    Some(Mark {
        colour: colour_of(cx.chips, &doc.path).unwrap_or(0),
        doc,
        out_of_date,
    })
}

/// The link to a document's concept page: the vision and metaplans stand for their overview.
/// `None` for documents concept mode has no page for.
pub(crate) fn doc_link(project: &Project, path: &Path) -> Option<DocLink> {
    let doc = project.docs.get(path)?;
    let (id, page) = match summary_id_for(project, path) {
        Some(id) => {
            let page = page_key_of(&id);
            (id, page)
        }
        None => match doc.doc.kind {
            DocKind::Vision => ("product".to_owned(), "product".to_owned()),
            DocKind::Metaplan => ("engineering".to_owned(), "engineering".to_owned()),
            _ => return None,
        },
    };
    Some(DocLink {
        id,
        title: doc.title.clone(),
        path: path.to_string_lossy().into_owned(),
        page,
    })
}
