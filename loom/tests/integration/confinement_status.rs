//! `loom status` under `srt` with the state directory write-denied
//! (`doc/plans/PLAN-loom-state-confinement.md`, section 14 item 9): it must
//! run, and leave the state directory byte-identical.
//!
//! Linux only, and skipped unless `bwrap`, `socat` and `srt` are on PATH and a
//! trivial `srt -c true` runs: a nested sandbox can refuse the namespaces or
//! the sockets srt needs. The srt settings are built by hand for this check:
//! the repository and a scratch `LOOM_HOME` writable, the state directory
//! denied, the network closed.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};
use tempfile::TempDir;

use crate::helpers::{clear_relay_env, init_test_repo, loom_bin_path};
use loom::models::stage::Stage;
use loom::process::sandbox_probe::skip_unless;
use loom::verify::transitions::save_stage;

/// The directories `WorkDir::load` creates when one is missing. All exist
/// here, so a status run has no reason to create one.
const STATE_DIRS: [&str; 10] = [
    "signals", "handoffs", "archive", "stages", "sessions", "crashes", "memory", "wrappers",
    "pids", "logs",
];

fn srt_settings(allow_write: &[&Path], deny_write: &[&Path]) -> Value {
    let paths = |list: &[&Path]| -> Vec<String> {
        list.iter().map(|path| path.display().to_string()).collect()
    };
    json!({
        "filesystem": {
            "denyRead": [],
            "allowWrite": paths(allow_write),
            "denyWrite": paths(deny_write),
        },
        "network": {"allowedDomains": [], "deniedDomains": []},
    })
}

fn on_path(tool: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(tool).is_file()))
}

/// How much of a run's stderr a failure message quotes.
const STDERR_LINES: usize = 40;

/// `output`'s exit status, stdout, and stderr (whole, or its first
/// `STDERR_LINES` lines), for a failure message. A crashed srt prints its
/// exception above Node's closing version line, so the last line alone
/// hides it.
fn diagnostics(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stderr.lines().collect();
    let cut = lines.len().saturating_sub(STDERR_LINES);
    let more = if cut > 0 {
        format!("\n[{cut} more lines]")
    } else {
        String::new()
    };
    format!(
        "{}\nstdout:\n{}\nstderr:\n{}{more}",
        output.status,
        String::from_utf8_lossy(&output.stdout).trim_end(),
        lines[..lines.len() - cut].join("\n")
    )
}

/// Why this host cannot run srt, or `None` when it can.
fn srt_unavailable(scratch: &Path) -> Option<String> {
    if !cfg!(target_os = "linux") {
        return Some("the srt checks run on Linux only".to_string());
    }
    if let Some(tool) = ["bwrap", "socat", "srt"]
        .into_iter()
        .find(|tool| !on_path(tool))
    {
        return Some(format!("`{tool}` is not on PATH"));
    }
    let settings = scratch.join("probe.srt.json");
    fs::write(&settings, srt_settings(&[], &[]).to_string()).unwrap();
    let output = Command::new("srt")
        .arg("--settings")
        .arg(&settings)
        .args(["-c", "true"])
        .stdin(Stdio::null())
        .output();
    match output {
        Ok(output) if output.status.success() => None,
        Ok(output) => Some(format!(
            "`srt -c true` fails here, so this host cannot nest srt's sandbox: {}",
            diagnostics(&output)
        )),
        Err(error) => Some(format!("cannot run srt: {error}")),
    }
}

/// Every entry under `dir`, relative to it, with a file's bytes or a
/// symlink's target; a directory maps to `None`.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut entries = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).unwrap() {
            let path = entry.unwrap().path();
            let kind = fs::symlink_metadata(&path).unwrap().file_type();
            let content = if kind.is_symlink() {
                let target = fs::read_link(&path).unwrap();
                Some(target.to_string_lossy().into_owned().into_bytes())
            } else if kind.is_dir() {
                pending.push(path.clone());
                None
            } else {
                Some(fs::read(&path).unwrap())
            };
            entries.insert(path.strip_prefix(dir).unwrap().to_path_buf(), content);
        }
    }
    entries
}

/// `repo`'s state directory, complete, with one stage for `loom status` to
/// read.
fn state_dir(repo: &Path) -> PathBuf {
    let work_dir = repo.join(".loom").join("work");
    for dir in STATE_DIRS {
        fs::create_dir_all(work_dir.join(dir)).unwrap();
    }
    fs::write(work_dir.join("config.toml"), "").unwrap();
    let stage = Stage {
        id: "stage-1".to_string(),
        name: "Stage One".to_string(),
        ..Stage::default()
    };
    save_stage(&stage, &work_dir).unwrap();
    work_dir
}

/// `loom status` from `repo` with no session identity and update checks off,
/// under srt with `settings` when given, directly otherwise.
fn loom_status(repo: &Path, loom_home: &Path, settings: Option<&Path>) -> Output {
    let mut command = match settings {
        Some(settings) => {
            let mut srt = Command::new("srt");
            srt.arg("--settings").arg(settings).arg(loom_bin_path());
            srt
        }
        None => Command::new(loom_bin_path()),
    };
    clear_relay_env(&mut command);
    command
        .arg("status")
        .env("LOOM_HOME", loom_home)
        .current_dir(repo)
        .stdin(Stdio::null())
        .output()
        .expect("run loom status")
}

#[test]
fn loom_status_runs_with_the_state_dir_write_denied_and_changes_nothing() {
    let scratch = TempDir::new().unwrap();
    let reason = srt_unavailable(scratch.path());
    let test_name = "loom_status_runs_with_the_state_dir_write_denied_and_changes_nothing";
    if skip_unless(
        reason.is_none(),
        test_name,
        reason.as_deref().unwrap_or_default(),
    ) {
        return;
    }
    let repo_dir = init_test_repo();
    let repo = repo_dir.path().canonicalize().unwrap();
    let work_dir = state_dir(&repo);
    let loom_home = scratch.path().join("loom-home");
    fs::create_dir_all(&loom_home).unwrap();
    fs::write(loom_home.join("config.toml"), "[update]\ncheck = false\n").unwrap();
    let settings = scratch.path().join("status.srt.json");
    let document = srt_settings(&[&repo, &loom_home], &[&work_dir]);
    fs::write(&settings, document.to_string()).unwrap();
    let before = snapshot(&work_dir);

    let output = loom_status(&repo, &loom_home, Some(&settings));

    if !output.status.success() {
        // Tell a state write apart from a fixture `loom status` rejects anyway.
        let control = loom_status(&repo, &loom_home, None);
        panic!(
            "`loom status` failed with {} write-denied: {}\nthe same run outside srt: {}",
            work_dir.display(),
            diagnostics(&output),
            diagnostics(&control)
        );
    }
    assert_eq!(
        snapshot(&work_dir),
        before,
        "`loom status` changed the state directory"
    );
}
