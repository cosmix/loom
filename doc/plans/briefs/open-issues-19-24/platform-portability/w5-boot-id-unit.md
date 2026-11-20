# W5: boot-ID resolver unit (issue 24)

You write ONE Rust file with its inline tests: `loom/src/process/boot_id.rs`. Nothing else.

## Rules for this unit

- Never run git in any form. Never create, read or write anything under a `.loom/` path.
- Never run cargo, a build, a test, a formatter or a linter. The orchestrator proves the file.
- Touch only `loom/src/process/boot_id.rs`. The line `pub mod boot_id;` already exists in
  `loom/src/process/mod.rs`; do not edit that file.
- The file may be created if it is missing, and replaced if a placeholder exists.
- Never write an apostrophe anywhere in the file, comments and character literals included. Where a
  character literal would be natural, use a string or a method call instead.
- Rustdoc is built with warnings denied: in doc comments, put every code-like token in backticks and
  never write angle brackets outside backticks.
- No TODO markers, no stubs, no unused imports, no dead code. Keep functions short.

## Purpose

macOS denies the sandbox the `kern.bootsessionuuid` sysctl, so the daemon reads it unsandboxed and
exports it to each session as the environment variable `LOOM_BOOT_ID`. This module resolves a boot
identity: a well-formed `LOOM_BOOT_ID` first, then the operating system source.

## Public interface (exact)

```rust
pub const BOOT_ID_ENV: &str = "LOOM_BOOT_ID";

pub fn resolve_boot_id(
    env_value: Option<&str>,
    os_boot_id: impl FnOnce() -> anyhow::Result<String>,
) -> anyhow::Result<String>;

pub fn os_boot_id() -> anyhow::Result<String>;

pub fn current_boot_id() -> anyhow::Result<String>;
```

Behaviour:

- `resolve_boot_id` trims `env_value`. When the trimmed text is UUID-shaped it returns that trimmed
  text (case preserved) and never calls `os_boot_id`. In every other case (None, empty, whitespace,
  wrong shape) it returns whatever `os_boot_id()` returns, an error included.
- UUID-shaped means five groups separated by single dashes with lengths 8, 4, 4, 4 and 12, every
  character an ASCII hex digit of either case. Implement it as a private `fn is_uuid_shaped(value:
  &str) -> bool` with `value.split("-")`, no regex crate.
- `current_boot_id` is exactly
  `resolve_boot_id(std::env::var(BOOT_ID_ENV).ok().as_deref(), os_boot_id)`.
- `os_boot_id` has three compiled variants selected by `cfg(target_os)`:
  - Linux: read `/proc/sys/kernel/random/boot_id` with `std::fs::read_to_string`, context
    `failed to read Linux boot ID`, trim, bail with `Linux boot ID is empty` when empty, return the
    trimmed text.
  - macOS: call a private `fn macos_boot_id() -> Result<String>` (body below).
  - Any other target: `bail!("boot identity is unsupported on this operating system")`.

The macOS body is `macos_boot_session_uuid` in `loom/src/commands/subagents/wait/lease.rs` at your
base (PR #25 split it out of `macos_boot_id`; W4 deletes that file region, so copy from the text
below). Read `kern.bootsessionuuid` only: the PR also added a `kern.boottime` fallback
(`macos_boot_time`) that you do not carry over, because macOS recomputes `kern.boottime` when the
wall clock is stepped, so the value can change within one boot, and an identity written under one
source and read under the other looks like a different boot. `LOOM_BOOT_ID` from the daemon removes
the need for a fallback. No `kern.boottime` text may appear in the file. Copy the body with these
two edits only: the character-array trim becomes a closure, and the file gains a module doc comment.

```rust
#[cfg(target_os = "macos")]
fn macos_boot_id() -> Result<String> {
    let name = b"kern.bootsessionuuid\0";
    let mut size = 0usize;
    // SAFETY: the name is NUL-terminated and the null output pointer requests the required size.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr().cast(),
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error()).context("failed to size macOS boot ID");
    }
    let mut bytes = vec![0u8; size];
    // SAFETY: `bytes` provides `size` writable bytes and the name remains NUL-terminated.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr().cast(),
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error()).context("failed to read macOS boot ID");
    }
    bytes.truncate(size);
    let value = std::str::from_utf8(&bytes)?.trim_matches(|c: char| c == char::from(0) || c.is_whitespace());
    if value.is_empty() {
        bail!("macOS boot ID is empty");
    }
    Ok(value.to_owned())
}
```

Imports: `use anyhow::{bail, Context, Result};` and keep each of the three names used on Linux, on
macOS and on other targets (put an import behind the same `cfg` as its only user, or reference it
with a full path, so no target warns about an unused import). `libc` is already a dependency.

## Steps (three)

1. Write the module: the module doc comment, `BOOT_ID_ENV`, `is_uuid_shaped`, `resolve_boot_id`,
   `os_boot_id` in its three variants with `macos_boot_id`, and `current_boot_id`, each public item
   with a one-sentence doc comment.
2. Append `#[cfg(test)] mod tests` with `use super::*;` and these tests, using `std::cell::Cell` to
   record whether the closure ran and `anyhow::anyhow!("os source denied")` for the failing source:
   - `valid_env_wins_without_calling_the_closure`: input `0f8fad5b-d9cb-469f-a165-70867728950e`,
     the closure sets a flag, the result equals the input, the flag is still false.
   - `env_value_is_trimmed_before_the_shape_check`: input with a leading space and a trailing
     newline returns the bare UUID.
   - `mixed_case_uuid_is_accepted`: `0F8fAD5b-d9CB-469f-A165-70867728950E` is returned unchanged.
   - `empty_env_falls_back_to_the_os_source`: `Some("")` and `Some("   ")` return the closure value.
   - `non_uuid_env_falls_back_to_the_os_source`: `Some("not-a-uuid")`, a 36-character string with
     the wrong dash positions, and a UUID with a non-hex digit each return the closure value.
   - `missing_env_falls_back_to_the_os_source`: `None` returns the closure value.
   - `os_error_with_no_env_propagates`: `None` and a failing closure give an `Err` whose text
     contains `os source denied`.
   - `os_error_with_an_invalid_env_propagates`: `Some("garbage")` and a failing closure give the
     same `Err`.
   - `linux_os_boot_id_is_uuid_shaped`, gated with `#[cfg(target_os = "linux")]`: `os_boot_id()`
     succeeds and `is_uuid_shaped` accepts the result.
3. Read the file once more as a reviewer: every public item matches the interface above, no
   apostrophe exists anywhere, no unused import on any target, no function exceeds 50 lines, and the
   file is well under 400 lines. Fix what you find, then stop.

## Report

One short message: the file path, and any line where you deviated from this brief with the reason.
