//! Pid-namespace boundary around one codex app-server: spawn, nesting probe and quiescence check.
//!
//! The boundary is a bubblewrap sandbox with its own pid namespace, no network
//! (only loopback), a read-only root and an allowlisted environment. The stage
//! worktree is the only writable project path; `.loom`, `.git` and `.claude`
//! inside it stay read-only. The Codex home is a private tmpfs on which only
//! `sessions/`, the auth files and `AGENTS.md` are bound from the host.
//! `/tmp` is a private tmpfs mounted under every bind; the boundary's
//! `TMPDIR` is `BOUNDARY_TMPDIR`, an empty directory on it, and the Codex
//! turn policy makes only the worktree and `TMPDIR` writable to sandboxed
//! commands, never `/tmp` itself. The proxy socket directory is bound
//! read-only at `/loom-proxy` on the read-only root.
use std::path::PathBuf;

#[cfg(target_os = "linux")]
mod environment;
#[cfg(target_os = "linux")]
mod launch;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod mounts;
mod probe;
mod quiescence;
#[cfg(target_os = "linux")]
mod tmpdir;

pub use probe::native_backend_qualified;
pub(crate) use quiescence::identity_for;
#[cfg(target_os = "linux")]
pub(crate) use quiescence::kill_namespace;
pub use quiescence::verify_quiescent;

/// The boundary's `TMPDIR`, `TMP` and `TEMP`: a 0700 directory on the
/// private `/tmp` tmpfs, the only temporary directory sandboxed commands may
/// write.
pub(crate) const BOUNDARY_TMPDIR: &str = "/tmp/loom-tmp";

/// What one real boundary runs and exposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundarySpec {
    /// Read-write, except `.loom`, `.git` and `.claude` (read-only).
    pub worktree: PathBuf,
    /// Host `~/.codex`; a private tmpfs inside the boundary, onto which only
    /// `sessions/`, the auth files and `AGENTS.md` are bound from this path.
    pub codex_home: PathBuf,
    /// Executable path of the native codex binary.
    pub codex_program: PathBuf,
    /// Host path of the proxy Unix socket.
    pub proxy_socket: PathBuf,
    /// The running loom executable, mounted at `/loom-bin/loom`.
    pub loom_program: PathBuf,
}

/// A started boundary. The value must stay alive (not be destructured with
/// `..`) until the boundary is retired: it owns the private Cargo home that is
/// mounted inside, and dropping it removes that directory unless
/// [`keep_scratch_unless_released`] kept it.
pub(crate) struct SpawnedBoundary {
    /// The outer bubblewrap process, leader of its own process group.
    pub(crate) process: crate::process::OwnedProcess,
    pub(crate) stdin: std::process::ChildStdin,
    pub(crate) stdout: std::process::ChildStdout,
    pub(crate) stderr: std::process::ChildStderr,
    /// Pid of the outer bubblewrap; it is not a member of the namespace.
    pub(crate) boundary_pid: i32,
    /// Pid, on this side, of the namespace's first process (bubblewrap's
    /// child), from bubblewrap's info JSON.
    pub(crate) namespace_pid: i32,
    /// Inode of the boundary's pid namespace, from bubblewrap's info JSON.
    pub(crate) pid_namespace: u64,
    /// The private scratch (Cargo home) bound inside, marked with the
    /// namespace (`CargoScratch::mark_boundary`).
    #[cfg(target_os = "linux")]
    pub(crate) scratch: crate::verify::criteria::CargoScratch,
}

/// Keep `scratch` on disk unless `quiescence` proved its boundary's namespace
/// empty: its writable Cargo home may still be bound into live processes. The
/// sweep reclaims it once that namespace is gone.
#[cfg(target_os = "linux")]
pub(crate) fn keep_scratch_unless_released(
    scratch: &mut crate::verify::criteria::CargoScratch,
    quiescence: &Quiescence,
) {
    if *quiescence != Quiescence::Released {
        scratch.keep();
    }
}

/// Outcome of the ownership check after a boundary was stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Quiescence {
    /// No process of the namespace is left, or the host rebooted since.
    Released,
    /// Members remain, or the check could not complete; the text says which.
    Unverified(String),
}

/// Start `codex app-server` behind the loom launcher inside the real boundary.
pub(crate) fn spawn(spec: &BoundarySpec) -> anyhow::Result<SpawnedBoundary> {
    #[cfg(target_os = "linux")]
    {
        let host: Vec<_> = std::env::vars_os().collect();
        let plan = linux::Plan::real(spec, &host)?;
        linux::start(&plan, &host)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = spec;
        anyhow::bail!("native Codex workers need Linux")
    }
}

/// Whether a `spawn` error may leave a namespace process: bubblewrap ran and
/// neither exited before creating its namespace nor had it proven empty.
pub(crate) fn spawn_started(error: &anyhow::Error) -> bool {
    #[cfg(target_os = "linux")]
    {
        error.downcast_ref::<launch::StartedThenFailed>().is_some()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = error;
        false
    }
}

/// Any program in the same boundary, without proxy, launcher or codex binds.
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn spawn_payload(
    worktree: &std::path::Path,
    program: &std::path::Path,
    args: &[std::ffi::OsString],
) -> anyhow::Result<SpawnedBoundary> {
    let plan = linux::Plan::payload(worktree, None, program, args)?;
    linux::start(&plan, &std::env::vars_os().collect::<Vec<_>>())
}

/// `spawn_payload` plus the private Codex home mounts and the read-only
/// `W/.claude` bind of the real boundary (without the codex package bind).
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn spawn_payload_with_codex_home(
    worktree: &std::path::Path,
    codex_home: &std::path::Path,
    program: &std::path::Path,
    args: &[std::ffi::OsString],
) -> anyhow::Result<SpawnedBoundary> {
    let plan = linux::Plan::payload(worktree, Some(codex_home), program, args)?;
    linux::start(&plan, &std::env::vars_os().collect::<Vec<_>>())
}

#[cfg(all(test, target_os = "linux"))]
mod tests;

#[cfg(all(test, target_os = "linux"))]
mod codex_home_tests;

#[cfg(all(test, target_os = "linux"))]
mod policy_tests;

#[cfg(all(test, target_os = "linux"))]
mod scratch_tests;

#[cfg(all(test, target_os = "linux"))]
mod tmp_tests;
