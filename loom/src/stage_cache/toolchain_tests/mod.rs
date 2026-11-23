//! Real-tool probes of the stage cache, one module per ecosystem. Each test runs
//! a real tool twice through [`run_spec_in_stage`] against ONE [`StageCache`]
//! and asserts the tool's own evidence of reuse.
//!
//! The runs are confined, so they need an OS launcher that can start inside
//! this process: where a trivial confined command fails (a session already
//! under a sandbox cannot nest another), every test skips, and
//! `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` turns each skip into a failure. A tool
//! that is not installed skips the same way.
//!
//! A confined command takes its environment from this process, so the runs use
//! the real `HOME` (the toolchains and host seeds live under it; the sandbox
//! keeps it read-only) with the cache base moved to a temporary
//! `XDG_CACHE_HOME` under `/tmp`. The host job's sccache socket sits in a
//! directory only the command can reach (`/tmp` under bubblewrap, the
//! command's scratch directory on macOS), never in the cache root, so sccache gets
//! `SCCACHE_SERVER_UDS` instead of falling back to localhost TCP, which the
//! boundary denies. The tests read the process
//! `HOME` and `PATH`, which other tests pin, so every one is `#[serial]`.

mod c_cpp;
mod go;
mod js;
mod jvm;
mod python;
mod rust;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use crate::models::stage::CommandConfinement;
use crate::process::sandbox_probe::skip_unless;
use crate::stage_cache::StageCache;
use crate::verify::criteria::{host_launcher, run_spec_in_stage, CommandSpec, CriterionResult};

/// A cold toolchain build (Go's standard library, a CMake configure) is slow.
const TIMEOUT: Duration = Duration::from_secs(600);

/// One stage's worth of state: a working tree and the stage cache beside it.
pub(super) struct Rig {
    _dirs: Vec<tempfile::TempDir>,
    pub tree: PathBuf,
    pub cache: StageCache,
}

impl Rig {
    /// A rig when the OS launcher works and every tool in `tools` is installed;
    /// otherwise the test skips (or fails under `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`).
    pub fn start(test: &str, tools: &[&str]) -> Option<Rig> {
        for tool in tools {
            let found = which::which(tool).is_ok();
            if skip_unless(found, test, &format!("{tool} not installed")) {
                return None;
            }
        }
        let launcher = host_launcher().is_some();
        if skip_unless(launcher, test, "OS sandbox launcher absent") {
            return None;
        }
        if skip_unless(confined_runs_start(), test, NESTING) {
            return None;
        }
        let rig = Rig::bare();
        skip_unless(
            rig.is_some(),
            test,
            "HOME is not set or /tmp is not writable",
        );
        rig
    }

    fn bare() -> Option<Rig> {
        let home = PathBuf::from(std::env::var_os("HOME").filter(|home| !home.is_empty())?);
        // The cache base sits under /tmp, never under the real HOME.
        let dirs = vec![
            tempfile::tempdir().unwrap(),
            tempfile::tempdir_in("/tmp").ok()?,
            tempfile::tempdir().unwrap(),
        ];
        let [repo, xdg, tree] = [0, 1, 2].map(|i| dirs[i].path().canonicalize().unwrap());
        let cache = StageCache::resolve(&repo, "toolchain", &home, Some(&xdg)).unwrap();
        Some(Rig {
            _dirs: dirs,
            tree,
            cache,
        })
    }

