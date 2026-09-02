//! Update notification — a stderr line when a newer release exists, never an
//! installer. `loom self-update` (`crate::commands::self_update`) stays the
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
//! Every loom command's stdout is somebody's input: `loom plan verify --json`
//! promises JSON-only stdout, `loom usage` emits JSON, `loom config -k <key>`
//! prints a bare scalar other tooling matches against. Printing the update
//! notice to stderr is what keeps all of those safe; the argv exclusion list
//! in `main.rs` (`suppresses_update_check`) is a second line of defence, not
//! the first.
//!
//! # Exactly one fetcher
//!
//! Two invocations of loom racing this module is the common case, not the
//! exception, since loom runs from every hook. `schedule_refresh` takes an
//! `O_EXCL` lock file before spawning so at most one detached fetcher is ever
//! in flight for a given stale interval; a fetch that fails still stamps
//! `last_checked` (`perform_refresh`) so a network outage backs off at the
//! configured interval instead of respawning a fetcher on every invocation.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Duration, Utc};
use semver::Version;

/// The persisted record at `<loom dir>/update-state.json`. Every field is
/// optional so a partial or older record still parses — an unparseable file
/// is treated as fully absent by [`read_state`], never as an error.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct UpdateState {
    #[serde(default)]
    last_checked: Option<DateTime<Utc>>,
    #[serde(default)]
    latest_version: Option<String>,
}

/// What the foreground should do this invocation: print at most one line,
/// and/or hand off to a detached fetcher.
pub(crate) struct Action {
    pub(crate) notice: Option<String>,
    pub(crate) refresh: bool,
}

/// The loom user directory (`~/.loom`, or `$LOOM_HOME` when set to a
/// non-empty value — the same rule as `crate::user_config::config_path`,
/// reimplemented here rather than shared because that module resolves a
/// *file* and may not be edited by this stage). Never creates anything.
fn resolve_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("LOOM_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    dirs::home_dir().map(|home| home.join(".loom"))
}

fn state_path(dir: &Path) -> PathBuf {
    dir.join("update-state.json")
}

fn lock_path(dir: &Path) -> PathBuf {
    dir.join("update-check.lock")
}

/// Read and parse the state file, treating anything short of a fully valid
/// record — missing file, unreadable, torn, or malformed JSON — as absent.
/// A pure `std::fs::read_to_string`, not `crate::fs::locking::locked_read`:
/// that helper locks (and creates) the *parent directory*, and a read must
/// never materialize `~/.loom/` on disk.
fn read_state(dir: &Path) -> Option<UpdateState> {
    let text = std::fs::read_to_string(state_path(dir)).ok()?;
    serde_json::from_str(&text).ok()
}

/// The pure decision: given the last-known state, the two `[update]` config
/// settings, the current time, and the running version, what should this
/// invocation do? Takes `check_enabled`/`interval_hours` rather than
/// `&UserConfig` so this function — the one real branch to test — needs no
/// seam into `user_config` (which this stage may not edit) to exercise the
/// disabled case.
pub(crate) fn decide(
    state: Option<&UpdateState>,
    check_enabled: bool,
    interval_hours: u32,
    now: DateTime<Utc>,
    current: &Version,
) -> Action {
    if !check_enabled {
        return Action {
            notice: None,
            refresh: false,
        };
    }

    let notice = state
        .and_then(|s| s.latest_version.as_deref())
        .and_then(|v| Version::parse(v.trim_start_matches('v')).ok())
        .filter(|latest| latest > current)
        .map(|latest| {
            format!(
                "loom {current} is out of date (latest {latest}) - run `loom self-update` to upgrade."
            )
        });

    let interval = Duration::hours(i64::from(interval_hours));
    let refresh = match state.and_then(|s| s.last_checked) {
        None => true,
        // A `last_checked` in the future (clock skew) yields a negative
        // elapsed, which is never `>= interval` — clock skew counts as
        // fresh rather than triggering a refresh storm.
        Some(last_checked) => now - last_checked >= interval,
    };

    Action { notice, refresh }
}

/// How long a held refresh lock is honoured before it is presumed abandoned,
/// at minimum. `update.check_interval_hours` may legitimately be `0` ("check
/// on every invocation"), which would otherwise make every lock stale the
/// instant it is written: two racing invocations would each take over the
/// other's lock and both spawn a fetcher, which is precisely the concurrency
/// the lock exists to prevent. How long a CRASHED fetcher may block the next
/// one is a different question from how often the user wants to check, so it
/// gets its own floor.
const MIN_LOCK_LIFETIME_MINUTES: i64 = 15;

/// Take the `O_EXCL` refresh lock for `dir`, or report that another
/// invocation already holds a live one. `true` means the caller should spawn
/// a fetcher and, eventually, [`release_lock`]. A held lock older than
/// `interval` (floored at [`MIN_LOCK_LIFETIME_MINUTES`]) is presumed abandoned
/// — its owner crashed before releasing — and taken over once; a second race
/// loss after that backs off.
pub(crate) fn schedule_refresh(dir: &Path, now: DateTime<Utc>, interval: Duration) -> bool {
    let _ = std::fs::create_dir_all(dir);
    let path = lock_path(dir);

    if try_create_lock(&path, now) {
        return true;
    }

    let lock_lifetime = interval.max(Duration::minutes(MIN_LOCK_LIFETIME_MINUTES));
    if lock_is_stale(&path, now, lock_lifetime) {
        let _ = std::fs::remove_file(&path);
        return try_create_lock(&path, now);
    }

    false
}

