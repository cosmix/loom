//! Operator probes P1 and P2 (`doc/plans/PLAN-planner-background-activity-and-skills.md`,
//! "Before `loom run`"): each `#[ignore]` test drives a logged-in Claude Code through the
//! session bridge over tmux and records what it writes, redacted, under
//! `tests/fixtures/plan_session/` (transcripts, spool, probe-notes).
//!
//! The operator runs them from `loom/`, on the host, outside any sandbox:
//!
//! ```text
//! cargo test --test plan_session_bridge_e2e probes:: -- --ignored --test-threads=1
//! ```
//!
//! Each test runs under a copied HOME (the real `~/.claude.json` and `~/.claude/settings.json`
//! are hashed before and after, and a change fails the test), has a 180 s deadline, and kills
//! the tmux server, the side process and every descendant on any exit.

#[path = "probes/evidence.rs"]
mod evidence;
#[path = "probes/home.rs"]
mod home;
#[path = "probes/names.rs"]
mod names;
#[path = "probes/output.rs"]
mod output;
#[path = "probes/p1.rs"]
mod p1;
#[path = "probes/p1_more.rs"]
mod p1_more;
#[path = "probes/p1_status.rs"]
mod p1_status;
#[path = "probes/p2.rs"]
mod p2;
#[path = "probes/p2_more.rs"]
mod p2_more;
#[path = "probes/paste.rs"]
mod paste;
#[path = "probes/probe.rs"]
mod probe;
#[path = "probes/procs.rs"]
mod procs;
#[path = "probes/redact.rs"]
mod redact;
#[path = "probes/rescrub.rs"]
mod rescrub;
#[path = "probes/scrub.rs"]
mod scrub;
#[path = "probes/setup.rs"]
mod setup;
#[path = "probes/side.rs"]
mod side;
#[path = "probes/side_run.rs"]
mod side_run;
#[path = "probes/side_tests.rs"]
mod side_tests;

use std::time::Duration;

/// The wall-clock bound of one probe test, launch included.
const DEADLINE: Duration = Duration::from_secs(180);

fn secs(count: u64) -> Duration {
    Duration::from_secs(count)
}
