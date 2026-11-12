//! The planning trace of a feature: its documents, spec headings, metaplan entries and
//! findings, linked by the edge lines of `doc/` (`Covers:`, `Implements:`, `About:`), with each
//! document's settle state, the gaps the links leave open and the next few to close.
//!
//! `build_trace` runs for every feature at the end of the project's derive step (each load and
//! refresh) and reads only the loaded project; the result is kept on `Feature::trace`. This is
//! not the source-code graph (`loom map`): no code symbol is a node and nothing here touches
//! `.loom/`.
//!
//! - `tokens`: the tokens of the edge lines.
//! - `settle`: the `Settled:` line, its hashes and the settle states.

mod build;
pub mod gate;
pub mod settle;
pub mod tokens;

use std::path::PathBuf;

use crate::planning::findings::Finding;

pub use build::build_trace;
pub use settle::SettledLine;

/// A trace problem: a malformed `Settled:` line, an `About:` token naming nothing, or a
/// problem of the findings ledger. `loom plan check` reports them with the others.
pub type TraceProblem = crate::planning::problems::Problem;

/// One feature's planning trace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trace {
    /// The vision, the sections (listed or written), the metaplan and the plans (written or
    /// only named by an entry), in spine order.
    pub documents: Vec<TraceDoc>,
    /// The level-2 headings of the written sections, in spine and document order.
    pub headings: Vec<TraceHeading>,
    /// The metaplan entries, in document order.
    pub entries: Vec<TraceEntry>,
    /// The findings ledger, in file order.
    pub findings: Vec<TraceFinding>,
    /// Every gap, ordered by kind (the `GapKind` order) and then spine order.
    pub gaps: Vec<Gap>,
    /// The first gaps to close: at most five, one per document.
    pub frontier: Vec<Gap>,
    /// Malformed `Settled:` lines, unresolved `About:` tokens and ledger problems.
    pub problems: Vec<TraceProblem>,
}

/// One document of the trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceDoc {
    /// Repository-relative path; for a stub, the path the document will have.
    pub path: PathBuf,
    /// What the document is.
    pub role: DocRole,
    /// Its title, or the stub's name (vision row, entry heading) while it is unwritten.
    pub title: String,
    /// How far the document has come.
    pub progress: Progress,
    /// Its own `Settled:` line compared with `current`.
    pub settle: SettleState,
    /// Whether the reader is asked to settle it: a written, unlocked document, other than the
    /// vision and metaplan of a feature whose plans are all done (its Run step reads done). Done work is final, so it
    /// carries no settle state, gap or control; the product sections stay settleable because a
    /// follow-up plan can still change what they promise.
    pub settleable: bool,
    /// Where its stub lives: the vision's sections table, or its metaplan entry.
    pub stub: Option<NodeRef>,
    /// A section's number.
    pub number: Option<u32>,
    /// A plan's slug.
    pub slug: Option<String>,
    /// A plan's brief files, sorted.
    pub bundle: Vec<PathBuf>,
    /// `(My call` markers in the text, case-insensitive.
    pub my_calls: u32,
    /// List items under its `Open questions` headings.
    pub open_questions: u32,
    /// The line settling the document now would write.
    pub current: SettledLine,
}

/// The part a document plays in the trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocRole {
    /// The product vision.
    Vision,
    /// A product section.
    Section,
    /// The metaplan.
    Metaplan,
    /// A plan.
    Plan,
}

/// How far a document has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Only its stub exists: a vision row or a metaplan entry, no file.
    Stub,
    /// The file exists (a section no entry or plan covers yet; a plan not yet run).
    Written,
    /// A section some entry or plan covers.
    Planned,
    /// A covered section whose covering plans are all done or retired, one at least done.
    Built,
    /// A running plan (`IN_PROGRESS-`).
    Running,
    /// A done or retired plan.
    Done,
}

/// A document's `Settled:` line compared with the line settling would write now, checked in
/// variant order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleState {
    /// No `Settled:` line, or one that does not parse.
    Draft,
    /// The body hash differs.
    Revised,
    /// The body matches; the stub hash differs or the stub is gone.
    StubChanged,
    /// Body and stub match; the upstream hash differs.
    UpstreamChanged,
    /// Everything matches.
    Settled,
}

/// A document, or one heading, entry or stage of it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeRef {
    /// Repository-relative document path.
    pub path: PathBuf,
    /// A heading anchor or `stage:<id>`; `None` names the whole document.
    pub anchor: Option<String>,
}

/// A level-2 heading of a written section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceHeading {
    /// The section's path.
    pub path: PathBuf,
    /// The heading's anchor.
    pub anchor: String,
    /// The heading's text.
    pub title: String,
    /// The entries, plans and stages covering it.
    pub covered_by: Vec<CoverRef>,
    /// The ids of the findings whose `About:` names it or its whole section, in ledger order.
    pub findings: Vec<String>,
}

/// What covers a heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverKind {
    /// A metaplan entry's `Covers:` line.
    Entry,
    /// A plan's `Implements:` line.
    Plan,
    /// A stage's `Implements (stage:<id>):` line.
    Stage,
}

/// One entry, plan or stage covering a heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverRef {
    /// Which edge line covers it.
    pub kind: CoverKind,
    /// The metaplan of an entry, the plan of a plan or stage.
    pub path: PathBuf,
    /// The entry's anchor, or `stage:<id>` for a stage.
    pub anchor: Option<String>,
    /// The plan's slug (an entry's when it names one).
    pub slug: Option<String>,
    /// The stage id.
    pub stage: Option<String>,
}

/// One metaplan entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEntry {
    /// The entry heading's anchor.
    pub anchor: String,
    /// The entry heading's text.
    pub title: String,
    /// The slug of the plan it names.
    pub slug: Option<String>,
    /// The plan's winning file, when it is written.
    pub plan: Option<PathBuf>,
    /// The nodes its `Covers:` line names.
    pub covers: Vec<NodeRef>,
    /// For a plan not yet running: the ids of the findings folded into the entry and of the
    /// open findings on a heading it covers or its written plan implements, in ledger order.
    pub revisit: Vec<String>,
}

/// A ledger finding with its `About:` tokens resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceFinding {
    /// The finding as the ledger holds it.
    pub finding: Finding,
    /// The nodes its resolving `About:` tokens name, in token order.
    pub about: Vec<NodeRef>,
}

/// Something the trace says is left to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    /// What kind of gap.
    pub kind: GapKind,
    /// The node to act on.
    pub node: NodeRef,
    /// How many findings it carries (`Revisit`, `Triage`).
    pub count: Option<u32>,
}

/// The kinds of gap, in frontier order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GapKind {
    /// A settled document whose body changed.
    Revised,
    /// A settled document whose stub changed.
    StubChanged,
    /// A settled plan whose implemented spec changed.
    UpstreamChanged,
    /// A plan not yet running that findings ask to revisit.
    Revisit,
    /// A written section or plan that is still a draft.
    Settle,
    /// A spec heading nothing covers.
    Uncovered,
    /// Open findings waiting for triage.
    Triage,
}
