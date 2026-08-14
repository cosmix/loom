//! The PURE decision half of viewer reconciliation — no tmux, no filesystem,
//! no session model, which is what makes every reconcile rule unit-testable
//! without a tmux server. This mirrors the split `tmux/mod.rs` already draws
//! between `evaluate_new_session` (pure decision) and `spawn_in_tmux` (the
//! tmux-calling executor), for exactly the same testability reason. The
//! executor half — probing, listing panes, and applying the steps computed
//! here — lives in the parent module, [`super`].

use std::collections::{HashMap, HashSet};

/// One pane of the viewer window, as tmux reports it.
pub(super) struct PaneInfo {
    pub(super) id: String,
    pub(super) dead: bool,
    /// The inner session socket this pane's attach client targets, parsed by
    /// [`parse_pane_socket`]. `None` means this pane is not attributable to
    /// loom at all — the operator's own split (see [`reconcile_steps`]'s
    /// Rule 5).
    pub(super) session_socket: Option<String>,
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
/// template, because [`super::super::viewer::pane_command`]'s exact shape is
/// that module's implementation detail, not a contract this one should
/// re-derive:
///
/// 1. The string must contain `attach-session`, or this is not a loom pane.
/// 2. Split on whitespace and find the first `-L` token; take the token
///    right after it. No `-L`, or nothing after it, means this is not a
///    loom pane.
/// 3. Strip any surrounding `"` or `'` from that token.
/// 4. The result must start with `loom-` (session sockets are
///    `loom-<session.id>`, see `super::socket_name`).
///
/// Rule 4 is a deliberate SAFETY TIGHTENING, not decoration. Without it, an
/// operator who split their own pane and ran a plain `tmux attach-session`
/// there would be attributed as a loom pane by rules 1-3 and then killed by
/// [`reconcile_steps`]'s duplicate/removal logic. Loom session ids
/// (`session-<uuid8>-<unixts>`, `[A-Za-z0-9-]` only) need no unescaping, so
/// the stripped token is unambiguous once rule 4 passes.
fn parse_pane_socket(start_command: &str) -> Option<String> {
    if !start_command.contains("attach-session") {
        return None;
    }
    let tokens: Vec<&str> = start_command.split_whitespace().collect();
    let position = tokens.iter().position(|&token| token == "-L")?;
    let raw = tokens.get(position + 1)?;
    let socket = raw.trim_matches(|c| c == '"' || c == '\'');
    if socket.starts_with("loom-") {
        Some(socket.to_string())
    } else {
        None
    }
}

/// Parse `list-panes -F PANE_FORMAT` stdout into panes, in tmux's own pane
/// order — the order [`reconcile_steps`]'s add/respawn/kill rules rely on
/// for "the first kill in pane order".
///
/// Each line is `id\tdead\tstart_command` (see [`super::PANE_FORMAT`]). A
/// line that does not yield at least the first two fields is skipped rather
/// than failing the whole reconcile over one malformed row.
pub(super) fn parse_pane_list(stdout: &str) -> Vec<PaneInfo> {
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
            ["split-window", "-t", super::super::viewer::OVERVIEW_SESSION]
                .into_iter()
                .map(str::to_string)
                .chain(["sh".to_string(), "-c".to_string()])
                .chain(std::iter::once(super::super::viewer::pane_command(
                    socket, key,
                )))
                .collect(),
        );
        steps.push(
            [
                "select-layout",
                "-t",
                super::super::viewer::OVERVIEW_SESSION,
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
/// order (the shape [`super::super::viewer::attachable_sessions`] returns).
/// Returns the tmux argvs to converge, in order, AFTER the `tmux` binary and
/// WITHOUT the leading `-L <viewer socket>` — [`super::reconcile_viewer`]
/// prepends that. No tmux, no filesystem, no session model: this is the
/// testable heart of the module.
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
/// An empty diff returns an empty vec, so [`super::reconcile_viewer`] runs
/// no tmux command beyond its probe and `list-panes`.
///
/// # Global order
///
/// ALL adds, then ALL respawns, then ALL kills — adds first so a concurrent
/// add guarantees a survivor before any kill runs.
pub(super) fn reconcile_steps(panes: &[PaneInfo], desired: &[(String, String)]) -> Vec<Vec<String>> {
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

#[cfg(test)]
#[path = "parse_tests.rs"]
mod parse_tests;
#[cfg(test)]
#[path = "steps_tests.rs"]
mod steps_tests;
