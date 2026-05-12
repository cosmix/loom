//! Convenience re-exports for the safe `.work/` write helpers.
//!
//! All real implementation lives in [`super::safe_fs`]. This module exists
//! to give callers a familiar `safe_write` import path for the common
//! single-call wrappers (`safe_locked_write`, `safe_append`,
//! `safe_create_new`, `safe_create_dir_all`), and to keep symlink-refusal
//! and size-limit semantics centralised in one place.
//!
//! Both code paths refuse to follow symlinks:
//!   * Linux: `openat2(RESOLVE_NO_SYMLINKS | RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS)`.
//!   * Portable fallback: component-by-component `openat(... O_NOFOLLOW | O_CLOEXEC)`
//!     so an intermediate or terminal symlink causes `ELOOP`.

pub use super::safe_fs::{
    safe_append, safe_append_in_workdir, safe_create_dir_all, safe_create_dir_all_in_workdir,
    safe_create_new, safe_create_new_in_workdir, safe_locked_write, safe_locked_write_in_workdir,
    safe_locked_write_str, safe_open_dirfd, safe_remove_in_workdir,
    safe_write_with_mode_in_workdir, MAX_LOG_BYTES,
};
