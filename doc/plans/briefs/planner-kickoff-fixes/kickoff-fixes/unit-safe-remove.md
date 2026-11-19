# Unit safe-remove

Implementer: codex, gpt-6.2-sol, effort xhigh. Wave: 1.

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` in full first (the
settled decisions, the wire contract and, for codex units, "Rules for codex units"). Then read
section 2 of `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/delete.md`.

## Files you own (write only these)

- `loom/src/fs/safe_fs.rs` (re-exports only)
- `loom/src/fs/safe_fs/write.rs`
- `loom/src/fs/safe_fs_write_tests.rs`

## Entry points

`safe_remove_in_workdir` and the `openat` helpers in `loom/src/fs/safe_fs/write.rs`; `safe_open_dirfd` in `loom/src/fs/safe_fs.rs`.

## Done

`safe_remove_tree_in_workdir` and `safe_remove_empty_dir_in_workdir` exist with the pinned
signatures, the two tests are written, and `cargo check --all-targets` shows no error in your
files. Orchestrator proof: `cargo test --lib fs::`.

## Traps

This is the security core of Delete. Never follow a symlink at any depth: open every component with `O_NOFOLLOW`, unlink a link as a link. The comment names which directory-reading call keeps that guarantee on both macOS and Linux. Refuse `..` and absolute paths before touching anything. Stop with an error past 4096 entries rather than partly deleting silently; say in the error how many were removed.
