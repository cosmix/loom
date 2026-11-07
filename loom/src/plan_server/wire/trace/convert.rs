//! Conversions from the planning trace model to its wire views.

use std::path::Path;

use super::*;
use crate::planning::findings::{Finding, FindingStatus};
use crate::planning::trace::{
    CoverKind, CoverRef, DocRole, Gap, GapKind, NodeRef, Progress, SettleState, SettledLine, Trace,
    TraceDoc, TraceEntry, TraceFinding, TraceHeading, TraceProblem,
};

/// `path` as a repository-relative string with `/` separators.
fn rel(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

impl From<&Trace> for TraceView {
    fn from(trace: &Trace) -> Self {
        Self {
            documents: trace.documents.iter().map(Into::into).collect(),
            headings: trace.headings.iter().map(Into::into).collect(),
            entries: trace.entries.iter().map(Into::into).collect(),
            findings: trace.findings.iter().map(Into::into).collect(),
            gaps: trace.gaps.iter().map(Into::into).collect(),
            frontier: trace.frontier.iter().map(Into::into).collect(),
            problems: trace.problems.iter().map(Into::into).collect(),
        }
    }
}

impl From<&NodeRef> for NodeRefView {
    fn from(node: &NodeRef) -> Self {
        Self {
            path: rel(&node.path),
            anchor: node.anchor.clone(),
        }
    }
}

impl From<&SettledLine> for SettledLineView {
    fn from(line: &SettledLine) -> Self {
        Self {
            body: line.body.clone(),
            stub: line.stub.clone(),
            upstream: line.upstream.clone(),
        }
    }
}

impl From<&TraceDoc> for TraceDocView {
    fn from(doc: &TraceDoc) -> Self {
        Self {
            path: rel(&doc.path),
            role: doc.role.into(),
            title: doc.title.clone(),
            progress: doc.progress.into(),
            settle: doc.settle.into(),
            settleable: doc.settleable,
            stub: doc.stub.as_ref().map(Into::into),
            number: doc.number,
            slug: doc.slug.clone(),
            bundle: doc.bundle.iter().map(|path| rel(path)).collect(),
            my_calls: doc.my_calls,
            open_questions: doc.open_questions,
            current: (&doc.current).into(),
        }
    }
}

impl From<&TraceHeading> for TraceHeadingView {
    fn from(heading: &TraceHeading) -> Self {
        Self {
            path: rel(&heading.path),
            anchor: heading.anchor.clone(),
            title: heading.title.clone(),
            covered_by: heading.covered_by.iter().map(Into::into).collect(),
            findings: heading.findings.clone(),
        }
    }
}

impl From<&CoverRef> for CoverRefView {
    fn from(cover: &CoverRef) -> Self {
        Self {
            kind: cover.kind.into(),
            path: rel(&cover.path),
            anchor: cover.anchor.clone(),
            slug: cover.slug.clone(),
            stage: cover.stage.clone(),
        }
    }
}

impl From<&TraceEntry> for TraceEntryView {
    fn from(entry: &TraceEntry) -> Self {
        Self {
            anchor: entry.anchor.clone(),
            title: entry.title.clone(),
            slug: entry.slug.clone(),
            plan: entry.plan.as_deref().map(rel),
            covers: entry.covers.iter().map(Into::into).collect(),
            revisit: entry.revisit.clone(),
        }
    }
}

impl From<&FindingStatus> for FindingStatusView {
    fn from(status: &FindingStatus) -> Self {
        let (kind, detail) = match status {
            FindingStatus::Open => (FindingKindView::Open, None),
            FindingStatus::Planned(detail) => (FindingKindView::Planned, Some(detail)),
            FindingStatus::Folded(detail) => (FindingKindView::Folded, Some(detail)),
            FindingStatus::Dismissed(detail) => (FindingKindView::Dismissed, Some(detail)),
        };
        Self {
            kind,
            detail: detail.cloned(),
        }
    }
}

impl From<&Finding> for FindingView {
    fn from(finding: &Finding) -> Self {
        Self {
            id: finding.id.clone(),
            title: finding.title.clone(),
            about: finding.about.clone(),
            source: finding.source.clone(),
            status: (&finding.status).into(),
            text: finding.text.clone(),
        }
    }
}

impl From<&TraceFinding> for TraceFindingView {
    fn from(found: &TraceFinding) -> Self {
        Self {
            finding: (&found.finding).into(),
            about: found.about.iter().map(Into::into).collect(),
        }
    }
}

impl From<&Gap> for GapView {
    fn from(gap: &Gap) -> Self {
        Self {
            kind: gap.kind.into(),
            node: (&gap.node).into(),
            count: gap.count,
        }
    }
}

impl From<&TraceProblem> for TraceProblemView {
    fn from(problem: &TraceProblem) -> Self {
        Self {
            code: problem.code.to_owned(),
            path: rel(&problem.path),
            line: problem
                .line
                .map(|line| u32::try_from(line).unwrap_or(u32::MAX)),
            message: problem.message.clone(),
        }
    }
}

impl From<DocRole> for DocRoleView {
    fn from(role: DocRole) -> Self {
        match role {
            DocRole::Vision => Self::Vision,
            DocRole::Section => Self::Section,
            DocRole::Metaplan => Self::Metaplan,
            DocRole::Plan => Self::Plan,
        }
    }
}

impl From<Progress> for ProgressView {
    fn from(progress: Progress) -> Self {
        match progress {
            Progress::Stub => Self::Stub,
            Progress::Written => Self::Written,
            Progress::Planned => Self::Planned,
            Progress::Built => Self::Built,
            Progress::Running => Self::Running,
            Progress::Done => Self::Done,
        }
    }
}

impl From<SettleState> for SettleStateView {
    fn from(state: SettleState) -> Self {
        match state {
            SettleState::Draft => Self::Draft,
            SettleState::Revised => Self::Revised,
            SettleState::StubChanged => Self::StubChanged,
            SettleState::UpstreamChanged => Self::UpstreamChanged,
            SettleState::Settled => Self::Settled,
        }
    }
}

impl From<CoverKind> for CoverKindView {
    fn from(kind: CoverKind) -> Self {
        match kind {
            CoverKind::Entry => Self::Entry,
            CoverKind::Plan => Self::Plan,
            CoverKind::Stage => Self::Stage,
        }
    }
}

impl From<GapKind> for GapKindView {
    fn from(kind: GapKind) -> Self {
        match kind {
            GapKind::Revised => Self::Revised,
            GapKind::StubChanged => Self::StubChanged,
            GapKind::UpstreamChanged => Self::UpstreamChanged,
            GapKind::Revisit => Self::Revisit,
            GapKind::Settle => Self::Settle,
            GapKind::Uncovered => Self::Uncovered,
            GapKind::Triage => Self::Triage,
        }
    }
}
