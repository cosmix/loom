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
//!
//! This mirrors the split `tmux/mod.rs` already draws between
//! `evaluate_new_session` (pure decision) and `spawn_in_tmux` (the
//! tmux-calling executor), for exactly the same testability reason. The pure
//! decision half — parsing pane info and computing the add/respawn/kill diff,
//! no tmux, no filesystem, no session model — lives in [`steps`]; this file
//! is the executor half: probing, listing panes, and applying the steps
//! [`steps::reconcile_steps`] computes.

mod steps;

use anyhow::{bail, Context, Result};
use std::path::Path;

use super::{run_tmux_control, socket_path_for, TMUX_PROBE_TIMEOUT};
use steps::{parse_pane_list, reconcile_steps, PaneInfo};

/// `-F` format for `list-panes`. `\t` here is a REAL tab byte — this is a
/// Rust string literal, not a shell string — and the argv built from it below
/// goes straight to `Command`, with no shell in between to expand an escape.
const PANE_FORMAT: &str = "#{pane_id}\t#{pane_dead}\t#{pane_start_command}";

/// Step 2 of [`reconcile_viewer`]/[`list_viewer_panes`]: probe ATTACHABILITY
/// (not liveness — the one sanctioned use documented in
/// `doc/loom/knowledge/architecture/terminal-backends.md`, "loom attach —
/// Overview and Direct", mirrored from the viewer's own endpoint-ready
/// probe). A non-zero exit, or the command erroring outright, means the
/// server is gone or `loom attach` is mid-rebuild — either way this tick has
/// nothing to converge, so the caller skips rather than fails the scheduler
/// loop.
///
/// Logs on `false`, not silent: unlike the socket-ABSENT case in
/// [`reconcile_viewer`] (the common nobody-attached case, kept to a single
/// `stat`), a socket that exists but will not accept clients means the
/// viewer server is gone or `loom attach` is mid-rebuild — worth a trace
/// line so a reconcile that never converges is diagnosable. The two `false`
/// causes are logged with DIFFERENT payloads on purpose — a probe that
/// failed to run at all (timeout, spawn error) is a different failure mode
/// from one that ran and got a non-zero exit (server refused) — so the log
/// line, not just the `false` return, tells them apart.
fn viewer_accepting_clients(viewer: &str) -> bool {
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
    match has_session {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            tracing::debug!(
                viewer = %viewer,
                status = ?output.status.code(),
                stderr = %String::from_utf8_lossy(&output.stderr),
                "Overview viewer socket present but server refused has-session; skipping reconcile"
            );
            false
        }
        Err(error) => {
            tracing::debug!(
                viewer = %viewer,
                error = %error,
                "Overview viewer has-session probe failed to run; skipping reconcile"
            );
            false
        }
    }
}

/// Steps 2 and 3 of [`reconcile_viewer`]: probe attachability, then list and
/// parse panes. `Ok(None)` means the viewer server is gone or `loom attach`
/// is mid-rebuild — nothing to converge this tick, not a reconcile failure.
fn list_viewer_panes(viewer: &str) -> Result<Option<Vec<PaneInfo>>> {
    // 2. See `viewer_accepting_clients`'s doc.
    if !viewer_accepting_clients(viewer) {
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

/// Whether `step` is COSMETIC — `select-layout`, emitted after every add
/// (Rule 1) and once after any kill (Rule 7) purely to re-tile the window.
/// Pure so [`apply_steps`]'s dispatch is unit-testable without a tmux
/// server.
fn step_is_cosmetic(step: &[String]) -> bool {
    step.first().map(String::as_str) == Some("select-layout")
}

/// Step 4 of [`reconcile_viewer`]: apply the computed steps in order. STOP
/// on the first failed FUNCTIONAL step (anything but `select-layout`) and
/// return an `Err` naming its index and the total step count — the next
/// tick retries from a freshly re-listed state rather than compounding a
/// partial apply, and the index/total lets the log show exactly how much of
/// the pass was skipped (e.g. one failing `split-window` used to silently
/// block every later `kill-pane`, since adds are ordered before kills — see
/// [`steps::reconcile_steps`]'s "Global order").
///
/// A COSMETIC step ([`step_is_cosmetic`]) is different: it changes no
/// session state, only geometry, so its failure (e.g. a window too small to
/// tile) is logged at `debug` and the pass CONTINUES rather than aborting —
/// a re-tile that cannot happen must never block the functional
/// add/respawn/kill it follows.
fn apply_steps(viewer: &str, steps: Vec<Vec<String>>) -> Result<()> {
    let total = steps.len();
    for (index, step) in steps.into_iter().enumerate() {
        let mut args: Vec<&str> = vec!["-L", viewer];
        args.extend(step.iter().map(String::as_str));
        let label = format!("tmux {} ({viewer})", step.join(" "));
        let position = index + 1;
        let output =
            run_tmux_control(&args, TMUX_PROBE_TIMEOUT, label.clone()).with_context(|| {
                format!("Failed to run reconcile step {position}/{total} '{label}'")
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if step_is_cosmetic(&step) {
                tracing::debug!(
                    viewer = %viewer,
                    step = %label,
                    stderr = %stderr,
                    "Cosmetic reconcile step failed; continuing with the rest of the pass"
                );
                continue;
            }
            bail!(
                "reconcile step {position}/{total} '{label}' failed on viewer '{viewer}': {stderr}"
            );
        }
    }
    Ok(())
}

/// Probe, list, diff, apply. Never panics; every failure is an `Err`, except
/// the "nothing to converge this tick" case below, which returns `Ok(())`
/// by design — it describes a viewer that does not exist yet, is gone, or is
/// mid-rebuild, not a reconcile failure. Steps 2 and 3 (probe + list) are
/// `list_viewer_panes`; step 4 (apply) is `apply_steps`.
///
/// `pub`, not `pub(crate)`: exercised directly by the real-tmux e2e test
/// (`tests/e2e/tmux_reconcile.rs`), which needs a reconciler reachable from
/// outside the crate.
pub fn reconcile_viewer(work_dir: &Path, desired: &[(String, String)]) -> Result<()> {
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
mod tests {
    use super::step_is_cosmetic;

    #[test]
    fn step_is_cosmetic_accepts_select_layout_and_rejects_everything_else() {
        let select_layout = vec!["select-layout".to_string(), "-t".to_string()];
        assert!(
            step_is_cosmetic(&select_layout),
            "select-layout is cosmetic"
        );

        for functional in [
            vec!["split-window".to_string()],
            vec!["respawn-pane".to_string()],
            vec!["kill-pane".to_string()],
        ] {
            assert!(
                !step_is_cosmetic(&functional),
                "{functional:?} must never be treated as cosmetic"
            );
        }

        assert!(
            !step_is_cosmetic(&[]),
            "an empty step has no verb and must not be treated as cosmetic"
        );
    }
}
