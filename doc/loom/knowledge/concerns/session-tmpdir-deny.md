# Session Deny on Claude Code's TMPDIR

>

`sandbox/control_surfaces/session_denies.rs::session_denies` leaves `<tmp>/claude-<uid>`, Claude Code's own `TMPDIR`, writable, task output files included: its Bash wrapper writes its cwd file there, so a deny on that root would fail every sandboxed command. Linux skips `denyWrite` globs, so no literal path can deny only the receipt channels under its `*/*/tasks` directories, and a sandboxed session can write them.

Direction: protect the receipt channels by a path the sandbox can express (a fixed directory outside `TMPDIR`) and add a canary that a sandboxed Bash call returns the command's own exit code. The history of the whole-root deny is in [sandbox-write-rules-inert](../mistakes/sandbox-write-rules-inert.md#denying-claude-codes-tmpdir-fails-every-sandboxed-bash-call).
