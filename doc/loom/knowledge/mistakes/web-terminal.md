# Web Terminal (loom status --web terminal viewer)

> Bridge
> Mistakes in the PTY/tmux terminal bridge under `loom/src/commands/status/web/terminal/` and
> the xterm viewer under `web/src/components/terminal/`. See
> [architecture/web-terminal.md](../architecture/web-terminal.md) for the current design.

## PTY Master Fd Leaked Into the Child, Defeating Hangup Detection (2026-09-09)

**What happened:** confirmed by measurement, not inspection — `cargo test --lib
...tests_pty::pty_child_shutdown_reaps_the_child -- --exact` finished in exactly `2.00s` on
two consecutive runs, matching `PtyChild::shutdown`'s fallback deadline to the millisecond,
never the fast path. `nix::pty::openpty` wraps libc `openpty(3)`, which has no flags argument
and opens the master **without** `O_CLOEXEC`. The `fcntl` at `pty.rs::spawn` set `O_NONBLOCK`
(a file-status flag) but never touched the descriptor flags, so the master survived into the
tmux child across `Command::spawn`'s fork+exec. With the child holding its own copy,
`shutdown`'s `drop(master)` (`pty.rs::write_some` at the time) was never the last reference, the slave
never saw a hangup, the child never got `SIGHUP`, and every terminal close burned the full 2s
deadline before `SIGKILL`. Ironically the surrounding code already carried a comment about
dropping the three slave clones so the master would see the child's hangup — the master was
the fd nobody CLOEXEC'd, defeating exactly that mechanism.

**Why:** `openpty` gives `O_CLOEXEC` on neither end, and a timing assertion whose own budget
(`elapsed < 3s`) exceeds the fallback deadline cannot distinguish the fast path from the slow
one — it passed either way.

**Prevention:** after any `openpty`/`posix_openpt`-style call, `fcntl(F_SETFD(FD_CLOEXEC))` the
master explicitly — never assume it. When timing a "did the fast path fire" behaviour, assert
strictly _below_ the fallback deadline, not merely under some larger ceiling.

## A Backpressure Gate Made a Half-Closed Browser Invisible, and Two Fixes for It Failed First (2026-09-09)

**What happened:** the terminal bridge's inbound gate narrows the polled TCP event mask to
`empty()` once the pending-input queue is full, so it stops asking whether the socket is
readable. The brief that reached the implementer claimed "POLLHUP/POLLERR/POLLNVAL are
reported whether or not you ask for them, so masking POLLIN off costs nothing in disconnect
detection" — true for those three flags, wrong as a conclusion. A TCP peer's `close()` sends a
FIN, which sets `POLLIN` (read returns EOF) and, on Linux, `POLLRDHUP` — **not** `POLLHUP`,
which `tcp_poll` only sets after an RST or `sk_shutdown == SHUTDOWN_MASK`. So a gated,
half-closed browser tab (control terminal, a paste over 64 KiB, a slow-draining tmux child,
or simply closing the tab while the PTY is quiet) was invisible for the whole gated window,
holding one of 8 terminal slots, a PTY and a tmux child indefinitely. The claim had been
copied verbatim from a reviewer into an implementer's brief and propagated unchecked through
two agents.

The first fix attempted — `recv(MSG_PEEK|MSG_DONTWAIT)` as a portable EOF probe, overriding a
reviewer's POLLRDHUP suggestion on cross-platform grounds — also failed, for a reason obvious
only in hindsight: `recv` returns 0 only once the receive queue is **empty** and the FIN has
been consumed in sequence, but the gate exists precisely because the queue was left unread.
While gated, the queue is almost always non-empty, so the peek returns `>0` and the probe is
silent in exactly the state it was built to detect.

**Why:** "reported whether or not you asked for it" is a true statement about `POLLHUP`
specifically, never a general statement about disconnect detection — the ordinary TCP close
path is `POLLIN`/`POLLRDHUP`. And any EOF-by-read probe is defeated by design whenever the
surrounding code deliberately leaves the receive queue unread.

**Fix (the one that shipped):** a portable stall deadline instead of any readiness- or
read-based detector — `gate_stalled` tracks how long `pending` has sat at its cap and gives up
after `GATE_STALL_TIMEOUT` (30s), closing with code 4008. It needs no `cfg`, works identically
on every platform, and also catches the case no readiness flag or FIN can: a peer that is
alive but has wedged the PTY, so nothing ever signals its side.

**Prevention:** before masking a readiness flag off any socket, name the exact syscall-level
event that signals peer close on each target platform — do not generalise from "this flag
fires unconditionally." Before proposing any mechanism that _reads_ a socket to detect EOF,
ask what is already sitting in the queue in the state being detected; a backpressure gate
guarantees that queue is non-empty, which defeats every read-based EOF probe. A claim
inherited from a reviewer and pasted into a brief carries no more authority than one invented
from scratch — verify it before it reaches an implementer.

## Front-Truncating a Bounded Input Queue Executes an Attacker-Controlled Suffix (2026-09-09)

