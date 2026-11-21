//! Statistics of a finished plan: built from the persisted stages and the token
//! totals, archived once under the loom home, and exposed as a current copy in
//! the state directory for the status feed.
//!
//! * [`record`] holds the pinned record types.
//! * [`identity`] holds the run identity `loom init` stamps into the plan table.
//! * [`build()`] derives a record from the work directory.
//! * [`publish()`] archives a record without ever replacing an existing one.
//! * [`current`] reads and writes the state directory's copy.
//! * [`finish_plan`] ties them together for the scheduler.

mod build;
mod current;
mod finish;
mod identity;
mod publish;
mod record;
mod sanitize;

pub use current::{read_current, remove_current};
pub use finish::finish_plan;
pub use identity::stamp_new_run;
pub use publish::{archive_path, loom_home};
pub use record::{all_tokens, ModelStats, PlanStatsRecord, StageStats};

#[cfg(test)]
pub use current::{write_current, CURRENT_FILE};
#[cfg(test)]
pub use publish::{file_name, publish, Published};
#[cfg(test)]
pub use record::MAX_RECORD_BYTES;

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_files;
#[cfg(test)]
mod tests_fixture;
