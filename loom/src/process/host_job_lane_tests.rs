//! The host-job lane: this test binary's own tests, run again inside the
//! confinement a host job gives an acceptance criterion (`confined_policy`
//! with a stage cache, the policy `loom stage verify-host` applies). CI runs it
//! on macOS (`.github/workflows/ci.yml`, step "Run the lib tests under host-job
//! confinement") so a test that fails or hangs under that boundary fails the
//! commit that adds it. The probe guards may skip there: the strict CI steps
//! prove the same tests run unconfined.
//!
//! The binary already built by `cargo test` runs confined, so the lane needs
//! no network and no confined build; what it checks is how the tests behave.
//! A run that passes no test fails the lane, so a selection that matches
//! nothing cannot read as a pass.

use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::process::OwnedProcess;
use crate::stage_cache::StageCache;
use crate::verify::criteria::{confined_policy, host_launcher, CommandSpec, ConfinedPolicy};

/// libtest arguments for the confined run, split on whitespace: a filter, or
/// `--exact` and test paths. Unset runs every test that is not ignored.
const SELECTION: &str = "LOOM_HOST_JOB_LANE_ARGS";

/// How long the confined run may take before it is killed and the lane fails.
/// libtest reports every test still running after 60 seconds, so a hang names
/// itself in the log well before this.
const BOUND: Duration = Duration::from_secs(25 * 60);

/// How long the lane waits, once the run ended, for its output to close: a
/// process the run left behind may still hold the pipe.
const DRAIN: Duration = Duration::from_secs(30);

/// The confined run's temporary directory when the lane made it, removed with
/// everything in it. A directory that already exists (bubblewrap's private
/// `/tmp` is named `/tmp`) is left alone.
struct Scratch(Option<PathBuf>);

impl Scratch {
    fn create(path: &str) -> Self {
        let path = PathBuf::from(path);
        if path.exists() {
            return Self(None);
        }
        std::fs::create_dir_all(&path).expect("create the confined run's scratch");
        Self(Some(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

/// How many tests a libtest `test result:` line says passed; 0 for any other line.
fn passed_in(line: &str) -> u64 {
    let counts = line
        .strip_prefix("test result: ")
        .and_then(|rest| rest.split(';').next());
    counts
        .and_then(|first| first.split_whitespace().nth(1))
        .and_then(|count| count.parse().ok())
        .unwrap_or(0)
}

/// Copy the run's output to this test's as it arrives, and send how many
/// tests passed once the output closes.
fn relay_counting_passes(stdout: ChildStdout) -> mpsc::Receiver<u64> {
    let (sender, passes) = mpsc::channel();
    std::thread::spawn(move || {
        let mut passed = 0;
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            println!("{line}");
            passed += passed_in(&line);
        }
        let _ = sender.send(passed);
    });
    passes
}

#[test]
fn a_summary_line_counts_its_passes_and_any_other_line_none() {
    let ok = "test result: ok. 12 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out";
    assert_eq!(passed_in(ok), 12);
    assert_eq!(
        passed_in("test result: FAILED. 2 passed; 1 failed; 0 ignored"),
        2
    );
    assert_eq!(
        passed_in("test result: ok. 0 passed; 0 failed; 0 ignored"),
        0
    );
    assert_eq!(passed_in("test a::b ... ok"), 0);
}

/// This test binary, run with the `SELECTION` arguments under the policy a
/// host job applies to a criterion in this repository.
fn confined_policy_for_this_binary(tree: &Path, cache: &StageCache) -> ConfinedPolicy {
    let exe = std::env::current_exe().expect("locate this test binary");
    let selection = std::env::var(SELECTION).unwrap_or_default();
    let args = selection.split_whitespace().map(str::to_string);
    let spec = CommandSpec::program(exe.display().to_string(), args);
    confined_policy(&spec, tree, cache).expect("build the host-job policy")
}

#[test]
#[ignore = "re-runs the lib tests under host-job confinement; CI's macOS lane runs it"]
fn the_lib_tests_pass_under_host_job_confinement() {
    assert!(host_launcher().is_some(), "no OS sandbox launcher here");
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tree = crate_dir
        .parent()
        .expect("the crate lives in the repository");
    let home = tempfile::tempdir().expect("create a home for the stage cache");
    let cache = StageCache::resolve(tree, "host-job-lane", home.path(), None)
        .expect("resolve the lane's stage cache");
    let policy = confined_policy_for_this_binary(tree, &cache);
    // The policy names the command's scratch directory and removed it when
    // it returned; a host job keeps it for the run, so the lane makes it again.
    let scratch = Scratch::create(policy.env.get("TMPDIR").expect("the policy sets TMPDIR"));

    let mut command = Command::new(&policy.program);
    command
        .args(&policy.args)
        .env_clear()
        .envs(&policy.env)
        .current_dir(crate_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .process_group(0);
    let mut child = command.spawn().expect("start the confined run");
    // Relayed line by line, so the run's progress reaches the log as it happens.
    let passes = relay_counting_passes(child.stdout.take().expect("the run's stdout"));
    let mut run = OwnedProcess::new(child);
    let outcome = run.wait(BOUND).expect("wait for the confined run");
    drop(run);
    drop(scratch);
    let passed = passes.recv_timeout(DRAIN).unwrap_or_else(|_| {
        panic!("the confined run's output stayed open {DRAIN:?} after it ended")
    });
    match outcome {
        Some(status) => assert!(
            status.success(),
            "the lib tests failed under host-job confinement ({status}); see the run above"
        ),
        None => panic!(
            "the lib tests were still running under host-job confinement after {BOUND:?}; \
             the lines above name every test that ran past 60 seconds"
        ),
    }
    assert!(
        passed > 0,
        "the confined run passed no test: {SELECTION} selects nothing"
    );
}
