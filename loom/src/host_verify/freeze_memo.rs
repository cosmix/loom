//! Sharing a failed build between the contracts of one freeze job.
//!
//! A job's tree is pinned (the run ends by checking its binding again), so two
//! contracts with the same build key (`verify::contracts::failed_builds`)
//! compile the same code. Only a compile rejection is shared, as the adapter
//! reads it: a transient failure (rustc killed, a missing `cargo nextest`) reds
//! its own contract only. A `failed` run says something about its own test, and
//! a contract with no adapter is never shared because nothing is known about
//! what its command builds.

use anyhow::Result;

use crate::commands::stage::contracts::freeze::{JudgedRun, BUILD_FAILED};
use crate::daemon::ContractRunReport;
use crate::plan::schema::ContractSpec;
use crate::verify::contracts::failed_builds::{BuildKey, FailedBuilds};

/// One contract's judged run.
pub(super) struct Verdict {
    pub judged: Result<ContractRunReport, String>,
    /// The contract whose failed build this one took instead of running.
    pub shared_from: Option<String>,
}

/// Run every contract in order and hand each verdict to `emit` as it is
/// produced. `prepare` resolves a contract, `key` names its build and `run`
/// runs it; a contract whose build an earlier one in the job already failed
/// takes that report under its own id without running.
pub(super) fn run_memoized<P>(
    contracts: &[ContractSpec],
    prepare: impl Fn(&ContractSpec) -> Result<P>,
    key: impl Fn(&P) -> Option<BuildKey>,
    run: impl Fn(&ContractSpec, &P) -> Result<JudgedRun>,
    mut emit: impl FnMut(&ContractSpec, Verdict),
) -> Result<()> {
    let mut failed: FailedBuilds<ContractRunReport> = FailedBuilds::default();
    for contract in contracts {
        let prepared = prepare(contract)?;
        let build = key(&prepared);
        let shared = build.as_ref().and_then(|build| failed.get(build));
        let verdict = match shared {
            Some((first, report)) => Verdict {
                shared_from: Some(first.to_string()),
                judged: Ok(ContractRunReport {
                    contract_id: contract.id.clone(),
                    ..report.clone()
                }),
            },
            None => {
                let ran = run(contract, &prepared)?;
                if let (Some(build), Ok(report), true) = (build, &ran.judged, ran.compile_failed) {
                    if report.outcome == BUILD_FAILED {
                        failed.record(build, contract.id.clone(), report.clone());
                    }
                }
                Verdict {
                    judged: ran.judged,
                    shared_from: None,
                }
            }
        };
        emit(contract, verdict);
    }
    Ok(())
}
