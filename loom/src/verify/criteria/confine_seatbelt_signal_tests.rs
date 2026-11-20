//! A confined command signals what it started and nothing else: its own
//! children and process groups, never a process started outside its sandbox or
//! inside a separately started one. Each runtime test goes through loom's own
//! confinement path and prints the whole command result on failure.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

use crate::models::stage::CommandConfinement;
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};
use crate::stage_cache::StageCache;
use crate::verify::criteria::{
    confined_policy, host_launcher, run_spec_with_timeout, spawn_confined, CommandSpec,
    CriterionResult,
};

const SIGNAL_RULE: &str = "(allow signal (target same-sandbox))";
const TIMEOUT: Duration = Duration::from_secs(20);

fn canonical_tree() -> (TempDir, PathBuf) {
    let root = TempDir::new().unwrap();
    let tree = root.path().canonicalize().unwrap();
    (root, tree)
}

fn run_confined(script: &str, tree: &Path) -> CriterionResult {
    let spec = CommandSpec::shell(script);
    run_spec_with_timeout(&spec, Some(tree), TIMEOUT, CommandConfinement::Confined).unwrap()
}

/// A `sleep 30` started outside every sandbox, in a group of its own. It
/// is killed and reaped on drop, so a failing test leaves none behind.
struct OutsideSleeper(Child);

impl OutsideSleeper {
    fn start() -> Self {
        let child = Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Self(child)
    }

    /// Kill and reap the sleeper, failing when either does not happen.
    fn stop(&mut self) {
        self.0.kill().expect("the outside sleeper is killed");
        reap(&mut self.0).expect("the outside sleeper is reaped");
    }
}

impl Drop for OutsideSleeper {
    fn drop(&mut self) {
        if still_running(&mut self.0) {
            let _ = self.0.kill();
            let _ = reap(&mut self.0);
        }
    }
}

/// Wait for `child` to exit, at most 5 s.
fn reap(child: &mut Child) -> std::io::Result<ExitStatus> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let kind = std::io::ErrorKind::TimedOut;
            return Err(std::io::Error::new(
                kind,
                "the child did not exit within 5 s",
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn still_running(child: &mut Child) -> bool {
    matches!(child.try_wait(), Ok(None))
}

fn assert_refused(result: &CriterionResult) {
    assert!(
        result.stdout.contains("status=1") && result.stderr.contains("Operation not permitted"),
        "the kill was not refused: {result:?}"
    );
}

/// The rule is pinned in the generated profile wherever the launcher exists,
/// even where no sandbox can start, and no unqualified signal allowance is.
#[test]
fn the_profile_allows_signals_only_within_the_same_sandbox() {
    let name = "the_profile_allows_signals_only_within_the_same_sandbox";
    if skip_unless(host_launcher().is_some(), name, "sandbox-exec is absent") {
        return;
    }
    let ((_r, repo), (_h, home), (_t, tree)) =
        (canonical_tree(), canonical_tree(), canonical_tree());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let policy = confined_policy(&CommandSpec::shell("true"), &tree, &cache).unwrap();
    let at = policy.args.iter().position(|arg| arg == "-p").unwrap();
    let profile = &policy.args[at + 1];
    let rule = profile.find(SIGNAL_RULE);
    let deny = profile.find("(deny default)").unwrap();
    assert!(rule.is_some_and(|rule| rule > deny), "{profile}");
    assert_eq!(profile.matches("(allow signal").count(), 1, "{profile}");
}

#[test]
fn a_confined_command_kills_its_own_child_and_process_group() {
    if !confined_commands_runnable() {
        return;
    }
    let (_root, tree) = canonical_tree();
    let result = run_confined(
        "sleep 30 & child=$!; kill -KILL \"$child\"; echo \"child=$?\"; \
         perl -e 'setpgrp(0, 0); sleep 30' & group=$!; \
         n=0; until kill -0 -- \"-$group\" 2>/dev/null || [ $n -ge 100 ]; do \
         sleep 0.05; n=$((n + 1)); done; \
         kill -KILL -- \"-$group\"; echo \"group=$?\"; wait",
        &tree,
    );
    assert!(
        result.success && result.stdout.contains("child=0") && result.stdout.contains("group=0"),
        "{result:?}"
    );
    assert!(result.duration < Duration::from_secs(15), "{result:?}");
}

#[test]
fn a_confined_command_cannot_signal_a_process_started_outside_it() {
    if !confined_commands_runnable() {
        return;
    }
    let (_root, tree) = canonical_tree();
    let mut outside = OutsideSleeper::start();
    let script = format!("kill -KILL {}; echo \"status=$?\"", outside.0.id());
    let result = run_confined(&script, &tree);
    let survived = still_running(&mut outside.0);
    outside.stop();
    assert_refused(&result);
    assert!(survived, "the outside process was killed: {result:?}");
}

#[test]
fn a_confined_command_cannot_signal_another_confined_command() {
    if !confined_commands_runnable() {
        return;
    }
    let (_root, tree) = canonical_tree();
    let marker = tree.join("first-ready");
    let first = CommandSpec::shell("echo ready > first-ready; exec sleep 30");
    let mut first = spawn_confined(&first, Some(&tree), CommandConfinement::Confined).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(marker.exists(), "the first confined command did not start");
    let script = format!("kill -KILL {}; echo \"status=$?\"", first.child().id());
    let result = run_confined(&script, &tree);
    assert_refused(&result);
    assert!(
        still_running(first.child()),
        "the other sandbox's process was killed: {result:?}"
    );
}
