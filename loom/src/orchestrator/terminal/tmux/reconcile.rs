//! Converges the tiled attach viewer (built once by `loom attach` with no
//! argument, see `commands/attach/overview.rs`) onto reality, one scheduler
//! tick at a time.
//!
//! `loom attach` builds that viewer ONCE and `exec`s into it; nothing else
//! updates it afterwards, so a finished stage leaves a dead pane forever and
//! a freshly spawned stage never appears until the operator detaches and
//! re-runs `loom attach`. This module is what turns that one-shot snapshot
//! into a live piece of glass, run best-effort from the daemon's scheduler
//! loop. It never CREATES the viewer — only `loom attach` does that; this
//! module only maintains one the operator already built.
//!
//! The identity/discovery half this module builds on — the viewer socket
//! name, the per-session pane command, and which sessions are attachable
//! right now — lives in [`super::viewer`]. Every tmux call here is bounded
//! (`super::run_tmux_control` plus `TMUX_PROBE_TIMEOUT`) because it runs on
//! the single scheduler loop, where a hang would stall every other stage's
//! polling too.

use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::{run_tmux_control, socket_path_for, TMUX_PROBE_TIMEOUT};

/// One pane of the viewer window, as tmux reports it.
struct PaneInfo {
    id: String,
    dead: bool,
    /// The inner session socket this pane's attach client targets, parsed by
    /// [`parse_pane_socket`]. `None` means this pane is not attributable to
    /// loom at all — the operator's own split (see [`reconcile_steps`]'s
    /// Rule 5).
    session_socket: Option<String>,
}

/// Parse the inner-server socket out of a viewer pane's recorded start
/// command (`#{pane_start_command}`), so this module can tell a loom pane
/// from the operator's own split.
///
/// Verified on tmux 3.6a: `#{pane_start_command}` survives for panes made by
/// both `new-session` and `split-window`, rendering the launched argv
/// shell-quoted, e.g.:
///
/// ```text
/// sh -c "unset TMUX; exec tmux -L loom-session-abc12345-1754900000 attach-session -t some-key"
/// ```
///
/// Parsed DEFENSIVELY — a token search, not a regex pinned to that exact
/// template, because [`super::viewer::pane_command`]'s exact shape is that
/// module's implementation detail, not a contract this one should re-derive:
///
/// 1. The string must contain `attach-session`, or this is not a loom pane.
/// 2. Split on whitespace and find the first `-L` token; take the token
///    right after it. No `-L`, or nothing after it, means this is not a
///    loom pane.
/// 3. The token must be quote-SYMMETRIC — quoted at both ends or neither.
///    A quoted value containing whitespace splits at step 2, handing back a
///    one-sided fragment like `'loom-a`; misattributing it would make Rule 1
///    re-add the "missing" real socket on every tick, growing the viewer by
///    one pane per tick until `split-window` runs out of room.
/// 4. Strip any surrounding `"` or `'` from that token.
/// 5. The result must start with `loom-` and contain only `[A-Za-z0-9_-]`
///    (the id charset `validate_id` enforces, and session sockets are
///    `loom-<session.id>` — see `super::socket_name`).
///
/// Rule 5 is a deliberate SAFETY TIGHTENING, not decoration. Without it, an
/// operator who split their own pane and ran a plain `tmux attach-session`
/// there would be attributed as a loom pane by the earlier rules and then
/// killed by [`reconcile_steps`]'s duplicate/removal logic. Loom session ids
/// (`session-<uuid8>-<unixts>`, `[A-Za-z0-9-]` only) need no unescaping, so
/// the stripped token is unambiguous once rule 5 passes.
fn parse_pane_socket(start_command: &str) -> Option<String> {
    if !start_command.contains("attach-session") {
        return None;
    }
    let tokens: Vec<&str> = start_command.split_whitespace().collect();
    let position = tokens.iter().position(|&token| token == "-L")?;
    let raw = tokens.get(position + 1)?;
    if raw.starts_with(['"', '\'']) != raw.ends_with(['"', '\'']) {
        return None; // Rule 3: a fragment of a quoted value, not a socket.
    }
    let socket = raw.trim_matches(|c| c == '"' || c == '\'');
    let is_identifier = socket
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if socket.starts_with("loom-") && is_identifier {
        Some(socket.to_string())
    } else {
        None
    }
}

