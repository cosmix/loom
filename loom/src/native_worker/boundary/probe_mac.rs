//! The real macOS host's qualification probes. Each runs in a hermetic scratch
//! (`probe_mac_fixture.rs`) and is bounded: the Seatbelt availability probe
//! has its own deadline, and the two launches stop at a deadline and are
//! retired, which kills and reaps their whole process group.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::judge::{classify_sandbox_exec, judge_codex_home, judge_payload, PAYLOAD_SCRIPT};
use super::mac_fixture::{Fixture, Pipes, Rpc};
use super::MacHostProbes;
use crate::native_worker::boundary::{self, Quiescence, SpawnedBoundary};
use crate::native_worker::protocol::wire::{self, SandboxMode};

/// How long the payload launch may take, from spawn to exit.
const PAYLOAD_TIMEOUT: Duration = Duration::from_secs(10);

/// How long the Codex launch may take for `initialize` and `thread/start`.
/// Longer than the payload: a cold Codex start also tries, and fails offline,
/// to refresh its model list before it answers (about 3 s measured).
const CODEX_TIMEOUT: Duration = Duration::from_secs(30);

/// The shell that stands in for Codex in the payload launch.
const PAYLOAD_SHELL: &str = "/bin/sh";

const INITIALIZE_ID: u64 = 1;
const THREAD_START_ID: u64 = 2;

/// The real host. `loom_program` is the executable the boundary starts as its
/// trusted bridge (`worker-exec`); `None` is this process's own executable,
/// which is loom in the daemon and the CLI.
#[derive(Default)]
pub(super) struct RealMacHost {
    pub(super) loom_program: Option<PathBuf>,
}

impl MacHostProbes for RealMacHost {
    fn sandbox_exec(&self) -> Result<(), String> {
        match crate::process::sandbox_probe::confined_commands_blocker() {
            None => Ok(()),
            Some(blocker) => Err(classify_sandbox_exec(blocker)),
        }
    }

    fn payload_boundary(&self) -> Result<(), String> {
        let fixture = Fixture::create(self.loom_program.as_deref())?;
        let spec = fixture.spec(Path::new(PAYLOAD_SHELL))?;
        let args: Vec<OsString> = vec![
            "-c".into(),
            PAYLOAD_SCRIPT.into(),
            "sh".into(),
            fixture.worktree.clone().into(),
            fixture.host_home.clone().into(),
        ];
        let boundary = boundary::spawn_with_args(&spec, &args)
            .map_err(|error| format!("payload boundary probe: cannot start: {error:#}"))?;
        run_retired(boundary, |launched| run_payload(launched, &fixture))?;
        fixture.host_untouched()
    }

    fn codex_home_honoured(&self) -> Result<(), String> {
        let program = crate::codex::find_codex_path()
            .and_then(|path| Ok(path.canonicalize()?))
            .map_err(|error| format!("codex home probe: codex executable not found: {error:#}"))?;
        let fixture = Fixture::create(self.loom_program.as_deref())?;
        let spec = fixture.spec(&program)?;
        let boundary = boundary::spawn(&spec)
            .map_err(|error| format!("codex home probe: cannot start codex: {error:#}"))?;
        run_retired(boundary, |launched| run_codex(launched, &fixture))?;
        fixture.host_untouched()
    }
}

/// Runs `body` on `boundary`, then retires it whatever `body` said: the group
/// is killed and the private invocation root removed. A boundary that is not
/// proven released, or whose private root is still there, fails the probe.
fn run_retired(
    mut boundary: SpawnedBoundary,
    body: impl FnOnce(&mut SpawnedBoundary) -> Result<(), String>,
) -> Result<(), String> {
    let private_root = boundary
        .mac_identity()
        .map(|identity| identity.scratch_root.clone());
    let outcome = body(&mut boundary);
    let leftover = match boundary.retire() {
        Quiescence::Unverified(detail) => Some(format!("the boundary was not released: {detail}")),
        Quiescence::Released => private_root
            .filter(|root| root.exists())
            .map(|root| format!("the private root {} was left behind", root.display())),
    };
    match (outcome, leftover) {
        (outcome, None) => outcome,
        (Ok(()), Some(leftover)) => Err(format!("probe: {leftover}")),
        (Err(reason), Some(leftover)) => Err(format!("{reason}; {leftover}")),
    }
}

/// Activates the payload, collects its output and exit status, and judges both
/// together with the effects it had on the fixture.
fn run_payload(launched: &mut SpawnedBoundary, fixture: &Fixture) -> Result<(), String> {
    let deadline = Instant::now() + PAYLOAD_TIMEOUT;
    launched
        .activate()
        .map_err(|error| format!("payload boundary probe: the bridge refused: {error:#}"))?;
    let mut pipes = Pipes::default();
    let stdout = pipes
        .read_to_end(&mut launched.stdout, &mut launched.stderr, deadline)
        .map_err(|error| {
            with_stderr(
                format!("payload boundary probe: {error}"),
                &pipes.stderr_tail(),
            )
        })?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    // Unreaped: the zombie leader pins the session and group `retire` kills.
    let status = launched
        .process
        .wait_unreaped(remaining)
        .map_err(|error| format!("payload boundary probe: cannot wait on the payload: {error}"))?
        .ok_or("payload boundary probe: the payload did not exit in time")?;
    judge_payload(status.code(), &stdout)
        .map_err(|reason| with_stderr(reason, &pipes.stderr_tail()))?;
    if fixture.worktree.join("allowed").is_file() {
        return Ok(());
    }
    Err("payload boundary probe: the payload's write is not in the worktree".into())
}

/// `reason`, with the end of the process's stderr when it wrote any.
fn with_stderr(reason: String, stderr: &str) -> String {
    if stderr.is_empty() {
        return reason;
    }
    format!("{reason} (stderr: {stderr})")
}

/// Activates Codex, asks it to `initialize` and start a thread, and judges
/// where it says its home is.
fn run_codex(launched: &mut SpawnedBoundary, fixture: &Fixture) -> Result<(), String> {
    let deadline = Instant::now() + CODEX_TIMEOUT;
    let mode = launched
        .activate()
        .map_err(|error| format!("codex home probe: the bridge refused: {error:#}"))?;
    let mut rpc = Rpc::new(
        &mut launched.stdin,
        &mut launched.stdout,
        &mut launched.stderr,
        deadline,
    );
    let (initialize, thread) = converse(&mut rpc, &fixture.worktree, mode)
        .map_err(|reason| with_stderr(format!("codex home probe: {reason}"), &rpc.stderr_tail()))?;
    judge_codex_home(
        &initialize,
        &thread,
        &fixture.cache_root,
        &fixture.host_home,
    )
    .map(|_| ())
}

/// The results of `initialize` and `thread/start`, the session operation that
/// needs no network.
fn converse(rpc: &mut Rpc, cwd: &Path, mode: SandboxMode) -> Result<(Value, Value), String> {
    let initialize = wire::initialize(INITIALIZE_ID, env!("CARGO_PKG_VERSION"));
    let initialize = rpc.request(&initialize, INITIALIZE_ID)?;
    rpc.notify(&wire::initialized())?;
    let thread_start = wire::bare_thread_start(THREAD_START_ID, cwd, mode);
    let thread = rpc.request(&thread_start, THREAD_START_ID)?;
    Ok((initialize, thread))
}
