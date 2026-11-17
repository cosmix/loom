//! Contracts of stage native-runtime: proxy allowlist, pid-namespace boundary,
//! quiescence check, private codex home, supervisor loop and settlement.

use std::time::Duration;

use serde_json::json;

use super::boundary::Quiescence;
use super::index::{stage_disposition, unit_state, NativeDisposition};
use super::model::{Outcome, Ownership, INTERRUPT_GRACE_SECS};
use super::proxy::allow::connect_target_allowed;
use super::store::read_manifest;
use super::supervisor::{run_session, settle};

#[path = "testkit/mod.rs"]
mod testkit;

use testkit::peer::{agent_message, line, turn_completed};
use testkit::{event_lines, invocation, End, ScriptedPeer, SteppingClock, STAGE, UNIT};

#[test]
fn proxy_refuses_lookalike_hosts() {
    let allowed = ["chatgpt.com", "*.chatgpt.com", "api.openai.com"];
    let cases = [
        ("api.openai.com:443", true),
        ("API.OpenAI.com.:443", true),
        ("ab.chatgpt.com:443", true),
        ("chatgpt.com.evil.net:443", false),
        ("evilchatgpt.com:443", false),
        ("api.openai.com:22", false),
        ("api.openai.com", false),
        ("127.0.0.1:443", false),
        ("[::1]:443", false),
        ("api.openai.com:443@evil.net:443", false),
    ];
    for (target, expected) in cases {
        assert_eq!(
            connect_target_allowed(target, &allowed),
            expected,
            "CONNECT target {target:?}"
        );
    }
}

/// The pid-namespace boundary exists only on Linux (bubblewrap).
#[cfg(target_os = "linux")]
mod bwrap {
    use std::ffi::OsString;
    use std::fs;
    use std::path::Path;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use crate::native_worker::boundary::{
        identity_for, spawn_payload, spawn_payload_with_codex_home, verify_quiescent, Quiescence,
    };
    use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};

    const NEEDS_BWRAP: &str = "needs a working bubblewrap pid namespace";

    fn sh(script: &str) -> Vec<OsString> {
        vec![OsString::from("-c"), OsString::from(script)]
    }

    fn canonical_tempdir() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let path = tmp.path().canonicalize().expect("canonical temp dir");
        (tmp, path)
    }

    fn file_len(path: &Path) -> u64 {
        fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    }

    #[test]
    #[serial_test::serial]
    fn boundary_kill_stops_detached_descendant() {
        let name = "boundary_kill_stops_detached_descendant";
        if skip_unless(confined_commands_runnable(), name, NEEDS_BWRAP) {
            return;
        }
        let (_tmp, worktree) = canonical_tempdir();
        // The loop stops by itself once `beat` is gone (the TempDir drop), so a
        // boundary that lets it escape does not leave it running after the test.
        let script = format!(
            "cd '{}' && setsid sh -c ': > beat; while [ -e beat ]; do date +%s%N >> beat; sleep 0.05; done' >/dev/null 2>&1 & sleep 60",
            worktree.display()
        );
        let mut boundary =
            spawn_payload(&worktree, Path::new("/bin/sh"), &sh(&script)).expect("spawn payload");
        let beat = worktree.join("beat");
        let waited = Instant::now();
        while file_len(&beat) < 40 && waited.elapsed() < Duration::from_secs(15) {
            sleep(Duration::from_millis(50));
        }
        assert!(file_len(&beat) >= 40, "the detached loop never wrote beat");
        let identity = identity_for(&boundary).expect("boundary identity");
        boundary.process.retire();
        let quiescence = verify_quiescent(&identity);
        let first = file_len(&beat);
        sleep(Duration::from_millis(300));
        let second = file_len(&beat);
        assert!(
            matches!(quiescence, Quiescence::Released),
            "verify_quiescent after the kill: {quiescence:?}"
        );
        assert_eq!(first, second, "the setsid descendant kept writing beat");
    }

    #[test]
    #[serial_test::serial]
    fn quiescence_detects_surviving_namespace_member() {
        let name = "quiescence_detects_surviving_namespace_member";
        if skip_unless(confined_commands_runnable(), name, NEEDS_BWRAP) {
            return;
        }
        let (_tmp, worktree) = canonical_tempdir();
        let mut boundary =
            spawn_payload(&worktree, Path::new("/bin/sh"), &sh("sleep 30")).expect("spawn payload");
        let identity = identity_for(&boundary);
        let quiescence = identity.as_ref().map(verify_quiescent);
        boundary.process.retire();
        let quiescence = quiescence.expect("boundary identity");
        assert!(
            matches!(quiescence, Quiescence::Unverified(_)),
            "a live namespace member must leave ownership unverified: {quiescence:?}"
        );
    }

    /// The host codex home of the codex-home contract: (hooks, config) contents.
    fn host_codex_home(codex: &Path) -> (&'static str, &'static str) {
        let hooks = "{\"hooks\":\"host-hooks-marker\"}\n";
        let config = "host_config_marker = \"operator-config-7f3a\"\n";
        fs::create_dir_all(codex.join("sessions")).expect("sessions dir");
        fs::write(codex.join("hooks.json"), hooks).expect("hooks.json");
        fs::write(codex.join("config.toml"), config).expect("config.toml");
        fs::write(codex.join("auth.json"), "{\"tokens\":{}}\n").expect("auth.json");
        (hooks, config)
    }

    #[test]
    #[serial_test::serial]
    fn boundary_codex_home_is_private() {
        let name = "boundary_codex_home_is_private";
        if skip_unless(confined_commands_runnable(), name, NEEDS_BWRAP) {
            return;
        }
        let (_home_tmp, home) = canonical_tempdir();
        let (_work_tmp, w) = canonical_tempdir();
        let codex = home.join(".codex");
        let (hooks, config) = host_codex_home(&codex);
        fs::create_dir(w.join(".claude")).expect("W/.claude");
        // Literal paths, never $HOME: a wrong boundary must not reach the operator's own home.
        let (c, w_str) = (codex.display(), w.display());
        let script = format!(
            "echo planted > '{c}/hooks.json'; echo planted > '{c}/config.toml'; \
             echo canary > '{c}/sessions/canary'; echo planted > '{w_str}/.claude/x'; \
             cat '{c}/config.toml' > '{w_str}/seen'; exit 0"
        );
        let mut boundary =
            spawn_payload_with_codex_home(&w, &codex, Path::new("/bin/sh"), &sh(&script))
                .expect("spawn payload with codex home");
        let exited = boundary.process.wait(Duration::from_secs(30));
        boundary.process.retire();
        assert!(
            matches!(exited, Ok(Some(_))),
            "payload did not exit: {exited:?}"
        );
        assert_eq!(fs::read_to_string(codex.join("hooks.json")).unwrap(), hooks);
        assert_eq!(
            fs::read_to_string(codex.join("config.toml")).unwrap(),
            config
        );
        assert!(
            codex.join("sessions/canary").is_file(),
            "sessions/ must be shared"
        );
        assert!(!w.join(".claude/x").exists(), "W/.claude must be read-only");
        let seen = fs::read_to_string(w.join("seen")).unwrap_or_default();
        assert!(
            !seen.contains("operator-config-7f3a"),
            "the host config.toml is visible inside the boundary"
        );
    }
}

