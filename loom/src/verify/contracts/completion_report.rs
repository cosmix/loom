//! What the contract check at completion found (`completion::run`), and the
//! failure text with the way out of each failure.

use anyhow::{bail, Result};

/// What the contract check found: the frozen files that are not as the
/// freeze (or a trial) expects, and every contract's verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractCheck {
    pub drift: Vec<String>,
    pub contracts: Vec<ContractVerdict>,
}

/// One contract's failures, none when it passes, and the environment marker
/// (`criteria::environment::environment_marker`) its failing run printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractVerdict {
    pub id: String,
    pub failures: Vec<String>,
    pub marker: Option<&'static str>,
}

impl ContractCheck {
    pub fn passed(&self) -> bool {
        self.drift.is_empty() && self.contracts.iter().all(|c| c.failures.is_empty())
    }

    /// `Ok` on a pass, else every failure with the way out of each.
    pub fn into_result(self, stage_id: &str) -> Result<()> {
        if self.passed() {
            return Ok(());
        }
        let markers: Vec<String> = self
            .contracts
            .iter()
            .filter_map(|c| {
                c.marker
                    .map(|marker| format!("'{marker}' (contract '{}')", c.id))
            })
            .collect();
        let mut failures = self.drift;
        failures.extend(self.contracts.into_iter().flat_map(|c| c.failures));
        let environment = if markers.is_empty() {
            String::new()
        } else {
            format!(
                "\nThe failing output names a restriction of the confined runner: {}. \
                 This is advice and changes no result.",
                markers.join(", ")
            )
        };
        bail!(
            "contract check failed for stage '{stage_id}':\n  - {}\n{}{environment}",
            failures.join("\n  - "),
            guidance(stage_id)
        )
    }
}

/// The way out of a failing contract check, for the stage `stage_id`.
pub fn guidance(stage_id: &str) -> String {
    format!(
        "Each frozen contract file must keep its frozen content: restore it with \
         `loom stage contracts restore {stage_id}`. A failing contract needs an implementation \
         that passes it. When it fails for a reason no implementation can change (the test \
         needs something the confined runner lacks), write a replacement outside the \
         worktree, run `loom check {stage_id} --contracts --replacement <frozen-file>=<path>`, \
         and once that passes file one `loom stage dispute-contract {stage_id} --contract <id> \
         [--contract <id> ...] --replacement <frozen-file>=<path> --reason ...`. A contract \
         module must be declared by a plain `mod` item, and its chain files must not be \
         cfg'd out."
    )
}
