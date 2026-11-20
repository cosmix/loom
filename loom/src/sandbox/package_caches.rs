//! Rust package-manager cache directories every stage must be able to WRITE.
//!
//! `cargo add` / `cargo fetch` inside a worktree session die with `EROFS` /
//! `Read-only file system` because the sandbox's write set is the worktree, the
//! session temp dir and whatever the plan lists, never the manager's per-user
//! cache. The stage then fails at its very first dependency install, before
//! any of its own code runs.
//!
//! Every other ecosystem (bun, npm, pnpm, yarn, deno, uv, pip, go, ccache,
//! sccache, gradle, maven) writes the stage's own cache root instead
//! ([`crate::stage_cache::session::apply_session_grant`]), with the host caches
//! as read-only seeds. Cargo and rustup stay here: their homes also hold the
//! toolchains and credentials of the operator's other work, so they cannot move
//! under the root.
//!
//! Emitted as `sandbox.filesystem.allowWrite`: additive, tilde-expanded,
//! OS-enforced for child processes; the one lever that reaches a subprocess
//! (see [`crate::codex::CODEX_SANDBOX_WRITE_PATHS`] for the sibling grant this
//! mirrors).
//!
//! **Policy: CACHE DIRECTORIES ONLY, never a credential-bearing parent.**
//! `~/.cargo` as a whole is NOT granted (`~/.cargo/credentials.toml` lives
//! there); `~/.m2`, `~/.gradle`, `~/.config` are not on the list at all.
//! Claude Code's sandbox SKIPS a listed path that does not exist
//! (`[Sandbox Linux] Skipping non-existent write path`), so an entry for a
//! manager the user never installed costs nothing.
//!
//! **Two limits:**
//! 1. A cache directory that does not exist at session start is not bound: a
//!    manager used for the very first time on the host fails until the
//!    directory exists.
//! 2. A cache relocated by an env var (`CARGO_HOME`, `RUSTUP_HOME`) is not
//!    covered.
//!
//! Both are granted through the plan's `sandbox.filesystem.allow_write`.

/// Per-user cargo and rustup cache directories, tilde form, granted to EVERY stage.
///
/// Owner decision 11: `~/.rustup/toolchains` is dropped entirely, an
/// executable install directory and not a cache; the operator preinstalls
/// toolchains instead.
pub const PACKAGE_MANAGER_CACHE_WRITE_PATHS: [&str; 8] = [
    // cargo: registry + git checkouts, plus the lock/tracking files cargo opens
    // read-write at the top of CARGO_HOME. NOT `~/.cargo` itself: credentials.toml.
    "~/.cargo/registry",
    "~/.cargo/git",
    "~/.cargo/.package-cache",
    "~/.cargo/.package-cache-mutate",
    "~/.cargo/.global-cache",
    // rustup: download/tracking files only. NOT `~/.rustup/toolchains`: see
    // the module doc.
    "~/.rustup/downloads",
    "~/.rustup/tmp",
    "~/.rustup/update-hashes",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_is_a_tilde_path_without_traversal() {
        for path in PACKAGE_MANAGER_CACHE_WRITE_PATHS {
            assert!(path.starts_with("~/"), "not a tilde path: {path}");
            assert!(!path.contains("../"), "parent traversal in: {path}");
            assert!(!path.ends_with('/'), "trailing slash in: {path}");
        }
    }

    #[test]
    fn no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for path in PACKAGE_MANAGER_CACHE_WRITE_PATHS {
            assert!(seen.insert(path), "duplicate entry: {path}");
        }
    }

    #[test]
    fn never_grants_a_credential_bearing_parent() {
        // These are the parents deliberately excluded (bin/lib/config live
        // there, or they are simply too broad) - this test pins that policy.
        const EXCLUDED_PARENTS: &[&str] = &[
            "~/.cargo",
            "~/.rustup",
            "~/.cache",
            "~/.local/share",
            "~/.local",
            "~",
            "~/",
            "~/.config",
            "~/.bun",
            "~/.yarn",
            "~/go",
        ];
        for parent in EXCLUDED_PARENTS {
            assert!(
                !PACKAGE_MANAGER_CACHE_WRITE_PATHS.contains(parent),
                "must not grant the whole parent dir: {parent}"
            );
        }
    }

    #[test]
    fn never_grants_an_executable_install_directory() {
        // Section 11 ("`LOOM_BIN`, hook helpers via `LOOM_HOOK_PATH`"): a
        // package-manager grant that reaches an install directory lets a
        // session overwrite `loom`, `cargo`, `uv`, or any hook helper
        // resolved through the filtered PATH. Pins owner decision 11
        // (rustup, uv) and extends it to the other managers' bin/global
        // install directories, including the pnpm home itself — pnpm installs
        // global binaries there, not only under `<home>/store`.
        const INSTALL_DIRS: &[&str] = &[
            "~/.rustup/toolchains",
            "~/.local/share/uv",
            "~/.local/bin",
            "~/.cargo/bin",
            "~/.bun/bin",
            "~/.local/share/pnpm",
            "~/Library/pnpm",
        ];
        for install_dir in INSTALL_DIRS {
            for granted in PACKAGE_MANAGER_CACHE_WRITE_PATHS {
                assert!(
                    granted != *install_dir,
                    "grants the install directory itself: {install_dir}"
                );
                assert!(
                    !install_dir.starts_with(&format!("{granted}/")),
                    "{granted} is an ancestor of install directory {install_dir}"
                );
            }
        }
    }

    #[test]
    fn keeps_cargo_and_rustup_and_drops_the_stage_cache_ecosystems() {
        for expected in ["~/.cargo/registry", "~/.rustup/downloads"] {
            assert!(
                PACKAGE_MANAGER_CACHE_WRITE_PATHS.contains(&expected),
                "missing cache: {expected}"
            );
        }
        for replaced in [
            "~/.bun/install/cache",
            "~/.npm",
            "~/.cache/uv",
            "~/.cache/pip",
            "~/.cache/deno",
            "~/.yarn/berry",
            "~/.local/share/pnpm/store",
            "~/go/pkg",
            "~/.cache/go-build",
        ] {
            assert!(
                !PACKAGE_MANAGER_CACHE_WRITE_PATHS.contains(&replaced),
                "the stage cache replaces {replaced}"
            );
        }
    }
}