    /// Writes `content` to `relative` in the tree, creating its directories.
    pub fn write(&self, relative: &str, content: &str) {
        let path = self.tree.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    /// Runs the shell `script` confined in the tree, with the stage cache.
    pub fn run(&self, script: &str) -> CriterionResult {
        let spec = CommandSpec::shell(script);
        let confinement = CommandConfinement::Confined;
        let tree = Some(self.tree.as_path());
        run_spec_in_stage(&spec, tree, tree, TIMEOUT, confinement, &self.cache).unwrap()
    }

    /// Runs `first`, then `second`, as two separate confined commands against
    /// the one cache; both must succeed.
    pub fn run_twice(&self, first: &str, second: &str) -> (CriterionResult, CriterionResult) {
        let first = self.run(first);
        assert!(
            first.success,
            "first run failed: {first:?}{}",
            self.explain()
        );
        let second = self.run(second);
        assert!(
            second.success,
            "second run failed: {second:?}{}",
            self.explain()
        );
        (first, second)
    }

    /// What a failed probe needs to tell a hidden host path from a changed
    /// environment: this process's `HOME` and the one it started with, then
    /// each `PATH` directory and the `HOME`-based seed roots as a confined
    /// command sees them.
    pub fn explain(&self) -> String {
        // Split on `:` alone, so a PATH entry holding a space stays whole.
        let script = "echo \"inside HOME=$HOME\"; IFS=:; set -f; \
            for d in $PATH:$HOME/.bun/bin:$HOME/go/pkg/mod:$HOME/.m2/repository:$HOME/.gradle/caches; \
            do if [ -d \"$d\" ]; then echo \"seen $d\"; else echo \"hidden $d\"; fi; done";
        let inside = self.run(script);
        format!(
            "\n-- probe context --\nprocess HOME={:?}\nstarting HOME={:?}\n{}{}",
            std::env::var_os("HOME"),
            starting_home(),
            inside.stdout,
            inside.stderr
        )
    }

    /// The cache root's canonical path: where the tools' variables point.
    pub fn root(&self) -> PathBuf {
        self.cache.root().canonicalize().unwrap()
    }
}

const NESTING: &str = "a confined command cannot start here (nested sandbox or launcher failure)";

/// Whether a trivial confined command runs at all; probed once per process.
fn confined_runs_start() -> bool {
    static STARTS: OnceLock<bool> = OnceLock::new();
    *STARTS.get_or_init(|| {
        let Some(rig) = Rig::bare() else { return false };
        let spec = CommandSpec::shell("true");
        let tree = Some(rig.tree.as_path());
        let confinement = CommandConfinement::Confined;
        run_spec_in_stage(&spec, tree, tree, TIMEOUT, confinement, &rig.cache)
            .is_ok_and(|result| result.success)
    })
}

/// A seed named by `variable`, or a skip when it is unset. The value ends up in
/// a shell line and a build file, so it is limited to coordinate characters.
pub(super) fn seed(test: &str, variable: &str) -> Option<String> {
    let value = std::env::var(variable)
        .ok()
        .filter(|value| !value.is_empty());
    if skip_unless(value.is_some(), test, &format!("{variable} is not set")) {
        return None;
    }
    let value = value.unwrap();
    let allowed = |c: char| c.is_ascii_alphanumeric() || "._-/@:+~".contains(c);
    assert!(
        value.chars().all(allowed),
        "{variable}={value:?} is not a plain coordinate"
    );
    Some(value)
}

/// Standard output and error of `result`, one string.
pub(super) fn output(result: &CriterionResult) -> String {
    format!("{}\n{}", result.stdout, result.stderr)
}

/// Every path below `root` with its size: a before/after comparison of a host
/// directory the confined commands must not change. A missing root is empty.
pub(super) fn listing(root: &Path) -> BTreeSet<(PathBuf, u64)> {
    let mut found = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.path().symlink_metadata() else {
                continue;
            };
            if metadata.is_dir() {
                pending.push(entry.path());
            }
            found.insert((entry.path(), metadata.len()));
        }
    }
    found
}

/// `HOME` as the process received it at exec, where the OS exposes that
/// (`/proc/self/environ` on Linux); a test that pinned `HOME` and never
/// restored it shows up as a difference from the current value.
fn starting_home() -> Option<String> {
    let environ = std::fs::read("/proc/self/environ").ok()?;
    environ
        .split(|byte| *byte == 0)
        .find_map(|entry| entry.strip_prefix(b"HOME="))
        .map(|home| String::from_utf8_lossy(home).into_owned())
}

/// The operator's home directory, for host seeds.
pub(super) fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("HOME is set"))
}
