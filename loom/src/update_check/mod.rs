//! Update notification — a stderr line when a newer release exists, never an
//! installer. `loom update` (`crate::commands::self_update`) stays the
//! only thing that writes a new binary; this module only reads a small state
//! file and, at most, spawns a detached child to refresh it.
//!
//! # The hot path takes no network call
//!
//! `loom` is invoked constantly from Claude Code hooks, so a synchronous
//! fetch on every launch would be a latency disaster. [`notify_and_maybe_refresh`]
//! only ever reads `update-state.json` from disk; the actual GitHub request
//! happens in a DETACHED child (`loom __update-refresh`, [`run_refresh`]),
//! spawned at most once per stale interval and never awaited.
//!
//! # Failures are silent everywhere
//!
//! A network error, an unreadable or torn state file, or an unresolvable home
//! directory must never make a loom command fail, panic, or print noise. None
//! of the IO in this module unwraps or expects; every fallible read collapses
//! to "absent" and every fallible write is best-effort.
//!
//! # The notice goes to stderr, never stdout
//!
//! Every loom command's stdout is somebody's input (`loom plan verify --json`,
//! `loom usage`, `loom config -k <key>`), so the notice goes to stderr; the
//! argv exclusion list in `main.rs` (`suppresses_update_check`) is a second
//! line of defence, not the first.
//!
//! # Exactly one fetcher
//!
//! Two invocations of loom racing this module is the common case, since loom
//! runs from every hook. Each worker takes an OS lock and rechecks freshness
//! before fetching;
//! a fetch that fails still stamps `last_checked` so a network outage backs
//! off instead of respawning on every invocation.

mod decide;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::{DateTime, Utc};
use fs2::FileExt;
use semver::Version;

use decide::decide;
#[cfg(test)]
use decide::notice_for;

/// The persisted record at `<loom dir>/update-state.json`, every field
/// optional so a partial or older record still parses; anything unparseable
/// reads as fully absent instead ([`read_state`]).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct UpdateState {
    #[serde(default)]
    last_checked: Option<DateTime<Utc>>,
    #[serde(default)]
    latest_version: Option<String>,
}

/// The loom user directory: the parent of `crate::user_config::config_path()`
/// (`~/.loom`, or `$LOOM_HOME`). Delegates so the two rules can never drift
/// apart. Has no `#[cfg(test)]` redirect, so tests here pass an explicit
/// `tempfile::tempdir()` instead of calling this. Never creates anything.
fn resolve_dir() -> Option<PathBuf> {
    crate::user_config::config_path()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

fn state_path(dir: &Path) -> PathBuf {
    dir.join("update-state.json")
}

fn lock_path(dir: &Path) -> PathBuf {
    dir.join("update-check.lock")
}

/// Read and parse the state file, treating anything short of a fully valid
/// record — missing file, unreadable, torn, or malformed JSON — as absent. A
/// pure `read_to_string`, not `locking::locked_read`, which locks (and
/// creates) the parent directory — a read must never materialize `~/.loom/`.
fn read_state(dir: &Path) -> Option<UpdateState> {
    let text = std::fs::read_to_string(state_path(dir)).ok()?;
    serde_json::from_str(&text).ok()
}

/// The minimum interval between refresh attempts, including failed attempts.
const MIN_REFRESH_INTERVAL_MINUTES: i64 = 15;

/// The worker holds this advisory lock until it exits. Never unlink the file:
/// another worker could otherwise acquire a lock on a different inode.
fn acquire_refresh_lock(dir: &Path) -> Option<std::fs::File> {
    std::fs::create_dir_all(dir).ok()?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path(dir))
        .ok()?;
    FileExt::try_lock_exclusive(&file).ok()?;
    Some(file)
}

/// Hidden argv token for the detached refresh child. Not a clap subcommand —
/// `main.rs` intercepts it before `Cli::parse()` — so `--help` never shows it.
pub const REFRESH_ARG: &str = "__update-refresh";

