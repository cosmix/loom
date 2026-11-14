//! The findings ledger: its findings with `About:` resolved and their checks, the headings they
//! concern, and the entries they ask to revisit.

use std::path::Path;

use super::{metaplan_path, verify, Cx};
use crate::planning::findings::{self, split_tokens, Finding, FindingStatus};
use crate::planning::kinds::{section_path, vision_path};
use crate::planning::metaplan::MetaplanEntry;
use crate::planning::slug::{PlanEntry, PlanState};
use crate::planning::trace::tokens::{tokens, Token};
use crate::planning::trace::{
    CoverKind, CoverRef, FindingCheck, NodeRef, TraceEntry, TraceFinding, TraceHeading,
    TraceProblem,
};

/// The problem code of an open finding whose quote is in none of the passages it is about.
pub const EVIDENCE_MISSING: &str = "findings-evidence-missing";

/// The ledger's findings in file order with their checks, its problems, those of every token
/// that names nothing, and one for each open finding whose evidence is missing.
pub(super) fn findings(cx: &Cx, problems: &mut Vec<TraceProblem>) -> Vec<TraceFinding> {
    let path = findings::ledger_path(cx.name());
    let Some(text) = cx.text(&path) else {
        return Vec::new();
    };
    let ledger = findings::parse(text);
    problems.extend(ledger.problems.into_iter().map(|problem| TraceProblem {
        code: problem.code,
        path: path.clone(),
        line: Some(problem.line),
        message: problem.message,
    }));
    let about: Vec<Vec<NodeRef>> = ledger
        .findings
        .iter()
        .map(|finding| nodes(cx, &path, finding, &finding.about, "About:", problems))
        .collect();
    let checks = verify::checks(cx, &ledger.findings, &about);
    let found = ledger.findings.into_iter().zip(about).zip(checks);
    found
        .map(|((finding, about), check)| {
            if finding.status.is_open() && check == FindingCheck::Stale {
                problems.push(TraceProblem {
                    code: EVIDENCE_MISSING,
                    path: path.clone(),
                    line: heading_line(cx, &path, &finding.id),
                    message: format!(
                        "{}: the Evidence quote is in none of the passages About: names",
                        finding.id
                    ),
                });
            }
            let changed = match &finding.status {
                FindingStatus::Addressed(detail) => {
                    let named = split_tokens(detail);
                    nodes(cx, &path, &finding, &named, "Status: addressed", problems)
                }
                _ => Vec::new(),
            };
            TraceFinding {
                finding,
                about,
                check,
                changed,
            }
        })
        .collect()
}

/// The nodes `raw` names, each once, with a problem for each token that names nothing.
fn nodes(
    cx: &Cx,
    ledger: &Path,
    finding: &Finding,
    raw: &[String],
    line: &str,
    problems: &mut Vec<TraceProblem>,
) -> Vec<NodeRef> {
    let mut out = Vec::new();
    for token in raw {
        match resolve(cx, token) {
            Ok(node) if !out.contains(&node) => out.push(node),
            Ok(_) => {}
            Err(reason) => problems.push(TraceProblem {
                code: "about-unresolved",
                path: ledger.to_path_buf(),
                line: heading_line(cx, ledger, &finding.id),
                message: format!("{}: {line} token `{token}` {reason}", finding.id),
            }),
        }
    }
    out
}

/// The 1-based line of entry `id`'s heading in the ledger.
fn heading_line(cx: &Cx, ledger: &Path, id: &str) -> Option<usize> {
    let doc = cx.project.docs.get(ledger)?;
    let prefix = format!("{id} ");
    let heading = doc
        .sections
        .headings
        .iter()
        .find(|h| h.level == 2 && h.title.starts_with(&prefix))?;
    Some(heading.span.start_line + 1)
}

