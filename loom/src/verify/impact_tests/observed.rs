//! `reviews/<stage>/impact-observed.json`: the impact-selected failures the
//! stage's latest failing completion run reported (DESIGN D14, D15). A
//! dispute of them names ids only; the daemon copies each named failure from
//! this record into the dispute's evidence, so the filing agent's request
//! adds nothing to what the adjudicator reads.
//!
//! Trust boundary: the record is not authenticated. The host job that runs a
//! stage's completion (`host_verify`) writes it after a failing run, and an
//! operator's own completion writes it directly. Either run executes the
//! stage's own code, so every field (outcome, command, base result, tests,
//! output) carries that code's output. The adjudicator re-runs each disputed
//! selection before accepting it, and an accepted failure excuses only a
//! later failure identical to it (`accepted`). Every field is bounded and
//! stripped of control characters (`ImpactFailure::bounded`).

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

use super::failure::{check_failure_id, ImpactFailure};
use crate::verify::review::store::{self, RECORD_VERSION};
use crate::verify::review::verdict_records;

pub(in crate::verify) const OBSERVED_FILE: &str = "impact-observed.json";

/// Most failures one record keeps; the gate's message names the failures
/// past it as not disputable in that run (`report`).
pub(super) const MAX_OBSERVED_FAILURES: usize = 32;

/// `impact-observed.json`; an absent file observed nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedImpact {
    pub version: u32,
    pub failures: Vec<ImpactFailure>,
}

impl Default for ObservedImpact {
    fn default() -> Self {
        Self::of(&[])
    }
}

impl ObservedImpact {
    /// A record of `failures`, each cut to its bounds
    /// (`ImpactFailure::bounded`), the first `MAX_OBSERVED_FAILURES` of them.
    pub fn of(failures: &[ImpactFailure]) -> Self {
        Self {
            version: RECORD_VERSION,
            failures: failures
                .iter()
                .take(MAX_OBSERVED_FAILURES)
                .map(ImpactFailure::bounded)
                .collect(),
        }
    }

    /// Fail unless the record is one loom writes: this version, at most
    /// `MAX_OBSERVED_FAILURES` failures, each with a well-formed id named once.
    pub fn check(&self) -> Result<()> {
        store::check_version(self.version, OBSERVED_FILE)?;
        if self.failures.len() > MAX_OBSERVED_FAILURES {
            bail!("{OBSERVED_FILE} holds more than {MAX_OBSERVED_FAILURES} failures");
        }
        let mut ids = HashSet::new();
        for failure in &self.failures {
            check_failure_id(&failure.id)?;
            if !ids.insert(failure.id.as_str()) {
                bail!("{OBSERVED_FILE} names '{}' twice", failure.id);
            }
        }
        Ok(())
    }
}

/// `reviews/<stage>/impact-observed.json` in `work_dir`, each failure bounded.
pub fn load_observed(work_dir: &Path, stage_id: &str) -> Result<ObservedImpact> {
    let observed: ObservedImpact =
        store::load_optional(work_dir, stage_id, OBSERVED_FILE)?.unwrap_or_default();
    observed.check()?;
    Ok(ObservedImpact::of(&observed.failures))
}

/// Replace the record of `stage_id` with `observed`, bounded. A record of no
/// failure clears it, so a dispute names only failures of the latest failing
/// completion run: one that passed the impact gate or never reached it left
/// none to dispute.
pub fn record_observed(work_dir: &Path, stage_id: &str, observed: &ObservedImpact) -> Result<()> {
    observed.check()?;
    let bounded = ObservedImpact::of(&observed.failures);
    verdict_records::update(work_dir, stage_id, OBSERVED_FILE, || Ok(bounded))
}

#[cfg(test)]
#[path = "observed_tests.rs"]
mod tests;
