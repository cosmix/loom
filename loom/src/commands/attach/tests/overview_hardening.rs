//! Viewer-hardening tests, split from `tests/overview.rs` to keep both modules
//! under the 400-line ceiling (CLAUDE.md Rule 17).
//!
//! Every test here is about ONE argv step — the `start-server` sequence that
//! configures the viewer server before pane 0 exists. Its position among the
//! steps and the order WITHIN it are both load-bearing, so they are pinned
//! separately from the pane-construction tests next door.

use super::tests::{panes, step_index};
use super::*;

/// The one `;`-separated tmux sequence that hardens the viewer server.
fn hardening_step(pane_count: usize) -> Vec<String> {
    let steps = build_overview_argv("loom-view-deadbeef", &panes(pane_count));
    steps[step_index(&steps, "start-server")].clone()
}

/// Index of `needle` within the hardening sequence, or a failure naming it.
fn position(harden: &[String], needle: &str) -> usize {
    harden
        .iter()
        .position(|word| word == needle)
        .unwrap_or_else(|| panic!("hardening must set {needle}: {harden:?}"))
}

#[test]
fn build_overview_argv_hardens_the_viewer_before_pane_0_exists() {
    // THE regression this pins: pane 0 is born already running an attach
    // client into another server, and until the viewer is hardened, that
    // client exiting on contact takes the window, the session, and the whole
    // viewer server with it — surfacing as `server exited unexpectedly` from
    // `new-session` and failing the command outright. REMAIN_ON_EXIT_FLAGS
    // provably cannot cover pane 0: it targets a window that does not exist
    // until `new-session` returns. So the ORDER here is the fix, and
    // asserting it is what keeps the hardening from drifting back below
    // `new-session`, where it would protect nothing.
    let steps = build_overview_argv("loom-view-deadbeef", &panes(3));

    let kill_idx = step_index(&steps, "kill-session");
    let harden_idx = step_index(&steps, "start-server");
    let new_session_idx = step_index(&steps, "new-session");

    assert!(
        kill_idx < harden_idx,
        "hardening must follow the previous viewer's teardown: killing a \
         default-configured server's last session takes the server with it"
    );
    assert!(
        harden_idx < new_session_idx,
        "the viewer must be hardened BEFORE new-session creates pane 0, or it \
         protects nothing that pane 0 can die from"
    );
}

#[test]
fn viewer_hardening_is_a_single_tmux_command_sequence() {
    // `start-server` brings up a server with no sessions, which the default
    // `exit-empty on` reaps before a second `tmux` process could connect to
    // configure it. Split into separate steps, the hardening silently
    // degrades to doing nothing at all — every command still exits 0, and the
    // attach still dies on pane 0. Only the `;` separators keep it real.
    let harden = hardening_step(1);

    assert!(
        harden.iter().any(|word| word == ";"),
        "hardening must be one `;`-separated tmux command sequence, not \
         separate invocations: {harden:?}"
    );

    // Set GLOBALLY. Targeted at the session (as the post-new-session step
    // correctly is) it would name a session that does not exist yet, and the
    // whole sequence would error.
    assert!(
        harden.iter().any(|word| word == "-g"),
        "hardening must set global options: pane 0's window does not exist yet"
    );
    assert!(
        !harden.iter().any(|word| word == OVERVIEW_SESSION),
        "hardening must not target the viewer session before it exists: {harden:?}"
    );
}

#[test]
fn viewer_hardening_keeps_the_server_alive_before_the_cosmetic_format() {
    // tmux abandons the rest of a sequence when one command errors, so an
    // entry can only be aborted by entries placed BEFORE it. `exit-empty off`
    // must therefore land first (nothing can be set on a server that has
    // already reaped itself), and the cosmetic `remain-on-exit-format` last —
    // it is the one entry whose availability varies across tmux builds, and
    // ahead of the others it would take both of them down on any tmux that
    // rejects it.
    let harden = hardening_step(1);
    let exit_empty = position(&harden, "exit-empty");
    let remain_on_exit = position(&harden, "remain-on-exit");
    let format = position(&harden, "remain-on-exit-format");

    assert_eq!(
        harden.get(exit_empty + 1).map(String::as_str),
        Some("off"),
        "exit-empty must be turned OFF, or the empty server reaps itself"
    );
    assert!(
        exit_empty < remain_on_exit,
        "exit-empty must land first — nothing else can be set on a server that \
         has already reaped itself"
    );
    assert!(
        remain_on_exit < format,
        "the cosmetic format string must come last so a tmux that rejects it \
         cannot abort the two settings that actually keep the viewer alive"
    );
}

#[test]
fn viewer_hardening_orders_mouse_settings_least_likely_to_be_rejected_first() {
    let harden = hardening_step(1);

    // `mouse off` overrides the operator's `~/.tmux.conf`, which tmux reads at
    // `start-server`: with capture on, a drag in a pane is eaten by tmux
    // instead of reaching the terminal, so agent output cannot be selected.
    // Asserted by value, not presence — `mouse on` here would be worse than
    // omitting it, since it would force capture on for operators who had it off.
    let mouse = position(&harden, "mouse");
    assert_eq!(
        harden.get(mouse + 1).map(String::as_str),
        Some("off"),
        "the viewer must turn mouse capture OFF, never on"
    );

    // `kmous@` closes the hole `mouse off` leaves: claude's own mouse-mode
    // request would otherwise be mirrored out to the operator's terminal, and
    // every drag forwarded back into the agent — where claude's clipboard
    // copy (`tmux load-buffer -w -`) crashes the tmux 3.6a stage server.
    // Asserted by value: the entry must DELETE the capability for every TERM,
    // in an indexed slot so re-running the hardening on the same long-lived
    // viewer server stays idempotent.
    let kmous = position(&harden, "terminal-overrides[99]");
    assert_eq!(
        harden.get(kmous + 1).map(String::as_str),
        Some("*:kmous@"),
        "the viewer must delete the kmous capability for every client TERM"
    );

    assert!(
        mouse < kmous,
        "the kmous override's indexed-array syntax is the likelier rejection \
         on an old tmux; it must not be able to abort `mouse off`"
    );
    assert!(
        kmous < position(&harden, "remain-on-exit-format"),
        "the cosmetic format string must stay last so a tmux that rejects it \
         cannot abort the kmous override"
    );
}
