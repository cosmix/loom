//! A criterion that hangs: its output reaches the live writer while it runs,
//! and its timeout summary names the test libtest reported still running.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;
use crate::testrun::RunningTests;
use crate::verify::criteria::{format_criterion_result, LiveOutput};

/// A stand-in `cargo` printing libtest's report of a slow test, then hanging.
const FAKE_CARGO: &str = "#!/bin/sh\n\
    echo 'running 2 tests'\n\
    echo 'test quick::passes ... ok'\n\
    echo 'test stuck::forever has been running for over 60 seconds'\n\
    exec sleep 30\n";

/// Appends to a buffer the test can read while the criterion runs.
struct Shared(Arc<Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_hung_criterion_shows_its_output_while_it_runs_and_names_the_stuck_test() {
    let dir = tempfile::TempDir::new().unwrap();
    let cargo = dir.path().join("cargo");
    std::fs::write(&cargo, FAKE_CARGO).unwrap();
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut stage = Stage::new("hung".to_string(), None);
    stage.sandbox.command_confinement = Some(CommandConfinement::Inherit);
    let command = "./cargo test --all-targets".to_string();
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple(command));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let held = Arc::clone(&seen);
    let live = LiveOutput(Arc::new(move |index| {
        assert_eq!(index, 0);
        Some(Box::new(Shared(Arc::clone(&held))) as Box<dyn Write + Send>)
    }));
    let config = CriteriaConfig::with_timeout(Duration::from_secs(2)).with_live_output(live);
    let cwd = dir.path().to_path_buf();
    let run = std::thread::spawn(move || run_acceptance_with_config(&stage, Some(&cwd), &config));

    let deadline = Instant::now() + Duration::from_secs(10);
    let shown = || String::from_utf8_lossy(&seen.lock().unwrap()).contains("stuck::forever");
    while !shown() && !run.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        shown() && !run.is_finished(),
        "the output was not shown before the end"
    );

    let outcome = run.join().unwrap().unwrap();
    let result = &outcome.results()[0];
    assert!(result.timed_out, "{result:?}");
    assert_eq!(
        result.running_at_timeout,
        Some(RunningTests::Named(vec!["stuck::forever".to_string()]))
    );
    assert!(result.stdout.contains("quick::passes ... ok"), "{result:?}");
    let summary = format_criterion_result(0, result, &[]);
    assert!(
        summary.contains("still running at the timeout (1):\n      stuck::forever"),
        "{summary}"
    );
}
