//! Contract: a Codex configuration edit survives the death of the server that ran Codex. The
//! baseline taken before Codex starts lives in `config-baseline.json` in the runtime directory,
//! so the next server's recovery compares it and blocks launches until confirmed. Rejects a
//! baseline kept only in memory, or in the session directory recovery removes, which only a
//! live run's finish or abandon would ever compare.

use std::time::Duration;

use serial_test::serial;

use super::fake::Script;
use super::{call_on, wait_for, Fixture, PLAN};
use crate::plan_server::pressure::codex::CONFIG_CHANGED;
use crate::plan_session::bridge::{RecoveredSession, RecoveryReport};
use crate::plan_session::kind::SessionKind;
use crate::test_stand_in::{require_orphan_signals, AdoptedGroup};

const BOUND: Duration = Duration::from_secs(10);

/// Poll `ready` for [`BOUND`]; a missing artifact is a failed contract, not a hung test.
fn expect_within_bound(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + BOUND;
    while !ready() {
        assert!(
            std::time::Instant::now() < deadline,
            "{what} did not appear within {BOUND:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[serial]
fn an_abrupt_server_death_after_a_config_edit_blocks_the_next_launch() {
    require_orphan_signals!();
    let f = Fixture::new();
    f.codex("config", "");
    f.codex("sleep", "30");
    f.fake.script(vec![Script::Hold]);
    f.start(1);
    let codex = wait_for("Codex recorded", || f.fake.children("s-1").first().copied());
    // The kill guard stops the sleeping Codex even when an assertion below panics.
    let _kill = AdoptedGroup::adopt(codex.pid);

    let runtime = f.ctx.runtime().dir.clone();
    let settings = f.root.join(".claude/settings.local.json");
    expect_within_bound("config-baseline.json", || {
        runtime.join("config-baseline.json").is_file()
    });
    expect_within_bound("the edited settings", || settings.is_file());

    // The server dies while Codex runs: its successor recovers the pressure session.
    let session = RecoveredSession {
        id: "s-1".to_owned(),
        kind: SessionKind::Pressure { plan: PLAN.into() },
        children: Vec::new(),
    };
    let report = RecoveryReport {
        torn_down: vec![session],
        ..RecoveryReport::default()
    };
    let restarted = f.restart(report);

    assert!(
        runtime.join("config-block.json").is_file(),
        "the restart wrote no config-block.json"
    );
    let (_, options) = call_on(&restarted, "/api/pressure/options", &f.holder, None);
    let reason = options["disabled_reason"].as_str().unwrap_or_default();
    assert!(reason.starts_with(CONFIG_CHANGED), "{options}");

    // Wind the first server's run down so no Codex or run thread outlives the test.
    let (status, _) = f.call("/api/pressure/stop", &f.holder, None);
    assert_eq!(status, 202);
    f.finished();
}