/// `O_EXCL` create, stamped with `now` so staleness is judged by content
/// (testable with an injected clock) rather than mtime.
fn try_create_lock(path: &Path, now: DateTime<Utc>) -> bool {
    let Ok(mut file) = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    else {
        return false;
    };
    // Best effort: even if the stamp fails to write, the lock file still
    // exists and still excludes other invocations — `lock_is_stale` treats
    // an unparseable stamp as stale, so a stuck writer is still recoverable.
    let _ = file.write_all(now.to_rfc3339().as_bytes());
    true
}

fn lock_is_stale(path: &Path, now: DateTime<Utc>, interval: Duration) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return true;
    };
    let Ok(stamp) = DateTime::parse_from_rfc3339(text.trim()) else {
        return true;
    };
    now - stamp.with_timezone(&Utc) >= interval
}

/// Release the refresh lock. Errors ignored — a missing lock is not this
/// caller's problem.
pub(crate) fn release_lock(dir: &Path) {
    let _ = std::fs::remove_file(lock_path(dir));
}

/// Hidden argv token for the detached refresh child (`loom __update-refresh`,
/// see `main.rs`). Deliberately not a clap subcommand — `main.rs` intercepts
/// it before `Cli::parse()` — so `loom --help` never advertises it.
pub const REFRESH_ARG: &str = "__update-refresh";

/// Guards [`spawn_refresh`] against ever creating a real, process-group-
/// leading child during this crate's own unit tests, mirroring
/// `commands::hook::reconcile_graph::SPAWN_ENABLED`. Defaults to disabled
/// whenever this crate is compiled with `--cfg test` (every `#[cfg(test)]`
/// unit test here, with no extra wiring). It does NOT cover `loom/tests/*.rs`
/// integration targets, which link this crate without `--cfg test`; one of
/// those reaching [`spawn_refresh`] must call [`disable_spawn_for_tests`]
/// itself first — a leaked detached fetcher inside a worktree about to be
/// removed is exactly the failure mode this guard exists to prevent.
static SPAWN_ENABLED: AtomicBool = AtomicBool::new(!cfg!(test));

/// Disable `spawn_refresh` for the remainder of this process. Idempotent.
/// `pub`, not `pub(crate)` and not `#[cfg(test)]`-gated: each file under
/// `loom/tests/*.rs` is its own crate depending on this one, so it needs a
/// real, externally visible item that exists in every build to reach this
/// guard — see `SPAWN_ENABLED`'s doc.
pub fn disable_spawn_for_tests() {
    SPAWN_ENABLED.store(false, Ordering::SeqCst);
}

/// Launch `loom __update-refresh` detached from this process: no stdio, its
/// own process group so it survives this process's exit, and NEVER awaited.
fn spawn_refresh(dir: &Path) -> std::io::Result<()> {
    if !SPAWN_ENABLED.load(Ordering::SeqCst) {
        return Ok(());
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

/// Fetch the latest release's version over the network. Reuses
/// `commands::self_update::get_latest_release` (widened to `pub(crate)` for
/// this call site) rather than a second HTTP client and GitHub API call.
fn fetch_latest_version() -> anyhow::Result<Version> {
    let release = crate::commands::self_update::get_latest_release()?;
    Ok(Version::parse(release.tag_name.trim_start_matches('v'))?)
}

/// The detached child's worker, factored over an injected clock and fetch so
/// it is testable without network access or a real spawn. A failed fetch
/// still stamps `last_checked`: the `O_EXCL` lock in [`schedule_refresh`]
/// stops two fetchers running at once, but if a failed attempt left
/// `last_checked` untouched the record would stay stale forever and every
/// subsequent loom invocation — loom runs from every Claude Code hook —
/// would spawn another fetcher for as long as the network stayed down.
/// Stamping the attempt is what turns the interval into real backoff.
pub(crate) fn perform_refresh<F>(dir: &Path, now: DateTime<Utc>, fetch: F)
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
        let _ = crate::fs::locking::atomic_write_locked(&state_path(dir), &text);
    }

    release_lock(dir);
}

/// Entry point for the detached child (`loom __update-refresh`, invoked from
/// `main.rs`). Fetches, rewrites the state file, releases the lock, and
/// exits. Never prints — its stdio is `/dev/null` (see `spawn_refresh`).
pub fn run_refresh() {
    let Some(dir) = resolve_dir() else { return };
    perform_refresh(&dir, Utc::now(), fetch_latest_version);
}

/// Entry point for every ordinary foreground invocation. Reads the state
/// file, prints at most one line to stderr, and schedules a detached refresh
/// when the record is stale. Takes no network call and never fails.
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
        let interval = Duration::hours(i64::from(config.update_check_interval_hours()));
        if schedule_refresh(&dir, now, interval) && spawn_refresh(&dir).is_err() {
            release_lock(&dir);
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
