//! The macOS launch: the trusted bridge (`loom subagents worker-exec`) runs
//! under `sandbox-exec` with the boundary's Seatbelt profile, in a fresh
//! session, with a clean environment and exactly two inherited descriptors:
//! the loopback listener bound here before the profile was generated, and
//! the bridge's end of the control channel. The listener stays held here
//! until the boundary is retired, so its port, the only one the profile
//! grants, stays reserved while the supervisor lives.
use std::ffi::OsString;
use std::net::{Ipv4Addr, TcpListener};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use super::handshake::Control;
use super::macos_spec::{plan, MacPlan};
use super::private_root::{self, PrivateRoot};
use super::{kill_mac, seatbelt_profile, stderr_tail, BoundarySpec, Quiescence, SpawnedBoundary};
use crate::native_worker::model::MacIdentity;
use crate::native_worker::protocol::wire::SandboxMode;
use crate::process::{spawn_retrying_text_busy, OwnedProcess};

/// The system launcher that applies a profile and execs its program.
const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";
/// How long the bridge may take to confirm it runs inside the profile.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// The macOS half of a started boundary.
pub(crate) struct MacLaunch {
    pub(crate) identity: MacIdentity,
    control: Control,
    /// Keeps the bridge's port bound here, so no other process can take it
    /// while the boundary lives (the bridge serves its own copy).
    listener: TcpListener,
}

impl MacLaunch {
    pub(super) fn activate(&mut self) -> Result<SandboxMode> {
        self.control.activate()
    }
}

/// A macOS spawn that failed after the bridge was started, and whose cleanup
/// did not prove every process of its session gone: the caller must not
/// record ownership as released, and the private root stays on disk.
#[derive(Debug)]
pub(crate) struct MacStartedThenFailed(String);

impl std::fmt::Display for MacStartedThenFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the Seatbelt bridge was started, then failed: {}",
            self.0
        )
    }
}

impl std::error::Error for MacStartedThenFailed {}

impl SpawnedBoundary {
    /// Closes the control channel and the bridge's input, then stops the
    /// boundary's session (`kill_mac`), reaps the leader and, once release
    /// is proven, removes the private invocation root.
    pub(crate) fn retire(self) -> Quiescence {
        let SpawnedBoundary {
            process,
            stdin,
            stdout,
            stderr,
            mac,
            ..
        } = self;
        drop((stdin, stdout, stderr));
        retire_parts(mac, process)
    }
}

/// The leader stays unreaped while `kill_mac` runs: its zombie keeps the
/// session and group ids from being reused until the scan is done.
fn retire_parts(mac: MacLaunch, mut process: OwnedProcess) -> Quiescence {
    let MacLaunch {
        identity,
        control,
        listener,
    } = mac;
    drop(control);
    let quiescence = kill_mac(&identity);
    process.retire();
    drop(listener);
    private_root::reclaim(Some(&identity), quiescence).unwrap_or_else(Quiescence::Unverified)
}

/// The production macOS launch with `args` handed to `spec.codex_program`.
/// Every failure after the bridge may exist goes through [`retire_parts`];
/// one whose release is not proven is a [`MacStartedThenFailed`], keeping
/// the private root.
pub(crate) fn spawn_with_args(spec: &BoundarySpec, args: &[OsString]) -> Result<SpawnedBoundary> {
    let host: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    spawn_with_host(spec, args, &host)
}

/// [`spawn_with_args`] with `host` as the supervisor's environment, which
/// names the boundary's `HOME`, Cargo and rustup homes.
pub(crate) fn spawn_with_host(
    spec: &BoundarySpec,
    args: &[OsString],
    host: &[(OsString, OsString)],
) -> Result<SpawnedBoundary> {
    crate::native_worker::ids::validate_invocation_id(&spec.invocation)
        .map_err(|error| anyhow!("invalid invocation id {:?}: {error}", spec.invocation))?;
    anyhow::ensure!(
        Path::new(SANDBOX_EXEC).is_file(),
        "native Codex workers on macOS need {SANDBOX_EXEC}"
    );
    let boot_id = crate::process::boot_id::os_boot_id()?;
    let cache = spec
        .stage_cache
        .as_ref()
        .context("the macOS boundary needs the stage cache for its private root")?;
    let cache_root = cache.ensure()?;
    let private = private_root::create(&cache_root, &spec.invocation, &spec.codex_home)?;
    let prepared = prepare(spec, &private, host).and_then(|prepared| {
        let child = spawn_bridge(&prepared, spec, args)?;
        Ok((prepared, child))
    });
    match prepared {
        Ok((prepared, child)) => started(prepared, child, private, spec, boot_id),
        Err(error) => {
            // No process was started: nothing can hold the private root.
            if let Err(cleanup) = private_root::remove(&private.root, &spec.invocation) {
                tracing::warn!(error = %cleanup, "cannot remove an unused private root");
            }
            Err(error)
        }
    }
}

/// What the bridge is started with: the held listener, the profile and plan,
/// and both control channel ends.
struct Prepared {
    listener: TcpListener,
    plan: MacPlan,
    profile: String,
    supervisor_end: UnixStream,
    bridge_end: UnixStream,
}