/// Launch `loom __update-refresh` detached from this process: no stdio, its
/// own process group, and NEVER awaited. `process_group(0)` moves the child
/// out of this process's group but not out from under it as a parent — it is
/// reparented to init only once THIS process exits, immediate for the
/// short-lived commands that dominate. Nothing ever waits on the child, so on
/// a long-lived foreground command (a status/attach TUI) a fetcher that
/// finishes first sits as `<defunct>` until the parent exits — harmless.
fn spawn_refresh(dir: &Path) -> std::io::Result<()> {
    // No unit test in this crate may fork a detached child into a worktree
    // that may be removed once its stage merges. `loom/tests/*.rs`
    // integration targets link this crate without `--cfg test` and are
    // unaffected — they exercise the real binary via a scratch `LOOM_HOME`
    // with the check switched off (`tests/integration/helpers.rs`).
    if cfg!(test) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "spawn_refresh is disabled under #[cfg(test)]",
        ));
    }

    let program = std::env::current_exe()?;
    let mut command = Command::new(program);
    command
        .arg(REFRESH_ARG)
        // The user's home, never the worktree this process may be running
        // in: a worktree is removed once its stage merges, and a fetcher
        // that outlives the parent must not depend on it still existing.
        .current_dir(dirs::home_dir().unwrap_or_else(|| dir.to_path_buf()))
        // All three: loom runs as a child of piped callers
        // (`verify/criteria/confine.rs`), and an inherited stdout/stderr fd
        // would keep that pipe open after loom exits, making the collector
        // block for the full output-collection timeout on every invocation.
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        // Safe-Rust process-group reassignment, not a `pre_exec` `setsid`:
        // this crate denies undocumented unsafe blocks, and the existing
        // detached-spawn precedent (`reconcile_graph::spawn_detached`) shows
        // `process_group(0)` is enough to survive the parent's exit.
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    command.spawn()?;
    Ok(())
}

/// Fetch the latest release's version, reusing
/// `commands::self_update::get_latest_release` (widened to `pub(crate)`).
fn fetch_latest_version() -> anyhow::Result<Version> {
    let release = crate::commands::self_update::get_latest_release()?;
    Ok(Version::parse(release.tag_name.trim_start_matches('v'))?)
}

/// The detached child's worker, factored over an injected clock and fetch so
/// it is testable without network access or a real spawn. A failed fetch
/// still stamps `last_checked`: the worker-held lock
/// stops two fetchers running at once, but if a failed attempt left
/// `last_checked` untouched, every subsequent loom invocation — loom runs
/// from every hook — would spawn another fetcher for as long as the network
/// stayed down. Stamping the attempt is what turns the interval into backoff.
fn perform_refresh<F>(dir: &Path, now: DateTime<Utc>, fetch: F)
where
    F: FnOnce() -> anyhow::Result<Version>,
{
    let mut state = read_state(dir).unwrap_or_default();

    if let Ok(version) = fetch() {
        state.latest_version = Some(version.to_string());
    }
    state.last_checked = Some(now);

    let _ = std::fs::create_dir_all(dir);
    if let Ok(text) = serde_json::to_string(&state) {
        // Outside a held directory lock, which `atomic_write_locked`'s own
        // doc flags as reintroducing lost-update/torn-read races — safe here
        // because writers are serialized by the worker-held lock, and readers
        // use a plain `read_to_string` against a path this helper only ever
        // `rename`s into place, so the worst case is a redundant whole-file
        // write, never a torn read.
        let _ = crate::fs::locking::atomic_write_locked(&state_path(dir), &text);
    }
}

/// Entry point for the detached child. Fetches, rewrites the state file,
/// releases the lock, and exits. Never prints — its stdio is `/dev/null`.
pub fn run_refresh() {
    let Some(dir) = resolve_dir() else { return };
    let Some(_lock) = acquire_refresh_lock(&dir) else {
        return;
    };
    let now = Utc::now();
    let config = crate::user_config::UserConfig::load();
    if config.update_check()
        && decide::refresh_due(
            read_state(&dir).as_ref(),
            config.update_check_interval_hours(),
            now,
        )
    {
        perform_refresh(&dir, now, fetch_latest_version);
    }
}

/// Entry point for every ordinary foreground invocation: reads the state
/// file, prints at most one line to stderr, and schedules a detached refresh
/// when stale. A dev build (see `decide`) prints nothing and schedules no
/// refresh. Takes no network call and never fails.
pub fn notify_and_maybe_refresh() {
    let Some(dir) = resolve_dir() else { return };
    let Ok(current) = Version::parse(env!("LOOM_VERSION")) else {
        return;
    };

    let config = crate::user_config::UserConfig::load();
    let state = read_state(&dir);
    let now = Utc::now();

    let action = decide(
        state.as_ref(),
        config.update_check(),
        config.update_check_interval_hours(),
        now,
        &current,
    );

    if let Some(notice) = &action.notice {
        eprintln!("{notice}");
    }

    if action.refresh {
        // Avoid spawning while a worker is active. The worker must reacquire
        // the lock and recheck freshness because another launcher can race us.
        if let Some(lock) = acquire_refresh_lock(&dir) {
            let due = decide::refresh_due(
                read_state(&dir).as_ref(),
                config.update_check_interval_hours(),
                Utc::now(),
            );
            drop(lock);
            if due {
                let _ = spawn_refresh(&dir);
            }
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