/// Parse `list-panes -F PANE_FORMAT` stdout into panes, in tmux's own pane
/// order — the order [`reconcile_steps`]'s add/respawn/kill rules rely on
/// for "the first kill in pane order".
///
/// Each line is `id\tdead\tstart_command` (see [`PANE_FORMAT`]). A line that
/// does not yield at least the first two fields is skipped rather than
/// failing the whole reconcile over one malformed row.
fn parse_pane_list(stdout: &str) -> Vec<PaneInfo> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let id = fields.next()?;
            let dead = fields.next()?;
            let start_command = fields.next().unwrap_or("");
            Some(PaneInfo {
                id: id.to_string(),
                dead: dead == "1",
                session_socket: parse_pane_socket(start_command),
            })
        })
        .collect()
}

/// Keeper selection (duplicate panes): a `loom attach` rebuild racing a
/// reconcile tick can leave two panes carrying the same socket. Group
/// attributed panes (`session_socket == Some(_)`) by socket in pane order;
/// the KEEPER of a group is its first non-dead pane, or its first pane if
/// every pane in the group is dead. Every OTHER pane in the group is a
/// duplicate and [`reconcile_steps`] kills it regardless of its own
/// dead/alive state.
fn select_keepers(panes: &[PaneInfo]) -> HashSet<usize> {
    let mut keeper_by_socket: HashMap<&str, usize> = HashMap::new();
    for (index, pane) in panes.iter().enumerate() {
        let Some(socket) = pane.session_socket.as_deref() else {
            continue;
        };
        match keeper_by_socket.get(socket) {
            None => {
                keeper_by_socket.insert(socket, index);
            }
            Some(&current_keeper) => {
                if panes[current_keeper].dead && !pane.dead {
                    keeper_by_socket.insert(socket, index);
                }
            }
        }
    }
    keeper_by_socket.values().copied().collect()
}

