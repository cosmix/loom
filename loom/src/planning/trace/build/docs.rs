//! The trace's documents in spine order: progress, stubs, hashes and settle states.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{count, Cx, PlanSlot, SectionSlot};
use crate::planning::briefs::bundle_files;
use crate::planning::kinds::{plan_path, vision_path};
use crate::planning::sections::normalized_hash;
use crate::planning::slug::PlanState;
use crate::planning::trace::settle::{
    body_hash, combined_hash, settled_line, settled_marks, short_hash, state, SettledLine,
};
use crate::planning::trace::{CoverKind, DocRole, NodeRef, Progress, TraceDoc, TraceProblem};

/// What a document's `TraceDoc` is built from besides its text.
struct Seed {
    path: PathBuf,
    role: DocRole,
    title: Option<String>,
    progress: Progress,
    number: Option<u32>,
    slug: Option<String>,
    /// Where the stub lives and its short hash.
    stub: Option<(NodeRef, String)>,
    upstream: Option<String>,
}

impl Seed {
    fn new(path: PathBuf, role: DocRole, progress: Progress) -> Self {
        Seed {
            path,
            role,
            title: None,
            progress,
            number: None,
            slug: None,
            stub: None,
            upstream: None,
        }
    }
}

/// The vision, the sections, the metaplan and the plans, in spine order, and the problems of
/// their `Settled:` lines.
pub(super) fn documents(cx: &Cx, problems: &mut Vec<TraceProblem>) -> Vec<TraceDoc> {
    let mut seeds = Vec::new();
    let vision = vision_path(cx.name());
    if cx.project.docs.contains_key(&vision) {
        seeds.push(Seed::new(vision, DocRole::Vision, Progress::Written));
    }
    seeds.extend(cx.sections.iter().map(|slot| section_seed(cx, slot)));
    for (path, _) in &cx.feature.metaplans {
        seeds.push(Seed::new(
            path.clone(),
            DocRole::Metaplan,
            Progress::Written,
        ));
    }
    seeds.extend(cx.plans.iter().map(|slot| plan_seed(cx, slot)));
    let documents: Vec<TraceDoc> = seeds.into_iter().map(|seed| finish(cx, seed)).collect();
    for doc in &documents {
        settled_problems(cx, &doc.path, problems);
    }
    documents
}

fn section_seed(cx: &Cx, slot: &SectionSlot) -> Seed {
    let progress = if slot.written {
        section_progress(cx, &slot.path)
    } else {
        Progress::Stub
    };
    let mut seed = Seed::new(slot.path.clone(), DocRole::Section, progress);
    seed.number = Some(slot.number);
    let Some(row) = slot.row else {
        return seed;
    };
    seed.title = Some(row.name.clone());
    let vision = vision_path(cx.name());
    // The heading `vision::parse_sections_table` reads the rows under.
    let table = cx.project.docs.get(&vision).and_then(|doc| {
        let mut headings = doc.sections.headings.iter();
        let table = headings.find(|h| h.title.eq_ignore_ascii_case("Specification sections"))?;
        Some(table.anchor.clone())
    });
    let text = cx.text(&vision).and_then(|text| text.lines().nth(row.line));
    seed.stub = text.map(|text| {
        let node = NodeRef {
            path: vision,
            anchor: table,
        };
        (node, short_hash(&normalized_hash(text)))
    });
    seed
}

/// `Planned` once anything claims the section or one of its headings; `Built` when every plan
/// claiming it, entries' plans included, is done or retired and one at least is done.
fn section_progress(cx: &Cx, path: &Path) -> Progress {
    let claims: Vec<_> = cx
        .claims
        .iter()
        .filter(|claim| claim.path == path)
        .collect();
    if claims.is_empty() {
        return Progress::Written;
    }
    let states: Vec<Option<PlanState>> = claims
        .iter()
        .map(|claim| claim.cover.slug.as_deref())
        .map(|slug| {
            slug.and_then(|slug| cx.project.tree.plan(slug))
                .map(|plan| plan.state)
        })
        .collect();
    let finished =
        |state: &Option<PlanState>| matches!(state, Some(PlanState::Done | PlanState::Retired));
    let done = states.contains(&Some(PlanState::Done));
    if done && states.iter().all(finished) {
        Progress::Built
    } else {
        Progress::Planned
    }
}

fn plan_seed(cx: &Cx, slot: &PlanSlot) -> Seed {
    let (path, progress) = match slot.plan {
        Some(plan) => (plan.path.clone(), plan_progress(plan.state)),
        None => (plan_path(Some(cx.name()), slot.slug), Progress::Stub),
    };
    let mut seed = Seed::new(path, DocRole::Plan, progress);
    seed.slug = Some(slot.slug.to_owned());
    if let Some((metaplan, entry)) = slot.entry {
        seed.title = Some(entry.heading.clone());
        let node = NodeRef {
            path: metaplan.to_path_buf(),
            anchor: Some(entry.anchor.clone()),
        };
        seed.stub = Some((node, short_hash(&entry.span_hash)));
    }
    if slot.plan.is_some() {
        seed.upstream = upstream(cx, slot.slug);
    }
    seed
}

fn plan_progress(state: PlanState) -> Progress {
    match state {
        PlanState::Written => Progress::Written,
        PlanState::Running => Progress::Running,
        PlanState::Done | PlanState::Retired => Progress::Done,
    }
}

