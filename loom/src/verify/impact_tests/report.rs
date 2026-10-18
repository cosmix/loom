//! Judging the selections that failed (DESIGN D14, D15). A failure identical
//! to one a dispute of the stage accepted is a note; one that fails at the
//! stage's base no better is a pre-existing failure and a note. The rest fail
//! the gate, which names each by its id and gives the dispute command that
//! contests the ones loom keeps for a dispute.

use anyhow::Result;
use std::fmt;

use super::accepted::{AcceptedImpact, Standing};
use super::base::{compare, BaseProbe};
use super::failure::ImpactFailure;
use super::observed::MAX_OBSERVED_FAILURES;
use super::runs::Selection;
use super::{ImpactOutcome, FULL_SUITE};
use crate::testrun::command::shell_word;

/// A completion check's selections, judged.
pub(super) struct Report {
    pub(super) outcome: ImpactOutcome,
    /// The failures the stage fixes or disputes.
    pub(super) failures: Vec<ImpactFailure>,
}

impl Report {
    /// Judge what `selection` left against `accepted`, then at `base`.
    pub(super) fn judge(
        selection: Selection<'_>,
        accepted: &AcceptedImpact,
        base: &BaseProbe<'_>,
    ) -> Self {
        let Selection {
            ran,
            mut notes,
            failed,
            ..
        } = selection;
        let mut unsettled = Vec::new();
        for mut failed in failed {
            match accepted.standing(&failed.id, &failed.failure.kind, &failed.tests()) {
                Standing::Covered(note) => {
                    notes.insert(format!("{note}; {FULL_SUITE}"));
                }
                Standing::Changed(why) => {
                    failed.accepted = Some(why);
                    unsettled.push(failed);
                }
                Standing::Unaccepted => unsettled.push(failed),
            }
        }
        let failures = compare(base, unsettled, &mut notes);
        Self {
            outcome: ImpactOutcome {
                ran,
                notes: notes.into_iter().collect(),
            },
            failures,
        }
    }

    /// The outcome, or an [`ImpactTestsFailed`] listing every failure and the
    /// dispute that contests them.
    pub(super) fn into_outcome(self, stage_id: &str) -> Result<ImpactOutcome> {
        if self.failures.is_empty() {
            return Ok(self.outcome);
        }
        Err(ImpactTestsFailed {
            message: failure_message(stage_id, &self.failures),
            failures: self.failures,
        }
        .into())
    }
}

/// The error [`super::run`] fails with when selections fail: the gate's
/// message, and the failures loom records for a dispute to name
/// (`observed`). A caller finds it with `anyhow::Error::downcast_ref`.
#[derive(Debug)]
pub struct ImpactTestsFailed {
    message: String,
    pub failures: Vec<ImpactFailure>,
}

impl fmt::Display for ImpactTestsFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ImpactTestsFailed {}

fn failure_message(stage_id: &str, failures: &[ImpactFailure]) -> String {
    let lines: Vec<String> = failures.iter().map(ImpactFailure::line).collect();
    let (kept, beyond) = failures.split_at(failures.len().min(MAX_OBSERVED_FAILURES));
    let tests: Vec<String> = kept
        .iter()
        .map(|failure| format!("--test {}", shell_word(&failure.id)))
        .collect();
    format!(
        "impact-selected tests failed for stage '{stage_id}':\n  - {}\n\
         These tests reach code the stage changed: fix the regression, then complete the stage \
         again. A failure this stage did not cause (its diff cannot reach the cause, or the base \
         could not be compared) is disputed, never blocked:\n  \
         loom stage dispute-criteria {stage_id} --field impact-tests {} --reason \"<why this \
         stage did not cause it>\"{}",
        lines.join("\n  - "),
        tests.join(" "),
        beyond_note(beyond)
    )
}

/// What the message says of the failures past the `MAX_OBSERVED_FAILURES`
/// loom keeps for a dispute: none of them can be disputed after this run.
fn beyond_note(beyond: &[ImpactFailure]) -> String {
    if beyond.is_empty() {
        return String::new();
    }
    let ids: Vec<&str> = beyond.iter().map(|failure| failure.id.as_str()).collect();
    format!(
        "\nLoom keeps the first {MAX_OBSERVED_FAILURES} failures for a dispute, so {} cannot be \
         disputed after this run: fix them, or fix or dispute the first ones and complete the \
         stage again.",
        ids.join(", ")
    )
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
