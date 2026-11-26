use super::*;
use crate::process::lifecycle_tests::InheritorGuard;

#[test]
fn verification_retires_stdout_inheritor_on_leader_exit() {
    crate::test_stand_in::require_orphan_signals!();
    let temp = tempfile::tempdir().unwrap();
    let guard = InheritorGuard::new(temp.path());
    let start = Instant::now();
    let result = run_spec_with_timeout(
        &CommandSpec::shell(format!("{} echo reply", guard.shell())),
        Some(temp.path()),
        Duration::from_secs(10),
        CommandConfinement::Inherit,
    )
    .unwrap();
    assert!(start.elapsed() < Duration::from_secs(4));
    assert!(result.success && result.output_complete);
    assert_eq!(result.stdout.trim(), "reply");
    guard.assert_retired();
}

#[test]
fn verification_timeout_kills_before_collecting_and_reports_elapsed() {
    crate::test_stand_in::require_orphan_signals!();
    let temp = tempfile::tempdir().unwrap();
    let guard = InheritorGuard::new(temp.path());
    let start = Instant::now();
    let result = run_spec_with_timeout(
        &CommandSpec::shell(format!("{} sleep 30", guard.shell())),
        Some(temp.path()),
        Duration::from_millis(200),
        CommandConfinement::Inherit,
    )
    .unwrap();
    assert!(result.timed_out && !result.success);
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(result.duration >= Duration::from_millis(200));
    assert!(start.elapsed().saturating_sub(result.duration) < Duration::from_millis(100));
    guard.assert_retired();
}

/// The sandbox's pid 1 keeps the command's stdio until its own exit ends, and
/// leads a session of its own, outside the leader's group: no group poll sees
/// it, so a confined run's collection waits for end of file instead.
#[cfg(target_os = "linux")]
#[test]
fn a_confined_commands_sandbox_init_leads_its_own_session() {
    use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};
    let test = "a_confined_commands_sandbox_init_leads_its_own_session";
    if skip_unless(confined_commands_runnable(), test, "it runs bubblewrap") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let script = "read -r _ _ _ _ group session _ < /proc/1/stat; echo \"$group $session\"";
    let confined = CommandConfinement::Confined;
    let spec = CommandSpec::shell(script);
    let prepared = confine::prepare_confined(&spec, Some(temp.path()), None, confined, None);
    let prepared = prepared.unwrap();
    assert!(prepared.contains_its_processes());
    let result = run_prepared_with_timeout(prepared, Duration::from_secs(30)).unwrap();
    assert!(result.success && result.output_complete, "{result:?}");
    assert_eq!(result.stdout.trim(), "1 1");
}