/// The node one token names in this feature, or why it names none.
fn resolve(cx: &Cx, raw: &str) -> Result<NodeRef, String> {
    let (found, _) = tokens(raw);
    let [token] = found.as_slice() else {
        return Err("is not a section, vision, metaplan or plan token".to_owned());
    };
    let name = cx.name();
    let (path, anchor) = match token {
        Token::Section(section) => (section_path(name, section.number), &section.anchor),
        Token::Vision(anchor) => (vision_path(name), anchor),
        Token::Metaplan(anchor) => (metaplan_path(name), anchor),
        Token::Plan { slug, anchor } => match cx.project.tree.plan(slug) {
            Some(plan) => (plan.path.clone(), anchor),
            None => return Err(format!("names no plan `{slug}`")),
        },
    };
    let Some(doc) = cx.project.docs.get(&path) else {
        return Err(format!("names {}, which does not exist", path.display()));
    };
    if let Some(anchor) = anchor.as_deref() {
        if doc.sections.find(anchor).is_none() {
            return Err(format!("names no `{anchor}` in {}", path.display()));
        }
    }
    let anchor = anchor.clone();
    Ok(NodeRef { path, anchor })
}

/// Attach each finding to every heading its `About:` names, and to every heading of a section
/// it names whole.
pub(super) fn attach(headings: &mut [TraceHeading], findings: &[TraceFinding]) {
    for finding in findings {
        let id = &finding.finding.id;
        for node in &finding.about {
            let named = headings.iter_mut().filter(|heading| {
                heading.path == node.path
                    && node
                        .anchor
                        .as_ref()
                        .is_none_or(|anchor| *anchor == heading.anchor)
            });
            for heading in named {
                if !heading.findings.contains(id) {
                    heading.findings.push(id.clone());
                }
            }
        }
    }
}

/// The metaplan entries in document order, each with the findings to revisit before its plan
/// runs.
pub(super) fn entries(
    cx: &Cx,
    headings: &[TraceHeading],
    findings: &[TraceFinding],
) -> Vec<TraceEntry> {
    let mut out = Vec::new();
    for (metaplan, entries) in &cx.feature.metaplans {
        for entry in entries {
            let plan = entry
                .slug
                .as_deref()
                .and_then(|slug| cx.project.tree.plan(slug));
            let started = plan.is_some_and(|plan| plan.state != PlanState::Written);
            let revisit = if started {
                Vec::new()
            } else {
                let at = Revisit {
                    metaplan,
                    entry,
                    plan,
                };
                at.ids(headings, findings)
            };
            let covers = entry.covers.iter().map(|section| NodeRef {
                path: section_path(cx.name(), section.number),
                anchor: section.anchor.clone(),
            });
            out.push(TraceEntry {
                anchor: entry.anchor.clone(),
                title: entry.heading.clone(),
                slug: entry.slug.clone(),
                plan: plan.map(|plan| plan.path.clone()),
                covers: covers.collect(),
                revisit,
            });
        }
    }
    out
}

/// An entry whose plan has not started running, and its plan when it is written.
struct Revisit<'a> {
    metaplan: &'a Path,
    entry: &'a MetaplanEntry,
    plan: Option<&'a PlanEntry>,
}

impl Revisit<'_> {
    /// The actionable findings on a heading it covers or its written plan implements, in ledger
    /// order.
    fn ids(&self, headings: &[TraceHeading], findings: &[TraceFinding]) -> Vec<String> {
        let wanted = |finding: &&TraceFinding| {
            finding.actionable()
                && headings.iter().any(|heading| {
                    heading.findings.contains(&finding.finding.id)
                        && heading.covered_by.iter().any(|cover| self.reaches(cover))
                })
        };
        let found = findings.iter().filter(wanted);
        found.map(|finding| finding.finding.id.clone()).collect()
    }

    /// Whether `cover` is this entry, or its written plan or one of the plan's stages.
    fn reaches(&self, cover: &CoverRef) -> bool {
        match cover.kind {
            CoverKind::Entry => {
                cover.path == self.metaplan && cover.anchor.as_ref() == Some(&self.entry.anchor)
            }
            CoverKind::Plan | CoverKind::Stage => {
                self.plan.is_some_and(|plan| cover.path == plan.path)
            }
        }
    }
}