/// The upstream hash of a plan: over each heading its `Implements:` and stage lines name, by
/// path then anchor, each with its span hash, and each section they name whole, with the hash
/// of its text without its `Settled:` line (settling a section is no upstream change).
/// `None` when the plan implements nothing.
fn upstream(cx: &Cx, slug: &str) -> Option<String> {
    let claims: Vec<_> = cx
        .claims
        .iter()
        .filter(|claim| claim.cover.kind != CoverKind::Entry)
        .filter(|claim| claim.cover.slug.as_deref() == Some(slug))
        .collect();
    let whole: BTreeSet<&Path> = claims
        .iter()
        .filter(|claim| claim.anchor.is_none())
        .map(|claim| claim.path.as_path())
        .collect();
    let named: BTreeSet<(&Path, &str)> = claims
        .iter()
        .filter(|claim| claim.anchor.is_none() || !whole.contains(claim.path.as_path()))
        .map(|claim| {
            (
                claim.path.as_path(),
                claim.anchor.as_deref().unwrap_or_default(),
            )
        })
        .collect();
    if named.is_empty() {
        return None;
    }
    let parts: Vec<String> = named
        .into_iter()
        .map(|(path, anchor)| {
            let hash = node_hash(cx, path, anchor);
            format!("{} {anchor} {hash}", path.display())
        })
        .collect();
    Some(combined_hash(&parts))
}

/// The hash of one implemented node, `-` for a node that does not exist. Both forms leave out
/// `Settled:` lines: settling a document must not change what the plans implementing it read as
/// upstream, and a level-1 heading's span holds the line settling writes.
fn node_hash(cx: &Cx, path: &Path, anchor: &str) -> String {
    let hash = if anchor.is_empty() {
        cx.text(path).map(|text| body_hash(text, &[]))
    } else {
        span_hash(cx, path, anchor)
    };
    hash.unwrap_or_else(|| "-".to_owned())
}

/// The `body_hash` of the lines `anchor` addresses in `path`.
fn span_hash(cx: &Cx, path: &Path, anchor: &str) -> Option<String> {
    let span = cx.project.docs.get(path)?.sections.find(anchor)?;
    let lines: Vec<&str> = cx.text(path)?.lines().collect();
    let end = span.end_line.min(lines.len());
    let mut body = lines.get(span.start_line..end)?.join("\n");
    body.push('\n');
    Some(body_hash(&body, &[]))
}

/// The document as the trace shows it, with the line settling would write now.
fn finish(cx: &Cx, seed: Seed) -> TraceDoc {
    let doc = cx.project.docs.get(&seed.path);
    let text = cx.text(&seed.path);
    let bundle = match (seed.role, seed.progress) {
        (DocRole::Plan, progress) if progress != Progress::Stub => {
            bundle_files(cx.project, &seed.path)
        }
        _ => Vec::new(),
    };
    // A brief without text (non-Markdown, unreadable or oversized) is hashed by its fingerprint.
    let prints: Vec<Option<String>> = bundle
        .iter()
        .map(|path| match cx.text(path) {
            Some(_) => None,
            None => cx.project.fingerprint(path),
        })
        .collect();
    let briefs: Vec<(&Path, &str)> = bundle
        .iter()
        .zip(&prints)
        .map(|(path, print)| {
            let content = cx.text(path).or(print.as_deref());
            (path.as_path(), content.unwrap_or_default())
        })
        .collect();
    let (stub, stub_hash) = seed.stub.unzip();
    let current = SettledLine {
        body: body_hash(text.unwrap_or_default(), &briefs),
        stub: stub_hash,
        upstream: seed.upstream,
    };
    let settle = state(text.and_then(settled_line).as_ref(), &current);
    // A stub has no document yet: it shows its vision row's or entry heading's name.
    let title = doc.map(|doc| doc.title.clone()).or(seed.title);
    let open_questions = doc.map_or(0, |doc| doc.open_questions);
    TraceDoc {
        title: title.unwrap_or_default(),
        role: seed.role,
        progress: seed.progress,
        settle,
        stub,
        number: seed.number,
        slug: seed.slug,
        bundle,
        my_calls: count(text.map_or(0, |text| text.to_lowercase().matches("(my call").count())),
        open_questions: count(open_questions),
        current,
        path: seed.path,
    }
}

/// `settled-duplicate` and `settled-bad` for an unlocked document's `Settled:` lines: a locked
/// document keeps the line it had when it locked and cannot be edited.
fn settled_problems(cx: &Cx, path: &Path, problems: &mut Vec<TraceProblem>) {
    let Some(text) = cx.text(path).filter(|_| cx.unlocked(path)) else {
        return;
    };
    for (index, mark) in settled_marks(text).iter().enumerate() {
        let problem = |code, message: &str| TraceProblem {
            code,
            path: path.to_path_buf(),
            line: Some(mark.line + 1),
            message: message.to_owned(),
        };
        if index > 0 {
            problems.push(problem(
                "settled-duplicate",
                "the document has a second Settled: line",
            ));
        }
        if mark.parsed.is_none() {
            problems.push(problem(
                "settled-bad",
                "the Settled: line does not parse, so the document reads as a draft",
            ));
        }
    }
}
