//! Page bodies: a summary file's blocks with citations, marks, source anchors and plan links.

use std::path::{Path, PathBuf};

use crate::plan_server::concept::key::colour_of;
use crate::plan_server::concept::links::implemented_by;
use crate::plan_server::concept::page::{page_key_of, PageContext, PageKey};
use crate::plan_server::concept::state::out_of_date_reason;
use crate::plan_server::wire::concept::{
    Block, Citation, DocLink, KeyChip, Mark, Sentence, SourceAnchor,
};
use crate::planning::kinds::DocKind;
use crate::planning::project::Project;
use crate::planning::summary::evaluate::{sentences_of, Evaluation, SummaryState};
use crate::planning::summary::format::{self, LinkTarget, SummaryFile};
use crate::planning::summary::store::{
    is_overview, metaplan_feature, source_of, summary_id_for, vision_feature,
};

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
    feature: Option<&'a str>,
}

/// A cited document and whether one of its cited sentences no longer exists.
struct Cited {
    path: PathBuf,
    missing: bool,
}

/// Render a summary body with its citations and feature-local plan links.
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
        feature: project
            .tree
            .get(Path::new(&file.front.of))
            .and_then(|doc| doc.kind.feature()),
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
        DocKind::ProductSection { number, .. } => Some(number),
        _ => None,
    }
}

fn sentence(cx: &Cx, file: &SummaryFile, one: &format::Sentence, number: Option<u32>) -> Sentence {
    let citations = citations(cx, &one.links);
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
            .zip(cx.feature)
            .map(|(number, feature)| implemented_by(cx.project, feature, number))
            .unwrap_or_default(),
    }
}

/// The sentence's citations. A cited summary whose document is gone has no page to load and no
/// mark, so its citation goes the way of an unresolved link; [`mark`] applies the same test.
fn citations(cx: &Cx, links: &[format::Link]) -> Vec<Citation> {
    links
        .iter()
        .filter_map(|link| match &link.resolved {
            LinkTarget::Sentence { summary, id } => summary_source(cx, summary)
                .and_then(|path| doc_link(cx.project, &path))
                .map(|_| Citation::Sentence {
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
    let doc = match cx.level {
        Level::Overview => overview_stand_in(doc),
        Level::Document => doc,
    };
    // Metaplans and the vision, when an overview cites them, carry the overview's id: the overview
    // cites their passages directly.
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

/// An overview cites the passages of the vision and of the metaplans directly, so their links
/// there stand for the product and engineering overviews, not for their own summary pages.
fn overview_stand_in(mut doc: DocLink) -> DocLink {
    let id = match (vision_feature(&doc.id), metaplan_feature(&doc.id)) {
        (Some(feature), _) => format!("overview/{feature}/product"),
        (None, Some(feature)) => format!("overview/{feature}/engineering"),
        (None, None) => return doc,
    };
    doc.id = id.clone();
    doc.page = id;
    doc
}

/// The link to a document's concept page: a document with a summary id, vision and metaplan
/// included, links its own summary page; a metaplan with no summary id stands for its overview.
/// `None` for documents concept mode has no page for, and for a document whose page key the
/// web client would reject (a slug past the 128-character page name).
pub(crate) fn doc_link(project: &Project, path: &Path) -> Option<DocLink> {
    let doc = project.docs.get(path)?;
    let (id, page) = match summary_id_for(project, path) {
        Some(id) => {
            let page = page_key_of(project, &id)?;
            (id, page)
        }
        None => match &doc.doc.kind {
            DocKind::Metaplan {
                feature: Some(feature),
            } => {
                let id = format!("overview/{feature}/engineering");
                (id.clone(), id)
            }
            _ => return None,
        },
    };
    PageKey::parse(&page)?;
    Some(DocLink {
        id,
        title: doc.title.clone(),
        path: path.to_string_lossy().into_owned(),
        page,
    })
}
