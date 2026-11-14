//! The deterministic checks of the findings ledger: duplicates and evidence.
//!
//! A finding is trusted only when its `Evidence:` quote is in a passage it is about. The check
//! reads the passage each `About:` node names (the heading's lines, a plan stage's entry, else
//! the whole document) and looks for the normalized quote in it, so an edit that removes the
//! quoted text turns the finding stale without anyone re-reading it.

use super::Cx;
use crate::planning::findings::{normalized_quote, same_finding, Finding};
use crate::planning::trace::{FindingCheck, NodeRef};

/// The fewest words a quote needs to count as evidence.
const MIN_QUOTE_WORDS: usize = 3;

/// The check of each finding, in ledger order. `about[i]` holds the resolved nodes of
/// `findings[i]`.
pub(super) fn checks(cx: &Cx, findings: &[Finding], about: &[Vec<NodeRef>]) -> Vec<FindingCheck> {
    let evidence: Vec<FindingCheck> = findings
        .iter()
        .zip(about)
        .map(|(finding, about)| evidence_check(cx, finding, about))
        .collect();
    let mut out: Vec<FindingCheck> = Vec::with_capacity(findings.len());
    for (index, finding) in findings.iter().enumerate() {
        let later = Entry {
            finding,
            evidence: &evidence[index],
        };
        let original = (0..index).find(|&at| {
            let earlier = Entry {
                finding: &findings[at],
                evidence: &evidence[at],
            };
            absorbs(&earlier, &out[at], &later) && same_finding(&findings[at], finding)
        });
        out.push(match original {
            Some(at) => FindingCheck::Duplicate(findings[at].id.clone()),
            None => evidence[index].clone(),
        });
    }
    out
}

/// A ledger entry and what its evidence check alone found.
struct Entry<'a> {
    finding: &'a Finding,
    evidence: &'a FindingCheck,
}

/// Whether an earlier entry that reports the same thing makes `later` a duplicate. `check` is
/// the earlier entry's own result.
///
/// - Open: when the checks verified it, so a verified finding stands for its rewordings.
/// - Addressed: unless `later` is a regression with new evidence, a quote that differs from the
///   earlier one's (marks, quotation marks, case and spacing aside), or a quote that is in its
///   passage now while the earlier quote is gone. A re-raise with the same quote, or with no
///   quote, adds nothing to a finding already dealt with.
/// - Unaddressed or dismissed: never. An unaddressed finding waits for the reader to reopen it,
///   and a later entry about the same thing stands on its own; a review that records a verified
///   finding in place of an imported one, and dismisses the imported one, keeps the verified one.
fn absorbs(earlier: &Entry<'_>, check: &FindingCheck, later: &Entry<'_>) -> bool {
    use crate::planning::findings::FindingStatus::{Addressed, Dismissed, Open, Unaddressed};
    match &earlier.finding.status {
        Open => *check == FindingCheck::Verified,
        Addressed(_) => !regressed(earlier, later),
        Unaddressed(_) | Dismissed(_) => false,
    }
}

/// `later` quotes new evidence of an addressed finding.
fn regressed(addressed: &Entry<'_>, later: &Entry<'_>) -> bool {
    let quote = |entry: &Entry<'_>| entry.finding.evidence.as_deref().map(normalized_quote);
    let Some(new) = quote(later).filter(|quote| !quote.is_empty()) else {
        return false;
    };
    let gone = *addressed.evidence == FindingCheck::Stale;
    quote(addressed).as_ref() != Some(&new) || (*later.evidence == FindingCheck::Verified && gone)
}

fn evidence_check(cx: &Cx, finding: &Finding, about: &[NodeRef]) -> FindingCheck {
    let quote = finding
        .evidence
        .as_deref()
        .map(normalized_quote)
        .unwrap_or_default();
    let unverified = quote.split(' ').count() < MIN_QUOTE_WORDS
        || finding.severity.is_none()
        || about.is_empty();
    if unverified {
        return FindingCheck::Unverified;
    }
    if about
        .iter()
        .any(|node| passage(cx, node).is_some_and(|text| text.contains(&quote)))
    {
        FindingCheck::Verified
    } else {
        FindingCheck::Stale
    }
}

/// The normalized text of the passage `node` names: its heading or stage span when the anchor
/// finds one, else the whole document.
fn passage(cx: &Cx, node: &NodeRef) -> Option<String> {
    let text = cx.text(&node.path)?;
    let span = node.anchor.as_deref().and_then(|anchor| {
        let doc = cx.project.docs.get(&node.path)?;
        doc.sections.find(anchor).cloned()
    });
    let passage = match span {
        Some(span) => {
            let lines: Vec<&str> = text.lines().collect();
            let end = span.end_line.min(lines.len());
            lines[span.start_line.min(end)..end].join("\n")
        }
        None => text.to_owned(),
    };
    Some(normalized_quote(&passage))
}
