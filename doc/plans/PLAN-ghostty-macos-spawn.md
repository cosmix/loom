# PLAN: Fix Ghostty terminal spawning on macOS

## Context

`TerminalEmulator::Ghostty` is wired into detection (`detection.rs`), the emulator enum + display (`emulator.rs`), parent-process matching, and AppleScript window-close paths (`window_ops.rs`). Detection on macOS already finds Ghostty via `/Applications/Ghostty.app` even when `which ghostty` fails (loom/src/orchestrator/terminal/native/detection.rs:190-191).

The actual spawn path is broken. `build_command()` for `Self::Ghostty` invokes `Command::new("ghostty")` with `--title TITLE --working-directory DIR -e bash -c CMD` (loom/src/orchestrator/terminal/emulator.rs:274-284). On macOS the Ghostty CLI binary lives inside the app bundle at `/Applications/Ghostty.app/Contents/MacOS/ghostty` and is **not** added to `PATH` automatically — see [ghostty-org/ghostty#2483](https://github.com/ghostty-org/ghostty/issues/2483). Result: detection picks Ghostty, then spawn fails with "Failed to spawn terminal 'ghostty'. Is it installed?".

The macOS-native way to launch a Ghostty window with arguments is `open -na Ghostty --args ...` (per Ghostty maintainer in [discussion #9221](https://github.com/ghostty-org/ghostty/discussions/9221) — "all the arguments work just as they do on linux `ghostty --arg=val`"). Using `open` works regardless of PATH and matches the pattern already used for Terminal.app / iTerm2 (which dispatch via `osascript`).

`-na` (force new instance) is chosen over `-a` because each loom worktree session needs its own `--working-directory` and `-e` to take effect; with `-a`, an already-running Ghostty instance may ignore `--args` for the new window. Process accumulation is acceptable since each window corresponds to a finite stage.

Linux behavior is unchanged.

## Files to Modify

- **`loom/src/orchestrator/terminal/emulator.rs`** — Make `build_command()` arm for `Self::Ghostty` platform-conditional. On macOS, build `open -na Ghostty --args --working-directory=DIR --title=TITLE -e bash -c CMD`. On Linux, keep current invocation.
- **`loom/src/orchestrator/terminal/emulator.rs` (tests)** — Add `test_ghostty_build_command_macos` (cfg-gated) verifying program is `open` and args contain `-na`, `Ghostty`, `--args`, `--working-directory=`, `--title=`, `-e`, `bash`, `-c`. Add `test_ghostty_build_command_linux` (cfg-gated) verifying program is `ghostty` with current arg shape.

## Implementation Sketch

In the `Self::Ghostty` arm of `build_command()` (~emulator.rs:274), replace the current single block with:

```rust
Self::Ghostty => {
    #[cfg(target_os = "macos")]
    {
        // Ghostty's CLI binary is inside /Applications/Ghostty.app/Contents/MacOS/
        // and is not on PATH by default (ghostty-org/ghostty#2483). Use `open -na`
        // which works without PATH setup and lets `--args` propagate to the new
        // process. -na (force new instance) ensures --working-directory and -e
        // take effect rather than being ignored by a running singleton.
        command = Command::new("open");
        command
            .arg("-na")
            .arg("Ghostty")
            .arg("--args")
            .arg(format!("--working-directory={}", workdir.display()))
            .arg(format!("--title={}", title))
            .arg("-e")
            .arg("bash")
            .arg("-c")
            .arg(cmd);
    }
    #[cfg(not(target_os = "macos"))]
    {
        command
            .arg("--title")
            .arg(title)
            .arg("--working-directory")
            .arg(workdir)
            .arg("-e")
            .arg("bash")
            .arg("-c")
            .arg(cmd);
    }
}
```

The reassignment requires `let mut command = ...` (already the case at line 114). No change needed to `binary()` — it still returns `"ghostty"`, which keeps detection's `which::which("ghostty")` check correct for Linux and any macOS user with the CLI shim installed; the macOS app-bundle fallback (detection.rs:190-191) handles the no-PATH case.

`window_ops.rs` paths for Ghostty (close/exists by title via `tell application "Ghostty"`) are unchanged — Ghostty 1.3+ supports the standard AppleScript app vocabulary.

PID tracking via wrapper script (`pid_tracking.rs`) is unchanged: the wrapper writes `$$` to `.work/pids/{stage_id}.pid` regardless of how the terminal was launched, and `spawner.rs` reads from that file.

## Verification

End-to-end on macOS with Ghostty installed and **not** on PATH:

```bash
cd loom && cargo build
cargo test -p loom orchestrator::terminal::emulator::tests
```

Manual smoke test (requires macOS + Ghostty):

```bash
LOOM_TERMINAL=Ghostty loom run   # in a loom-initialized project
```

Expect: Ghostty windows open per stage with the correct title and working directory, and `loom status` shows live sessions with PIDs resolved (proving the wrapper script ran).

Negative check (Ghostty not installed): detection should fall through to iTerm2 / Terminal.app instead of selecting Ghostty (already handled by `Path::new("/Applications/Ghostty.app").exists()` gate at detection.rs:190).

## Out of Scope

- TERM_PROGRAM detection for Ghostty (Ghostty does set `TERM_PROGRAM=ghostty`; could be added to the `match_process_to_terminal`/`from_name` paths later, but not required for spawn to work).
- Adding `ghostty` to PATH for the user — that's a Ghostty installer concern, tracked upstream.
- Replacing `-na` with the AppleScript `tell application "Ghostty"\n new window` approach — would avoid process accumulation but loses the ability to pass `--working-directory` / `-e` directly. Revisit if accumulation becomes a real UX problem.
