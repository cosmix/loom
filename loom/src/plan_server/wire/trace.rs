//! The planning trace of a feature as the browser receives it (`GET /api/trace/<feature>`),
//! mirrored by `web/src/plan/api/schema-trace.ts`.
//!
//! Every path is repository-relative with `/` separators, enums are snake_case and an absent
//! value is `null`. Finding text, titles and reasons are agent-written; the browser renders
//! them as text only.

use serde::{Deserialize, Serialize};

use super::files::ValidationView;

mod convert;
#[cfg(test)]
mod tests;

/// One feature's planning trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceView {
    /// The vision, sections, metaplan and plans, in spine order.
    pub documents: Vec<TraceDocView>,
    /// The level-2 headings of the written sections.
    pub headings: Vec<TraceHeadingView>,
    /// The metaplan entries, in document order.
    pub entries: Vec<TraceEntryView>,
    /// The findings ledger, in file order.
    pub findings: Vec<TraceFindingView>,
    /// Every gap, in frontier-kind order.
    pub gaps: Vec<GapView>,
    /// The first gaps to close: at most five.
    pub frontier: Vec<GapView>,
    /// Malformed marks, unresolved tokens and ledger problems.
    pub problems: Vec<TraceProblemView>,
}

/// The part a document plays in the trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocRoleView {
    Vision,
    Section,
    Metaplan,
    Plan,
}

/// How far a document has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressView {
    Stub,
    Written,
    Planned,
    Built,
    Running,
    Done,
}

/// A document's settle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettleStateView {
    Draft,
    Revised,
    StubChanged,
    UpstreamChanged,
    Settled,
}

/// What kind of edge line covers a heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverKindView {
    Entry,
    Plan,
    Stage,
}

/// The kinds of gap, in frontier order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapKindView {
    Revised,
    StubChanged,
    UpstreamChanged,
    Revisit,
    Settle,
    Uncovered,
    Triage,
}

/// A finding's triage state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKindView {
    Open,
    Planned,
    Folded,
    Dismissed,
}

/// A document, or one heading or stage of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeRefView {
    pub path: String,
    /// A heading anchor or `stage:<id>`; null names the whole document.
    pub anchor: Option<String>,
}

/// The hashes a `Settled:` line records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettledLineView {
    pub body: String,
    pub stub: Option<String>,
    pub upstream: Option<String>,
}

/// One document of the trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceDocView {
    pub path: String,
    pub role: DocRoleView,
    pub title: String,
    pub progress: ProgressView,
    pub settle: SettleStateView,
    /// Whether the reader is asked to settle it (`TraceDoc::settleable`); a document that is not
    /// shows no settle state anywhere.
    pub settleable: bool,
    /// Where the document's stub lives: a vision row or a metaplan entry.
    pub stub: Option<NodeRefView>,
    pub number: Option<u32>,
    pub slug: Option<String>,
    /// A plan's brief files, sorted.
    pub bundle: Vec<String>,
    pub my_calls: u32,
    pub open_questions: u32,
    /// The line settling the document now would write.
    pub current: SettledLineView,
}

/// A level-2 heading of a written section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceHeadingView {
    pub path: String,
    pub anchor: String,
    pub title: String,
    pub covered_by: Vec<CoverRefView>,
    /// Ids of the findings whose `About:` names the heading or its whole section.
    pub findings: Vec<String>,
}

/// One entry, plan or stage covering a heading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverRefView {
    pub kind: CoverKindView,
    pub path: String,
    pub anchor: Option<String>,
    pub slug: Option<String>,
    pub stage: Option<String>,
}

/// One metaplan entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceEntryView {
    pub anchor: String,
    pub title: String,
    pub slug: Option<String>,
    pub plan: Option<String>,
    pub covers: Vec<NodeRefView>,
    /// Ids of the findings to look at before the entry's plan is written or run.
    pub revisit: Vec<String>,
}

/// A finding's status: the kind and, for all but `open`, its detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingStatusView {
    pub kind: FindingKindView,
    pub detail: Option<String>,
}

/// A ledger entry as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingView {
    pub id: String,
    pub title: String,
    /// The `About:` tokens as written.
    pub about: Vec<String>,
    pub source: Option<String>,
    pub status: FindingStatusView,
    pub text: String,
}

/// A ledger finding with its `About:` tokens resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceFindingView {
    pub finding: FindingView,
    pub about: Vec<NodeRefView>,
}

/// Something the trace says is left to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GapView {
    pub kind: GapKindView,
    pub node: NodeRefView,
    pub count: Option<u32>,
}

/// A convention problem of the trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceProblemView {
    pub code: String,
    pub path: String,
    pub line: Option<u32>,
    pub message: String,
}

/// `POST /api/settle` once the line is written or cleared: the save's answer and where its commit
/// stands. The commit runs after the answer, off the request; a conflict or an invalid plan
/// answers the save route's `SaveResult`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum SettleResult {
    Saved {
        hash: String,
        validation: Option<ValidationView>,
        commit: SettleCommitStart,
    },
}

/// `POST /api/settle-all`: documents written, and those left for a later or confirmed press.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettleAllResult {
    pub settled: Vec<String>,
    pub skipped: Vec<SettleSkipped>,
}

/// One document the bulk settle did not write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettleSkipped {
    pub path: String,
    pub reason: SettleSkipReason,
}

/// Why bulk settling left a document alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettleSkipReason {
    Unresolved,
    Stale,
    Stub,
}

/// Where a written line's commit stands when `POST /api/settle` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum SettleCommitStart {
    /// Queued behind any earlier settle; how it ends arrives as a `settle_commit` message with
    /// this `job`, an id led by the server's own token so no other server's job carries it.
    Pending { job: String },
    /// It will not run: why (the commit worker could not start).
    NotCommitted { reason: String },
}

/// How a settle commit ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettleCommitState {
    /// Git made the commit; `commit` names it when it is the settle's own.
    Committed,
    /// The settled paths already matched HEAD: nothing to commit.
    Unchanged,
    /// The line is written but not committed.
    NotCommitted,
}

/// The `settle_commit` message: how a queued settle commit ended, sent to every browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettleCommitView {
    /// The `job` of the settle answer that queued it.
    pub job: String,
    /// The document, repository-relative.
    pub path: String,
    pub state: SettleCommitState,
    /// The settle's own commit; set for `committed` only, and null there when git made the commit
    /// but HEAD no longer shows it as the settle's.
    pub commit: Option<String>,
    /// For `not_committed`, why (a merge in progress, a hook that refused, an edit that arrived
    /// first); for `committed`, what the reader still has to do, in sentences (a commit hook
    /// changed the document after it was settled). The settling browser reads git's and the hooks'
    /// words; every other browser a short reason in their place. Drawn as text.
    pub reason: Option<String>,
}
