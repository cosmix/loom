# confinement-boundaries / w3-bounded-reads: byte-exact parent paths, no-follow bounded locked and session reads

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/security.md`, section CR-45.

This unit runs no `git` command, touches no path under `.loom/`, and runs no tests. Its proof is
`cargo check --all-targets` from `loom/`. The orchestrator runs the tests named below.

## Findings covered

- CR-45: `safe_fs/write.rs` rebuilds a parent path with `String::from_utf8_lossy`, so a non-UTF-8
  directory name opens a different path; `fs/locking.rs` `locked_read` and `locked_update` open
  the data file with `File::open` (follows a symlink, blocks on a FIFO, reads without a cap);
  `fs/session_files.rs::find_session_for_stage` reads every `sessions/*.md` with
  `fs::read_to_string`.

## Files

Own (write): `loom/src/fs/safe_fs/write.rs`; `loom/src/fs/safe_fs/write/non_utf8_tests.rs` (new);
`loom/src/fs/locking.rs`; `loom/src/fs/locking/tests.rs`; `loom/src/fs/session_files.rs`.

Read only: `loom/src/fs/safe_fs.rs` (`validate_relpath`, `open_dir_safely`, `safe_open_dirfd`),
`loom/src/fs/safe_read.rs` (`read_open_bounded`, `read_to_string_bounded`),
`loom/src/fs/session_files/enumerate.rs`.

Frozen (contract test, never edit): `loom/src/fs/safe_fs/non_utf8_parent_contract_tests.rs`.

## Changes

### 1. Byte-exact parent paths (`fs/safe_fs/write.rs`)

- Add `fn components_path(components: &[Vec<u8>]) -> PathBuf` that pushes each component as
  `OsStr::from_bytes` (`std::os::unix::ffi::OsStrExt`).
- `parent_dirfd_for`: replace the `from_utf8_lossy` join with
  `open_dir_safely(dirfd, &components_path(parent_components))`.
- `safe_remove_in_workdir`: build `parent` with `components_path(&components[..len - 1])`; the
  empty test becomes `parent.as_os_str().is_empty()`.
- These two are the only `from_utf8_lossy` uses under `fs/safe_fs`; none remains afterwards.
- Declare at the end of `write.rs`:
  `#[cfg(all(test, not(target_os = "macos")))] mod non_utf8_tests;` (APFS rejects such names).

### 2. No-follow bounded locked reads (`fs/locking.rs`)

- Add `const MAX_LOCKED_FILE_BYTES: usize = 64 * 1024 * 1024;` with a doc saying it bounds a file
  read under the directory lock.
- Add `fn open_no_follow(path: &Path) -> std::io::Result<File>`:
  `OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path)`.
  `O_NONBLOCK` keeps a FIFO from blocking the open; it has no effect on a regular file.
- Add `fn read_opened(file: File, path: &Path) -> Result<String>`:
  `crate::fs::safe_read::read_open_bounded(file, path, MAX_LOCKED_FILE_BYTES)` (it refuses a
  non-regular file and a file over the cap), then `String::from_utf8` with context
  `Failed to read file: {path}: not valid UTF-8`.
- `locked_read`: `let file = open_no_follow(path).with_context(|| format!("Failed to open file: {}", path.display()))?; read_opened(file, path)`.
  The `io::Error` stays in the chain, so a caller testing for `NotFound` still sees it.
- `locked_update`: `match open_no_follow(path)`: `Ok(file) => read_opened(file, path)?`;
  `Err(e) if e.kind() == NotFound => String::new()`; other errors as today.
- Remove the now unused `BufReader`/`Read` imports.
- `lock_parent_dir` is NOT changed: it opens the parent directory to lock it, and that parent is
  legitimately reached through a symlink (a stage worktree's `.loom/work`).
- Callers: none changes. The signatures stay; all 22 call sites outside `locking.rs` keep
  compiling. Behaviour changes only for a data file that is a symlink, a FIFO or over 64 MiB,
  which now errors. The one plausibly symlinked target is the user config written by
  `user_config/write.rs` through `locked_update`; `atomic_write` already replaced such a link with
  a plain file, so the refusal surfaces earlier with a clear error. Do not edit `user_config`.

### 3. Bounded session reads (`fs/session_files.rs::find_session_for_stage`)

Replace `fs::read_to_string(&path)` with
`crate::fs::safe_read::read_to_string_bounded(&sessions_dir, Path::new(&entry.file_name()), 1024 * 1024)`
(the same 1 MiB bound `session_files/enumerate.rs` uses). Keep the `if let Ok(..)` shape: an
unreadable or oversized file is skipped as today. Do not touch `find_session_file`: it is listed
in `loom/maintainability-baseline.txt` and must not grow.

## Interfaces shared with other workers

None.

## Tests (written by this unit, run by the orchestrator)

- `fs/safe_fs/write/non_utf8_tests.rs` (new), using `tempfile::TempDir`, a directory named by
  `OsStr::from_bytes(b"a\xff")` under it and `crate::fs::safe_fs::safe_open_dirfd`:
  - `a_directory_is_created_under_a_non_utf8_parent`: `safe_create_new_dir_in_workdir(fd, "a\xff/made", 0o700)`
    succeeds and `<root>/a\xff/made` is a directory.
  - `a_rename_under_a_non_utf8_parent_lands_there`: a file `a\xff/src` written with `std::fs`,
    `safe_rename_in_workdir(fd, "a\xff/src", "a\xff/dst")` succeeds, `dst` exists, `src` does not.
  - `a_remove_under_a_non_utf8_parent_removes_that_file`: `safe_remove_in_workdir(fd, "a\xff/dst")`
    removes it.
  Each fails today with `ENOENT`.
- `fs/locking/tests.rs`:
  - `test_locked_read_refuses_a_symlinked_file`: a real file outside, a symlink at the target;
    `locked_read(&link)` errs. Today it reads through the link.
  - `test_locked_update_refuses_a_symlinked_file_and_leaves_the_target`: `locked_update` errs and
    the outside file's bytes are unchanged.
  - `test_locked_read_refuses_a_fifo_without_blocking`: `libc::mkfifo` at the path (mode 0o600);
    run `locked_read` on a spawned thread that sends its result over an `mpsc` channel, and
    assert `recv_timeout(Duration::from_secs(5))` yields an `Err` result. Today the open blocks
    and the receive times out.
  - `test_locked_update_treats_a_missing_file_as_empty` if no existing test covers it.
- Contract to turn green: `lossy-non-utf8-parent-path`.

## Proof

`cargo check --all-targets` from `loom/`. The orchestrator then runs
`cargo test --lib fs::safe_fs::`, `cargo test --lib fs::locking::`,
`cargo test --lib fs::session_files::`.

## Traps

- Fixing `parent_dirfd_for` but not `safe_remove_in_workdir`, which builds its own parent.
- Opening with `O_NOFOLLOW` only and forgetting `O_NONBLOCK`: a FIFO blocks the daemon.
- Mapping every open error to an empty string in `locked_update`: only `NotFound` is empty.
- Adding `O_NOFOLLOW` to `lock_parent_dir`: it breaks every worktree path through `.loom/work`.
- Converting the locked read result with `from_utf8_lossy`: invalid UTF-8 was an error before and
  stays one.
