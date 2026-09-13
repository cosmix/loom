//! `W/inbox/`: the daemon-owned relay inbox each session's requests land in
//! once the relay hook has validated a CLI ticket
//! (`doc/plans/PLAN-loom-state-confinement.md` section 8).
//!
//! Two writers only, both unsandboxed loom code: the relay hook helper
//! ([`write_entry`]) and the daemon's drain ([`append_ledger`] plus the
//! deletes it performs directly with `fs::safe_fs`). Everything here is
//! filesystem plumbing — no wiring into either caller lives in this module.

mod ledger;
mod paths;
mod pending;
mod status;
mod write;

pub use ledger::{
    append_ledger, is_recorded, read_ledger, LedgerOutcome, LedgerRecord, LedgerState,
    MAX_LEDGER_LINE_BYTES,
};
pub use paths::{inbox_root, session_inbox, validate_request_id};
pub use pending::{pending_entries, PendingEntries};
pub use status::{request_status, RequestStatus};
pub use write::{write_entry, WriteOutcome};
