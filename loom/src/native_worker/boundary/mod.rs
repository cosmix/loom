//! The boundary around one codex app-server: spawn, qualification probe and
//! quiescence check.
//!
//! Linux: a bubblewrap sandbox with its own pid namespace, no network (only
//! loopback), a read-only root and an allowlisted environment. The stage
//! worktree is the only writable project path; `.loom`, `.git` and `.claude`
//! inside it stay read-only. The Codex home is a private tmpfs on which only
//! `sessions/`, the auth files and `AGENTS.md` are bound from the host.
//! `/tmp` is a private tmpfs mounted under every bind; the boundary's
//! `TMPDIR` is `BOUNDARY_TMPDIR`, an empty directory on it, and the Codex
//! turn policy makes only the worktree and `TMPDIR` writable to sandboxed
//! commands, never `/tmp` itself. The proxy socket directory is bound
//! read-only at `/loom-proxy` on the read-only root.
//!
//! macOS: one Seatbelt profile (`seatbelt_profile`) wraps the trusted bridge
//! and everything Codex starts, in a fresh session (`macos`). The writable
//! roots are the worktree and a private invocation root under the stage
//! cache (`private_root`) holding the temp directory and the Codex home copy.
use std::path::PathBuf;

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod environment;
pub(crate) mod handshake;
#[cfg(target_os = "linux")]
mod launch;
#[cfg(target_os = "linux")]
mod linux;
pub(crate) mod loom_program;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_spec;
mod mounts;
pub(crate) mod private_root;
mod probe;
mod quiescence;
pub mod quiescence_scan;
pub mod seatbelt_profile;
#[cfg(target_os = "linux")]
mod tmpdir;

#[cfg(target_os = "macos")]
pub(crate) use macos::spawn_with_args;
pub use probe::native_backend_qualified;
pub(crate) use quiescence::identity_for;
#[cfg(target_os = "macos")]
pub(crate) use quiescence::kill_mac;
#[cfg(target_os = "linux")]
pub(crate) use quiescence::kill_namespace;
pub use quiescence::verify_quiescent;

#[cfg(target_os = "macos")]
use crate::native_worker::model::MacIdentity;
use crate::native_worker::protocol::wire::SandboxMode;

/// The boundary's `TMPDIR`, `TMP` and `TEMP`: a 0700 directory on the
/// private `/tmp` tmpfs, the only temporary directory sandboxed commands may
/// write.
#[cfg(target_os = "linux")]
pub(crate) const BOUNDARY_TMPDIR: &str = "/tmp/loom-tmp";

/// What one real boundary runs and exposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundarySpec {
    /// Read-write, except `.loom`, `.git` and `.claude` (read-only).
    pub worktree: PathBuf,
    /// Host `~/.codex`. Linux: a private tmpfs inside the boundary, onto
    /// which only `sessions/`, the auth files and `AGENTS.md` are bound from
    /// this path. macOS: hidden; the auth files and `AGENTS.md` are copied
    /// into the private invocation root.
    pub codex_home: PathBuf,
    /// Executable path of the native codex binary.
    pub codex_program: PathBuf,
    /// Host path of the proxy Unix socket.
    pub proxy_socket: PathBuf,
    /// The running loom executable, mounted at `/loom-bin/loom` (Linux).
    pub loom_program: PathBuf,
    /// The stage's build cache, created by the supervisor's host. Linux:
    /// bound read-write at its own path and exported through the boundary's
    /// environment; `None` leaves the boundary without one. macOS: the
    /// private invocation root lives below it, and a launch without one is
    /// refused.
    pub stage_cache: Option<crate::stage_cache::StageCache>,
    /// The validated invocation id (`ids::validate_invocation_id`). macOS
    /// names the private root and the boundary marker after it; Linux
    /// ignores it.
    pub invocation: String,
}

