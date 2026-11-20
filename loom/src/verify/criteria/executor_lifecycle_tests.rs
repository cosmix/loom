use super::*;
use crate::process::lifecycle_tests::InheritorGuard;

/// True when a host job's profile refuses the process-group signals these tests need.
fn cannot_signal_groups(name: &str) -> bool {
    crate::process::sandbox_probe::skip_unless(
        crate::process::sandbox_probe::process_groups_signalable(),
        name,
        "this process cannot signal a child's process group",
    )
}

#[test]
fn verification_retires_stdout_inheritor_on_leader_exit() {
    if cannot_signal_groups("verification_retires_stdout_inheritor_on_leader_exit") {
        return;
    }
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
    if cannot_signal_groups("verification_timeout_kills_before_collecting_and_reports_elapsed") {
        return;
    }
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

#[test]
fn capture_error_after_partial_output_is_incomplete() {
    struct Failing(bool);
    impl Read for Failing {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            if self.0 {
                return Err(std::io::Error::other("partial failure"));
            }
            self.0 = true;
            bytes[0] = b'x';
            Ok(1)
        }
    }
    let capture = read_stream(Failing(false));
    assert!(!capture.complete);
    assert!(capture.text.starts_with('x'));
}
