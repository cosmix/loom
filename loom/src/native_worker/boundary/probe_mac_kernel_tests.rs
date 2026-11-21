//! The real macOS probes against the kernel's Seatbelt. A Seatbelt profile
//! cannot start inside another one, so inside a stage session or any sandbox
//! these skip (or fail under `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`) and the
//! operator runs them from a plain shell. The launches need a built `loom`
//! next to the test binary (`cargo build`): the boundary starts it as the
//! trusted bridge.

use serial_test::serial;

use super::mac::RealMacHost;
use super::{qualify_macos_from, MacHostProbes};
use crate::process::sandbox_probe::{
    confined_commands_blocker, confined_commands_runnable, skip_unless,
};

/// A real host that can launch, or `None` after the skip decision.
fn launching_host(test: &str) -> Option<RealMacHost> {
    if !confined_commands_runnable() {
        return None;
    }
    let loom = crate::native_worker::boundary::loom_program::built_loom();
    let found = loom.is_some();
    let why = "no built loom beside the test binary: run cargo build first";
    if skip_unless(found, test, why) {
        return None;
    }
    Some(RealMacHost { loom_program: loom })
}

#[test]
fn the_sandbox_exec_probe_reports_what_the_launcher_probe_found() {
    let outcome = RealMacHost::default().sandbox_exec();
    match confined_commands_blocker() {
        None => assert_eq!(outcome, Ok(())),
        Some(blocker) => {
            let refusal = outcome.unwrap_err();
            assert!(refusal.contains(blocker), "{refusal}");
        }
    }
}

#[test]
#[serial]
fn a_payload_is_confined_by_the_production_launch() {
    let Some(host) = launching_host("a_payload_is_confined_by_the_production_launch") else {
        return;
    };
    assert_eq!(host.payload_boundary(), Ok(()));
}

#[test]
#[serial]
#[ignore = "needs an installed Codex"]
fn an_installed_codex_honours_the_controlled_home() {
    let Some(host) = launching_host("an_installed_codex_honours_the_controlled_home") else {
        return;
    };
    assert_eq!(host.codex_home_honoured(), Ok(()));
    assert_eq!(qualify_macos_from(&host), Ok(()));
}