/// A started boundary. The value must stay alive (not be destructured with
/// `..`) until the boundary is retired: on Linux it owns the private Cargo
/// home that is mounted inside, and dropping it removes that directory unless
/// [`keep_scratch_unless_released`] kept it; on macOS it holds the bridge's
/// control channel and the reserved loopback port.
pub(crate) struct SpawnedBoundary {
    /// Linux: the outer bubblewrap process, leader of its own process group.
    /// macOS: the bridge, leader of its own session and process group.
    pub(crate) process: crate::process::OwnedProcess,
    pub(crate) stdin: std::process::ChildStdin,
    pub(crate) stdout: std::process::ChildStdout,
    pub(crate) stderr: std::process::ChildStderr,
    /// Linux: pid of the outer bubblewrap; it is not a member of the
    /// namespace. macOS: the leader's pid, also its session and group id.
    pub(crate) boundary_pid: i32,
    /// Pid, on this side, of the namespace's first process (bubblewrap's
    /// child), from bubblewrap's info JSON.
    #[cfg(target_os = "linux")]
    pub(crate) namespace_pid: i32,
    /// Inode of the boundary's pid namespace, from bubblewrap's info JSON;
    /// 0 on macOS, which has none.
    pub(crate) pid_namespace: u64,
    /// The private scratch (Cargo home) bound inside, marked with the
    /// namespace (`CargoScratch::mark_boundary`).
    #[cfg(target_os = "linux")]
    pub(crate) scratch: crate::verify::criteria::CargoScratch,
    /// The bridge's control channel, the reserved listener and the identity.
    #[cfg(target_os = "macos")]
    pub(crate) mac: macos::MacLaunch,
}

impl SpawnedBoundary {
    /// The macOS identity recorded before the bridge may start Codex.
    #[cfg(target_os = "macos")]
    pub(crate) fn mac_identity(&self) -> Option<&MacIdentity> {
        Some(&self.mac.identity)
    }

    /// Called once, after the supervisor recorded the identity. macOS:
    /// releases the bridge, which confirmed entering the Seatbelt profile
    /// during `spawn`, to start Codex, and returns `DangerFullAccess`. Linux:
    /// Codex already runs; returns `WorkspaceWrite`. An `Err` means Codex was
    /// not started and the protocol must not start.
    pub(crate) fn activate(&mut self) -> anyhow::Result<SandboxMode> {
        #[cfg(target_os = "macos")]
        {
            self.mac.activate()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(SandboxMode::WorkspaceWrite)
        }
    }
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
    /// No process of the boundary is left, or the host rebooted since.
    Released,
    /// Members remain, or the check could not complete; the text says which.
    Unverified(String),
}

/// Bound on the stderr kept from a boundary that failed to start.
#[cfg(any(target_os = "linux", target_os = "macos"))]
const STDERR_TAIL_BYTES: u64 = 2048;

/// At most [`STDERR_TAIL_BYTES`] of a retired boundary's stderr, waiting at
/// most one second.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn stderr_tail(stderr: std::process::ChildStderr) -> String {
    use std::io::Read;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.take(STDERR_TAIL_BYTES).read_to_end(&mut bytes);
        let _ = sender.send(String::from_utf8_lossy(&bytes).into_owned());
    });
    receiver
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap_or_default()
}

/// The arguments the boundary hands the codex program.
#[cfg(target_os = "macos")]
fn app_server_args() -> Vec<std::ffi::OsString> {
    ["app-server", "--listen", "stdio://"]
        .map(std::ffi::OsString::from)
        .into()
}

/// Start `codex app-server` behind the loom launcher inside the real boundary.
pub(crate) fn spawn(spec: &BoundarySpec) -> anyhow::Result<SpawnedBoundary> {
    #[cfg(target_os = "linux")]
    {
        let host: Vec<_> = std::env::vars_os().collect();
        let plan = linux::Plan::real(spec, &host)?.with_stage_cache(spec.stage_cache.clone());
        linux::start(&plan, &host)
    }
    #[cfg(target_os = "macos")]
    {
        spawn_with_args(spec, &app_server_args())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = spec;
        anyhow::bail!("native Codex workers need Linux or macOS")
    }
}

/// Whether a `spawn` error may leave a boundary process: Linux, bubblewrap
/// ran and neither exited before creating its namespace nor had it proven
/// empty; macOS, the bridge was started and cleanup did not prove release.
pub(crate) fn spawn_started(error: &anyhow::Error) -> bool {
    #[cfg(target_os = "linux")]
    {
        error.downcast_ref::<launch::StartedThenFailed>().is_some()
    }
    #[cfg(target_os = "macos")]
    {
        error
            .downcast_ref::<macos::MacStartedThenFailed>()
            .is_some()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
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

#[cfg(test)]
mod seatbelt_profile_tests;

#[cfg(test)]
mod private_root_tests;

#[cfg(test)]
mod quiescence_scan_tests;

#[cfg(test)]
mod seatbelt_profile_contract_tests;

#[cfg(test)]
mod quiescence_scan_contract_tests;

#[cfg(test)]
mod probe_contract_tests;