fn prepare(
    spec: &BoundarySpec,
    private: &PrivateRoot,
    host: &[(OsString, OsString)],
) -> Result<Prepared> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .context("bind the bridge's loopback listener")?;
    let port = listener
        .local_addr()
        .context("read the bridge port")?
        .port();
    let plan = plan(spec, private, port, host)?;
    let profile = seatbelt_profile::profile_with(&plan.seatbelt, &plan.host_files)?;
    let (supervisor_end, bridge_end) =
        UnixStream::pair().context("create the bridge control channel")?;
    Ok(Prepared {
        listener,
        plan,
        profile,
        supervisor_end,
        bridge_end,
    })
}

/// `sandbox-exec -p <profile> <loom> subagents worker-exec ...` in a fresh
/// session, from the worktree, with only the plan's environment.
fn spawn_bridge(
    prepared: &Prepared,
    spec: &BoundarySpec,
    args: &[OsString],
) -> Result<std::process::Child> {
    let listener = prepared.listener.as_raw_fd();
    let control = prepared.bridge_end.as_raw_fd();
    let plan = &prepared.plan;
    let mut command = Command::new(SANDBOX_EXEC);
    command.arg("-p").arg(&prepared.profile).arg(&plan.loom);
    command.args(["subagents", "worker-exec", "--proxy-socket"]);
    command.arg(&spec.proxy_socket);
    command.arg("--listener-fd").arg(listener.to_string());
    command.arg("--control-fd").arg(control.to_string());
    command.arg("--").arg(&plan.codex).args(args);
    command
        .env_clear()
        .envs(plan.env.iter().map(|(k, v)| (k, v)))
        .current_dir(&plan.seatbelt.worktree)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    new_session_inheriting(&mut command, [listener, control]);
    spawn_retrying_text_busy(&mut command).context("start the Seatbelt bridge")
}

/// Makes the child a session (and group) leader keeping `inherited` open
/// across exec; every other descriptor of this process is close-on-exec.
fn new_session_inheriting(command: &mut Command, inherited: [RawFd; 2]) {
    // SAFETY: `setsid` and `fcntl` are async-signal-safe and touch only the
    // child's own session and descriptor flags.
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            for fd in inherited {
                if libc::fcntl(fd, libc::F_SETFD, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
}

/// Records the identity right after the leader exists, then waits a bounded
/// time for the bridge to confirm its profile.
fn started(
    prepared: Prepared,
    mut child: std::process::Child,
    private: PrivateRoot,
    spec: &BoundarySpec,
    boot_id: String,
) -> Result<SpawnedBoundary> {
    let Prepared {
        listener,
        supervisor_end,
        bridge_end,
        ..
    } = prepared;
    drop(bridge_end);
    let pipes = (child.stdin.take(), child.stdout.take(), child.stderr.take());
    let mut process = OwnedProcess::new(child);
    let pid = process.child().id();
    let identity = identify(pid, spec, boot_id, private.root).inspect_err(|_| {
        process.retire();
    })?;
    let mut mac = MacLaunch {
        identity,
        control: Control::new(supervisor_end),
        listener,
    };
    let (Some(stdin), Some(stdout), Some(stderr)) = pipes else {
        return Err(abandon(
            mac,
            process,
            None,
            "the bridge started without its standard streams",
        ));
    };
    if let Err(error) = mac.control.await_ready(HANDSHAKE_TIMEOUT) {
        drop((stdin, stdout));
        return Err(abandon(mac, process, Some(stderr), &format!("{error:#}")));
    }
    Ok(SpawnedBoundary {
        process,
        stdin,
        stdout,
        stderr,
        boundary_pid: pid as i32,
        pid_namespace: 0,
        mac,
    })
}

/// The identity of the leader `pid`, read right after it exists. Without its
/// start time nothing can prove its session gone later.
fn identify(
    pid: u32,
    spec: &BoundarySpec,
    boot_id: String,
    scratch_root: std::path::PathBuf,
) -> Result<MacIdentity> {
    let Some(leader_start) = crate::process::process_start_time(pid) else {
        let detail = format!("cannot read the start time of the bridge (pid {pid})");
        return Err(MacStartedThenFailed(detail).into());
    };
    Ok(MacIdentity {
        invocation: spec.invocation.clone(),
        session: pid,
        leader_start: leader_start.to_string(),
        boot_id,
        scratch_root,
    })
}

/// Retires a started bridge that cannot be used; the error says whether its
/// release was proven.
fn abandon(
    mac: MacLaunch,
    process: OwnedProcess,
    stderr: Option<std::process::ChildStderr>,
    detail: &str,
) -> anyhow::Error {
    let quiescence = retire_parts(mac, process);
    let tail = stderr.map(stderr_tail).unwrap_or_default();
    let detail = match tail.trim() {
        "" => detail.to_string(),
        tail => format!("{detail}; bridge stderr: {tail}"),
    };
    match quiescence {
        Quiescence::Released => {
            anyhow!("the Seatbelt bridge failed, its session emptied: {detail}")
        }
        Quiescence::Unverified(why) => MacStartedThenFailed(format!("{detail}; {why}")).into(),
    }
}