#[test]
fn supervisor_survives_noisy_provider_output() {
    let record = invocation();
    let pad = "n".repeat(2048);
    let mut lines: Vec<Vec<u8>> = (0..5000)
        .map(|i| line(&json!({ "method": "noise/unknown", "params": { "i": i, "pad": pad } })))
        .collect();
    let huge = "h".repeat(2 * 1024 * 1024);
    lines.push(line(
        &json!({ "method": "noise/huge", "params": { "pad": huge } }),
    ));
    lines.push(agent_message("m1", "All done.", "final_answer"));
    lines.push(turn_completed("completed"));
    let mut peer = ScriptedPeer::through_turn_start(lines, End::Silent);
    let clock = SteppingClock::new(&[]);
    let (outcome, reason) = run_session(&record.dir, &mut peer, &clock).expect("run_session");
    assert_eq!(outcome, Outcome::Succeeded, "reason: {reason}");
    let count = event_lines(&record.dir);
    assert!(count < 200, "events.jsonl holds {count} lines");
}

#[test]
fn deadline_settles_timed_out() {
    let record = invocation();
    let deadline = read_manifest(&record.dir).expect("manifest").deadline_secs;
    let clock = SteppingClock::new(&[
        Duration::from_secs(deadline + 1),
        Duration::from_secs(INTERRUPT_GRACE_SECS + 1),
    ]);
    let mut peer = ScriptedPeer::through_turn_start(Vec::new(), End::Silent).arming(&clock);
    let (outcome, reason) = run_session(&record.dir, &mut peer, &clock).expect("run_session");
    assert_eq!(outcome, Outcome::TimedOut, "reason: {reason}");
    assert!(
        peer.sent_methods().contains(&"turn/interrupt"),
        "no turn/interrupt was sent: {:?}",
        peer.sent_methods()
    );
}

#[test]
fn eof_mid_turn_is_failed_not_succeeded() {
    let record = invocation();
    let lines = vec![agent_message("m1", "Finished the work.", "final_answer")];
    let mut peer = ScriptedPeer::through_turn_start(lines, End::Close);
    let clock = SteppingClock::new(&[]);
    let (outcome, reason) = run_session(&record.dir, &mut peer, &clock).expect("run_session");
    assert_eq!(outcome, Outcome::Failed, "reason: {reason}");
}

#[test]
fn success_requires_released_ownership() {
    let record = invocation();
    let detail = "pid namespace still has members".to_string();
    settle(
        &record.dir,
        Outcome::Succeeded,
        "turn completed",
        Quiescence::Unverified(detail),
    )
    .expect("settle");
    let state = unit_state(&record.work, STAGE, UNIT)
        .expect("unit state")
        .expect("unit has run");
    assert_eq!(state.outcome, Some(Outcome::Succeeded));
    assert_eq!(state.ownership, Ownership::Unverified);
    let disposition = stage_disposition(&record.work, STAGE);
    assert!(
        matches!(disposition, NativeDisposition::Unverified(_)),
        "disposition: {disposition:?}"
    );
}
