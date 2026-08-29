//! Package-manager cache directories every stage must be able to WRITE.
//!
//! `bun install` / `cargo add` / `uv sync` / `go get` inside a worktree session
//! die with `EROFS` / `Read-only file system` because the sandbox's write set
//! is the worktree, the session temp dir and whatever the plan lists — never
//! the manager's per-user cache. The stage then fails at its very first
//! dependency install, before any of its own code runs.
//!
//! Emitted as `sandbox.filesystem.allowWrite` — additive, tilde-expanded,
//! OS-enforced for child processes; the one lever that reaches a subprocess
//! (see [`crate::codex::CODEX_SANDBOX_WRITE_PATHS`] for the sibling grant this
//! mirrors).
//!
//! **Policy: CACHE DIRECTORIES ONLY, never a credential-bearing parent.**
//! `~/.cargo` as a whole is NOT granted (`~/.cargo/credentials.toml` lives
//! there); `~/.npm` is (npm's cache dir by definition — `~/.npmrc` lives
//! outside it); `~/.m2`, `~/.gradle`, `~/.config` are not on the list at all.
//! Linux and macOS locations are both listed because Claude Code's sandbox
//! SKIPS a listed path that does not exist
//! (`[Sandbox Linux] Skipping non-existent write path`), so an entry for the
//! other OS, or for a manager the user never installed, costs nothing.
//!
//! **Two limits, stated plainly:**
//! 1. A cache directory that does not exist at session start is not bound — a
//!    manager used for the very first time on the host fails until the
//!    directory exists.
//! 2. A cache relocated by an env var (`XDG_CACHE_HOME`, `CARGO_HOME`,
//!    `RUSTUP_HOME`, `BUN_INSTALL_CACHE_DIR`, `npm_config_cache`,
//!    `UV_CACHE_DIR`, `GOMODCACHE`, `DENO_DIR`, `PNPM_HOME`) is not covered.
//!
//! Both are granted through the plan's `sandbox.filesystem.allow_write`.

/// Per-user package-manager cache directories, tilde form, granted to EVERY stage.
pub const PACKAGE_MANAGER_CACHE_WRITE_PATHS: [&str; 29] = [
    // bun
    "~/.bun/install/cache",
    // npm — the whole dir is cache/logs (`_cacache`, `_logs`, `_npx`, `_prebuilds`, update-notifier stamp)
    "~/.npm",
    // pnpm — store, metadata cache, state; Linux then macOS
    "~/.local/share/pnpm",
    "~/.cache/pnpm",
    "~/.local/state/pnpm",
    "~/Library/pnpm",
    "~/Library/Caches/pnpm",
    // yarn — v1 cache (Linux, macOS) and berry's global cache
    "~/.cache/yarn",
    "~/Library/Caches/Yarn",
    "~/.yarn/berry",
    // deno
    "~/.cache/deno",
    "~/Library/Caches/deno",
    // cargo — registry + git checkouts, plus the lock/tracking files cargo opens
    // read-write at the top of CARGO_HOME. NOT `~/.cargo` itself: credentials.toml.
    "~/.cargo/registry",
    "~/.cargo/git",
    "~/.cargo/.package-cache",
    "~/.cargo/.package-cache-mutate",
    "~/.cargo/.global-cache",
    // rustup — a `rust-toolchain.toml` pin auto-installs into these
    "~/.rustup/toolchains",
    "~/.rustup/downloads",
    "~/.rustup/tmp",
    "~/.rustup/update-hashes",
    // uv — cache (Linux, macOS) and managed pythons/tools
    "~/.cache/uv",
    "~/Library/Caches/uv",
    "~/.local/share/uv",
    // pip
    "~/.cache/pip",
    "~/Library/Caches/pip",
    // go — module cache (`~/go/pkg/mod`) and build cache
    "~/go/pkg",
    "~/.cache/go-build",
    "~/Library/Caches/go-build",
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
    fn covers_the_managers_the_project_standardises_on() {
        // CLAUDE.md names bun, cargo, uv, go as the sanctioned package managers.
        for expected in [
            "~/.bun/install/cache",
            "~/.cargo/registry",
            "~/.cache/uv",
            "~/go/pkg",
        ] {
            assert!(
                PACKAGE_MANAGER_CACHE_WRITE_PATHS.contains(&expected),
                "missing standard manager cache: {expected}"
            );
        }
    }
}