**What happened:** an early version of the terminal bridge's pending-input queue dropped from
the **front** on overflow — the dangerous direction for a terminal. A typed command is
submitted by its _last_ byte (the newline), so front-truncation keeps that trailing newline
and discards the beginning: what reaches the agent is a suffix of what was typed, already
terminated, so the shell executes it. `echo "summarise <70KB of pasted log>"` front-truncated
becomes `...tail of the log"` + newline — a complete, executable line. Root cause was the
absence of backpressure: the socket was drained on any `POLLIN` with no reference to the
queue's length, making it a lossy ring rather than a staging buffer for one partial write.

**Why:** the queue was sized to bound memory, but nothing stopped the socket read that fed it
once full, so "bounded" meant "silently lossy" rather than "backpressured."

**Fix:** gate the inbound read on `pending.len() < MAX_PENDING_INPUT` and let TCP's own
receive window stall the browser instead — nothing is lost, ordering is preserved, and
`pending` now peaks near the WebSocket read-buffer size, so no drop policy is ever reached.
Coupling to watch: this is only safe while the outbound direction (`pump_pty`) drains the PTY
unconditionally — "stop reading input because the queue is full" plus "the queue is full
because the child is blocked writing" is the classic proxy deadlock.

**Prevention:** when a bounded queue sits between a network peer and a PTY, identify which
_end_ of the stream carries the commit token (for line-oriented input, the trailing newline)
before choosing a drop policy — any policy that discards context while preserving the commit
token turns a truncated paste into an executed command. Prefer backpressure over any drop
policy when the upstream transport already has flow control (TCP) to push back with.

## Terminal Viewer Scroll Uses Bounded JSON Frames, Not Raw Keystrokes or Read-Only (2026-09-17)

Web terminal viewing scroll uses dedicated bounded JSON `{scroll:{pages}}` frames
(`loom/src/commands/status/web/terminal/protocol.rs`) to synthesize PageUp/PageDown in the
bridge; raw binary stdin stays blocked for the View mode. `tmux attach` no longer runs
read-only, because read-only also blocks paging -- do not restore read-only without another
paging path. Mouse wheel on the alternate screen otherwise synthesizes Up/Down keystrokes,
which recalls prior agent prompts instead of scrolling; do not forward raw mouse or arrow
events for output history (`web/src/components/terminal/viewer-wheel.ts`). Double-click on the
terminal well takes control.

## jsdom Has No Canvas Backend; xterm Probes It on Open (2026-09-17)

`web/src/components/terminal/viewer-wheel.test.ts` prints `Not implemented:
HTMLCanvasElement's getContext() method: without installing the canvas npm package` to
stderr under vitest, because `new Terminal(...)` from `@xterm/xterm` probes canvas when it
opens. Fixed by stubbing `HTMLCanvasElement.prototype.getContext` to return `null` in
`web/src/test/setup.ts` beside the other jsdom layout stubs. Any future test that mounts an
xterm `Terminal` needs this stub already in place; it predates any specific plan and belongs
in the shared setup file, not per-test.

## A Signal Ended Terminal Bridges as Dead Peers (2026-10-14)

**What happened:** `bridge_survives_a_client_that_stops_reading` and `bridge_leaves_pty_output_upstream_while_the_client_is_stalled` failed about one full `cargo test --lib` run in three, and never alone or under CPU load (`scripts/flake-check.sh`). The client received a bare close partway through the output. Temporary logging at each `Stop::Peer` site showed `outbound: Io(Interrupted)`, an `EINTR` from the bridge's `send`.

**Why:** the bridge sets `SO_SNDTIMEO`/`SO_RCVTIMEO` (`WRITE_TIMEOUT`, `READ_TIMEOUT`). signal(7): socket calls on such a socket are never restarted after a signal handler runs, `SA_RESTART` or not. Once any handler is installed in the process (the server's own `ctrlc` one in `status/web/mod.rs`), every delivered signal is `EINTR` on whichever bridge is mid-call. `bridge.rs::stalled` counted only `WouldBlock` and `TimedOut` as not-yet-transferred, so `outbound_stop` and `pump_socket` ended the session as a dead peer.

**Prevention:** wherever an error kind is classified as retryable on a socket with a timeout, `Interrupted` belongs with `WouldBlock`/`TimedOut`. A test that fails only inside the full suite, and never under CPU load alone, points to process-global state (signals, descriptors) rather than timing. Log the stop reason before tuning budgets.

**Fix:** `stalled` includes `Interrupted`, pinned by `bridge.rs` `tests::a_signal_interrupting_a_transfer_is_retried_not_read_as_a_dead_peer`. `tests_bridge.rs::read_until` prints the close frame and byte count when a bridge closes before the needle arrives.

## Viewer Paging Covered the Wheel but Not Touch

**What happened:** On an iPad (Safari, touch) a one-finger swipe over a live tmux stage terminal in View mode did nothing, while the mouse wheel paged correctly.
**Why:** The bounded page path was wired only to `wheel` events. Touch never produces wheel events; xterm 6 handles touch with its own gesture listener, which scrolls only the scrollback viewport. Under tmux that viewport is empty (alternate screen), so the swipe had nothing to move.
**Prevention:** Any View-mode scroll path for the alternate screen must cover wheel and touch input; test touch with dispatched touch events, not only wheel events.
**Fix:** `web/src/components/terminal/viewer-touch.ts` feeds one-finger vertical swipes into the same pager as the wheel (`createViewerPager`), with `touch-action: pinch-zoom` on the View-mode host; covered by `viewer-touch.test.ts`.