/// Rule 1 — Add: for each `(socket, key)` in `desired`, in order, with NO
/// attributed pane (dead or alive) carrying that socket, emit a
/// `split-window` immediately followed by a `select-layout … tiled`.
/// Re-tiling after EVERY split, never once at the end, is load-bearing, not
/// a preference: `split-window -t <session>` targets the session's CURRENT
/// pane — the one the previous split just made — so pane heights halve
/// 50 → 25 → 12 → 5 → 2 and a sixth split fails outright with "no space for
/// a new pane" (verified on tmux 3.7b; see
/// `doc/loom/knowledge/mistakes/tmux-backend.md`, "tmux Layout and Option
/// Traps"). Called from [`reconcile_steps`], which uses the returned step
/// count to compute `add_count` for Rule 3's floor.
fn add_steps(panes: &[PaneInfo], desired: &[(String, String)]) -> Vec<Vec<String>> {
    let attributed: HashSet<&str> = panes
        .iter()
        .filter_map(|pane| pane.session_socket.as_deref())
        .collect();

    let mut steps: Vec<Vec<String>> = Vec::new();
    for (socket, key) in desired {
        if attributed.contains(socket.as_str()) {
            continue;
        }
        steps.push(
            ["split-window", "-t", super::viewer::OVERVIEW_SESSION]
                .into_iter()
                .map(str::to_string)
                .chain(["sh".to_string(), "-c".to_string()])
                .chain(std::iter::once(super::viewer::pane_command(socket, key)))
                .collect(),
        );
        steps.push(
            [
                "select-layout",
                "-t",
                super::viewer::OVERVIEW_SESSION,
                "tiled",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        );
    }
    steps
}

/// Pure diff: panes as tmux reports them vs. the sessions that SHOULD have a
/// pane, `desired` being `(session_socket, tmux_session)` pairs in discovery
/// order (the shape [`super::viewer::attachable_sessions`] returns). Returns
/// the tmux argvs to converge, in order, AFTER the `tmux` binary and WITHOUT
/// the leading `-L <viewer socket>` — [`reconcile_viewer`] prepends that. No
/// tmux, no filesystem, no session model: this is the testable heart of the
/// module.
///
/// Duplicate panes are resolved by [`select_keepers`] (see its doc for the
/// keeper rule); new panes are added by [`add_steps`] (see its doc for
/// Rule 1).
///
/// # Rule 2 — Respawn
///
/// A keeper that is dead whose socket IS still in `desired` gets
/// `respawn-pane`, which re-runs the pane's recorded start command — this
/// covers an attach client that died while its inner server lived on.
/// `respawn-pane` (without `-k`) fails on a pane that is not dead, so this
/// never fires on a live keeper.
///
/// # Rule 3 — Remove, and its floor
///
/// A keeper that is dead whose socket is NOT in `desired` gets `kill-pane`.
/// But killing the LAST pane kills the window, which kills the session,
/// which — despite `exit-empty off` — destroys the operator's whole view.
/// After building the full kill list, compute `survivors = panes.len() -
/// kills.len() + adds`; if that is `0`, drop the FIRST kill in pane order,
/// leaving that dead pane as the "nothing running" placeholder. Consequence
/// worth pinning: a single dead pane with nothing to add emits ZERO kills.
///
/// # Rule 5 — Untouchable
///
/// Panes with `session_socket == None` are the operator's own splits: never
/// killed, never respawned. They DO count toward the floor above, so an
/// operator pane sitting alongside a doomed one means the floor never binds.
///
/// # Rule 6 — No-op
///
/// An empty diff returns an empty vec, so [`reconcile_viewer`] runs no tmux
/// command beyond its probe and `list-panes`.
///
/// # Global order
///
/// ALL adds, then ALL respawns, then ALL kills — adds first so a concurrent
/// add guarantees a survivor before any kill runs.
fn reconcile_steps(panes: &[PaneInfo], desired: &[(String, String)]) -> Vec<Vec<String>> {
    let live: HashSet<&str> = desired.iter().map(|(socket, _)| socket.as_str()).collect();
    let mut steps = add_steps(panes, desired);
    let add_count = steps.len() / 2;
    let keepers = select_keepers(panes);

    // Rules 2, 3 (candidates), 4, 5 — one pass, tmux order, so "the first
    // kill in pane order" (the floor, below) is well defined.
    let mut respawns: Vec<Vec<String>> = Vec::new();
    let mut kill_candidates: Vec<usize> = Vec::new();
    for (index, pane) in panes.iter().enumerate() {
        let Some(socket) = pane.session_socket.as_deref() else {
            continue; // Rule 5: never touched.
        };
        if !keepers.contains(&index) {
            kill_candidates.push(index); // Rule 4: non-keeper duplicate.
            continue;
        }
        if !pane.dead {
            continue; // Live keeper: left alone.
        }
        if live.contains(socket) {
            respawns.push(vec![
                "respawn-pane".to_string(),
                "-t".to_string(),
                pane.id.clone(),
            ]);
        } else {
            kill_candidates.push(index); // Rule 3 candidate.
        }
    }

    // Rule 3's floor: never let a reconcile tick empty the window.
    let survivors = panes.len() as isize - kill_candidates.len() as isize + add_count as isize;
    if survivors == 0 && !kill_candidates.is_empty() {
        kill_candidates.remove(0);
    }

    steps.extend(respawns);
    steps.extend(kill_candidates.into_iter().map(|index| {
        vec![
            "kill-pane".to_string(),
            "-t".to_string(),
            panes[index].id.clone(),
        ]
    }));
    steps
}

/// `-F` format for `list-panes`. `\t` here is a REAL tab byte — this is a
/// Rust string literal, not a shell string — and the argv built from it below
/// goes straight to `Command`, with no shell in between to expand an escape.
const PANE_FORMAT: &str = "#{pane_id}\t#{pane_dead}\t#{pane_start_command}";

/// Steps 2 and 3 of [`reconcile_viewer`]: probe attachability, then list and
/// parse panes. `Ok(None)` means the viewer server is gone or `loom attach`
/// is mid-rebuild — nothing to converge this tick, not a reconcile failure.
fn list_viewer_panes(viewer: &str) -> Result<Option<Vec<PaneInfo>>> {
    // 2. `has-session` here asks ATTACHABILITY, not liveness — the one
    // sanctioned use documented in
    // `doc/loom/knowledge/architecture/terminal-backends.md` ("loom attach —
    // Overview and Direct") and mirrored from the viewer's own endpoint-ready
    // probe. A non-zero exit, or the command erroring outright, means the
    // server is gone or `loom attach` is mid-rebuild — either way this tick
    // has nothing to converge, so skip rather than fail the scheduler loop.
    let has_session = run_tmux_control(
        &[
            "-L",
            viewer,
            "has-session",
            "-t",
            super::viewer::OVERVIEW_SESSION,
        ],
        TMUX_PROBE_TIMEOUT,
        format!("tmux has-session ({viewer})"),
    );
    let attachable = matches!(&has_session, Ok(output) if output.status.success());
    if !attachable {
        return Ok(None);
    }

    // 3. `list-panes -t <session>` resolves the session name to its CURRENT
    // window — exactly the window the apply step below modifies via the same
    // session-name targeting, so the set we list here is the set we act on.
    let list = run_tmux_control(
        &[
            "-L",
            viewer,
            "list-panes",
            "-t",
            super::viewer::OVERVIEW_SESSION,
            "-F",
            PANE_FORMAT,
        ],
        TMUX_PROBE_TIMEOUT,
        format!("tmux list-panes ({viewer})"),
    )
    .with_context(|| format!("Failed to list panes on viewer '{viewer}'"))?;
    if !list.status.success() {
        let stderr = String::from_utf8_lossy(&list.stderr);
        bail!("tmux list-panes failed on viewer '{viewer}': {stderr}");
    }
    let stdout = String::from_utf8_lossy(&list.stdout);
    Ok(Some(parse_pane_list(&stdout)))
}

/// Step 4 of [`reconcile_viewer`]: apply the computed steps in order. STOP
/// on the first failed step and return an `Err` naming it — the next tick
/// retries from a freshly re-listed state rather than compounding a partial
/// apply.
fn apply_steps(viewer: &str, steps: Vec<Vec<String>>) -> Result<()> {
    for step in steps {
        let mut args: Vec<&str> = vec!["-L", viewer];
        args.extend(step.iter().map(String::as_str));
        let label = format!("tmux {} ({viewer})", step.join(" "));
        let output = run_tmux_control(&args, TMUX_PROBE_TIMEOUT, label.clone())
            .with_context(|| format!("Failed to run reconcile step '{label}'"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("reconcile step '{label}' failed on viewer '{viewer}': {stderr}");
        }
    }
    Ok(())
}

/// Probe, list, diff, apply. Never panics; every failure is an `Err`, except
/// the "nothing to converge this tick" case below, which returns `Ok(())`
/// by design — it describes a viewer that does not exist yet, is gone, or is
/// mid-rebuild, not a reconcile failure. Steps 2 and 3 (probe + list) are
/// [`list_viewer_panes`]; step 4 (apply) is [`apply_steps`].
pub(crate) fn reconcile_viewer(work_dir: &Path, desired: &[(String, String)]) -> Result<()> {
    let viewer = super::viewer::viewer_socket_name(work_dir);

    // 1. Cheap gate, no subprocess. The operator never attached, or the
    // daemon is polling before any `loom attach` has run — this module never
    // creates the viewer, it only maintains one the operator already built.
    if !socket_path_for(&viewer).exists() {
        return Ok(());
    }

    let Some(panes) = list_viewer_panes(&viewer)? else {
        return Ok(());
    };

    apply_steps(&viewer, reconcile_steps(&panes, desired))
}

/// Daemon entry point, called once per scheduler tick.
pub(crate) fn refresh_attached_viewer(work_dir: &Path) -> Result<()> {
    // Gate BEFORE discovery: with no viewer socket this costs one `stat` and
    // reads no session files at all, so the common case (nobody attached) is
    // free on every scheduler tick. `reconcile_viewer` re-checks the same
    // gate (its step 1) so it stays safe for any OTHER caller too — that
    // duplication is deliberate, not an oversight.
    if !socket_path_for(&super::viewer::viewer_socket_name(work_dir)).exists() {
        return Ok(());
    }
    let desired = super::viewer::attachable_sessions(work_dir)?;
    reconcile_viewer(work_dir, &desired)
}

#[cfg(test)]
#[path = "tests_reconcile.rs"]
mod tests;
