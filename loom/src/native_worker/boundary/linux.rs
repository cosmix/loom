//! Bubblewrap argument list, environment and process start of one boundary.
//!
//! The order of the mount arguments is the policy: a later mount sits on top
//! of an earlier one. `--remount-ro /` is not recursive, so every bind keeps
//! the mode it was given.
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use super::mounts::{self, codex_package, CodexPackage};
use super::{BoundarySpec, SpawnedBoundary, BOUNDARY_TMPDIR};
use crate::process::{spawn_retrying_text_busy, OwnedProcess};
use crate::verify::criteria::{
    bind_read_roots, bubblewrap, read_roots, CargoScratch, NetworkAccess,
};

/// Where the launcher is mounted inside the boundary.
const LOOM_BIN: &str = "/loom-bin/loom";
/// Where the proxy socket directory is mounted inside the boundary: on the
/// read-only root, so no sandboxed command can rename it or an ancestor and
/// plant its own socket in its place.
const PROXY_DIR: &str = "/loom-proxy";
/// How long bubblewrap may take to report its namespace.
const INFO_TIMEOUT: Duration = Duration::from_secs(5);
/// Bound on the stderr kept from a boundary that failed to start.
const STDERR_TAIL_BYTES: u64 = 2048;

/// The real boundary's launcher and host proxy socket directory.
struct Launch {
    loom_program: PathBuf,
    proxy_dir: PathBuf,
}

/// One boundary to start: what it exposes and what it runs.
pub(super) struct Plan {
    /// Canonical worktree.
    pub(super) worktree: PathBuf,
    /// Private Codex home mounts (and the `W/.claude` bind) when set.
    pub(super) codex_home: Option<PathBuf>,
    package: Option<CodexPackage>,
    launch: Option<Launch>,
    pub(super) program: OsString,
    pub(super) args: Vec<OsString>,
}

impl Plan {
    /// `codex app-server` behind the in-boundary launcher.
    pub(super) fn real(spec: &BoundarySpec, host: &[(OsString, OsString)]) -> Result<Self> {
        let worktree = canonical_worktree(&spec.worktree)?;
        let package = codex_package(&spec.codex_program)?;
        let home = lookup(host, "HOME").map(PathBuf::from);
        let exposed = [Some(&worktree), Some(&spec.codex_home), home.as_ref()];
        mounts::refuse_wide_package(&package.root, exposed.into_iter().flatten())?;
        let proxy_dir = spec
            .proxy_socket
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .context("the proxy socket path has no parent directory")?
            .to_path_buf();
        let socket_name = spec
            .proxy_socket
            .file_name()
            .context("the proxy socket path has no file name")?;
        let mut args: Vec<OsString> = ["subagents", "worker-exec", "--proxy-socket"]
            .map(OsString::from)
            .into();
        args.extend([Path::new(PROXY_DIR).join(socket_name).into(), "--".into()]);
        args.push(package.program.clone().into());
        args.extend(["app-server", "--listen", "stdio://"].map(OsString::from));
        Ok(Self {
            worktree,
            codex_home: Some(spec.codex_home.clone()),
            package: Some(package),
            launch: Some(Launch {
                loom_program: spec.loom_program.clone(),
                proxy_dir,
            }),
            program: LOOM_BIN.into(),
            args,
        })
    }

    /// `program` directly, optionally with the private Codex home mounts.
    pub(super) fn payload(
        worktree: &Path,
        codex_home: Option<&Path>,
        program: &Path,
        args: &[OsString],
    ) -> Result<Self> {
        Ok(Self {
            worktree: canonical_worktree(worktree)?,
            codex_home: codex_home.map(Path::to_path_buf),
            package: None,
            launch: None,
            program: program.into(),
            args: args.to_vec(),
        })
    }
}

fn canonical_worktree(worktree: &Path) -> Result<PathBuf> {
    let canonical = worktree
        .canonicalize()
        .with_context(|| format!("resolve the worktree {}", worktree.display()))?;
    anyhow::ensure!(
        canonical.is_dir(),
        "the worktree {} is not a directory",
        canonical.display()
    );
    Ok(canonical)
}

fn lookup<'a>(host: &'a [(OsString, OsString)], key: &str) -> Option<&'a OsStr> {
    host.iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_os_str())
}

/// The boundary's whole environment, built from scratch: nothing of `host`
/// is inherited except `HOME`, `RUSTUP_HOME` and `CARGO_HOME`. `launch` marks
/// the real boundary; `codex_path` is the package's helper directory.
pub(super) fn environment(
    host: &[(OsString, OsString)],
    launch: bool,
    codex_path: Option<&Path>,
) -> Vec<(OsString, OsString)> {
    let mut path: Vec<PathBuf> = Vec::new();
    if launch {
        path.push("/loom-bin".into());
        path.extend(codex_path.map(Path::to_path_buf));
    }
    // Workers compile: the toolchain proxies live in the Cargo home.
    path.extend(host_cargo_home(host).map(|cargo| cargo.join("bin")));
    path.extend(["/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    // An entry holding the separator cannot be expressed in PATH.
    let path = path
        .into_iter()
        .filter(|entry| !entry.as_os_str().as_encoded_bytes().contains(&b':'));
    let path = std::env::join_paths(path).unwrap_or_else(|_| "/usr/bin:/bin".into());
    let mut env: Vec<(OsString, OsString)> = vec![("PATH".into(), path)];
    if let Some(home) = lookup(host, "HOME") {
        env.push(("HOME".into(), home.to_os_string()));
    }
    let fixed = [
        ("LANG", "C.UTF-8"),
        ("TERM", "dumb"),
        ("TMPDIR", BOUNDARY_TMPDIR),
        ("TMP", BOUNDARY_TMPDIR),
        ("TEMP", BOUNDARY_TMPDIR),
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("LOOM_CODEX_FORWARD", "1"),
    ];
    env.extend(fixed.map(|(key, value)| (key.into(), value.into())));
    if launch {
        env.push(("LOOM_BIN".into(), LOOM_BIN.into()));
    }
    for key in ["RUSTUP_HOME", "CARGO_HOME"] {
        if let Some(value) = lookup(host, key) {
            env.push((key.into(), value.to_os_string()));
        }
    }
    env
}

/// The host Cargo home: `CARGO_HOME`, else `$HOME/.cargo`.
fn host_cargo_home(host: &[(OsString, OsString)]) -> Option<PathBuf> {
    lookup(host, "CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| lookup(host, "HOME").map(|home| Path::new(home).join(".cargo")))
}

/// The environment `CargoScratch::prepare` reads: `env` without `HOME`, the
/// host Cargo home named. Without `HOME` it probes no uv or bun cache for an
/// overlay; the boundary mounts none (its `TMPDIR` is private and no cache
/// variable points into the scratch), so that probe would be wasted work.
pub(super) fn scratch_probe(
    program: &OsStr,
    env: &[(OsString, OsString)],
    host: &[(OsString, OsString)],
) -> Command {
    let mut probe = Command::new(program);
    let kept = env.iter().filter(|(key, _)| key != "HOME");
    probe.env_clear().envs(kept.map(|(k, v)| (k, v)));
    if let Some(cargo) = host_cargo_home(host) {
        probe.env("CARGO_HOME", cargo);
    }
    probe
}

/// The bubblewrap command for `plan`, reporting its namespace on `info_fd`,
/// and the private Cargo home it mounts (which must outlive the boundary).
pub(super) fn command(
    plan: &Plan,
    host: &[(OsString, OsString)],
    info_fd: i32,
) -> Result<(Command, CargoScratch)> {
    let codex_path = plan
        .package
        .as_ref()
        .and_then(|package| package.helper_dir.as_deref());
    let env = environment(host, plan.launch.is_some(), codex_path);
    // `read_roots` and the Cargo scratch read the environment from a command.
    let mut probe = Command::new(&plan.program);
    probe.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
    let mut command = bubblewrap(NetworkAccess::Denied)?;
    // The only writable temporary directory, on the private `/tmp` tmpfs.
    // `/tmp` itself stays outside the Codex turn policy's writable roots, so a
    // sandboxed command cannot rename anything bound below it.
    command.args(["--perms", "0700", "--dir", BOUNDARY_TMPDIR]);
    command.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
    let scratch_env = scratch_probe(&plan.program, &env, host);
    let scratch = CargoScratch::prepare(&scratch_env, &plan.worktree, true)?;
    command.arg("--info-fd").arg(info_fd.to_string());
    bind_read_roots(&mut command, &probe, &scratch);
    if let Some(home) = scratch.linked_cargo_home() {
        command.arg("--bind").arg(home).arg(home);
        command.env("CARGO_HOME", home);
    }
    mounts::worktree(&mut command, &plan.worktree, plan.codex_home.is_some())?;
    if let Some(codex_home) = &plan.codex_home {
        let roots = read_roots(&probe);
        let package_root = plan
            .package
            .as_ref()
            .filter(|package| !roots.iter().any(|root| package.program.starts_with(root)))
            .map(|package| package.root.as_path());
        mounts::codex_home(&mut command, codex_home, package_root)?;
    }
    if let Some(launch) = &plan.launch {
        bind_launch(&mut command, &launch.loom_program, &launch.proxy_dir);
    }
    command.arg("--chdir").arg(&plan.worktree);
    command.args(["--remount-ro", "/", "--"]);
    command.arg(&plan.program).args(&plan.args);
    Ok((command, scratch))
}

/// The launcher and the proxy socket directory, read-only at fixed paths on the
/// read-only root, so no sandboxed command can rename or replace them.
fn bind_launch(
    command: &mut Command,
    loom_program: impl AsRef<OsStr>,
    proxy_dir: impl AsRef<OsStr>,
) {
    command.arg("--ro-bind").arg(loom_program).arg(LOOM_BIN);
    command.arg("--ro-bind").arg(proxy_dir).arg(PROXY_DIR);
}

/// A start that failed after bubblewrap was spawned. Retiring the outer
/// process group does not reach the namespace's first process (it runs in
/// its own session), and without the namespace identity nothing can prove it
/// gone: the caller must not record the boundary as released.
#[derive(Debug)]
pub(crate) struct StartedThenFailed(pub(super) String);

impl std::fmt::Display for StartedThenFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bubblewrap was started, then failed: {}", self.0)
    }
}

impl std::error::Error for StartedThenFailed {}

/// Start `plan` from the calling thread (`--die-with-parent` follows the
/// spawning thread). An `Err` before bubblewrap is spawned leaves no process;
/// one after it is a [`StartedThenFailed`].
pub(super) fn start(plan: &Plan, host: &[(OsString, OsString)]) -> Result<SpawnedBoundary> {
    let (reader, writer) = std::io::pipe().context("create the boundary info pipe")?;
    let info_fd = writer.as_raw_fd();
    let (mut command, scratch) = command(plan, host, info_fd)?;
    scratch.populate()?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    inherit_fd(&mut command, info_fd);
    let mut child = spawn_retrying_text_busy(&mut command).context("start bubblewrap")?;
    drop(writer);
    let pipes = (child.stdin.take(), child.stdout.take(), child.stderr.take());
    let mut process = OwnedProcess::new(child);
    let boundary_pid = process.child().id() as i32;
    let info = receive_info(reader);
    let (Some(stdin), Some(stdout), Some(stderr)) = pipes else {
        process.retire();
        let detail = "bubblewrap started without its standard streams".to_string();
        return Err(StartedThenFailed(detail).into());
    };
    match info {
        Ok(pid_namespace) => Ok(SpawnedBoundary {
            process,
            stdin,
            stdout,
            stderr,
            boundary_pid,
            pid_namespace,
            _cargo_home: scratch,
        }),
        Err(error) => {
            process.retire();
            drop(stdin);
            let tail = stderr_tail(stderr);
            let detail = format!("{error:#}; bubblewrap stderr: {}", tail.trim());
            Err(StartedThenFailed(detail).into())
        }
    }
}

/// Keep `fd` open across exec in the child of `command`.
fn inherit_fd(command: &mut Command, fd: i32) {
    // SAFETY: `fcntl` is async-signal-safe; it only clears close-on-exec on
    // the inherited write end so bubblewrap can report on it.
    unsafe {
        command.pre_exec(move || {
            if libc::fcntl(fd, libc::F_SETFD, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// The pid-namespace inode from bubblewrap's info JSON, read on a helper
/// thread for at most [`INFO_TIMEOUT`].
fn receive_info(reader: std::io::PipeReader) -> Result<u64> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(parse_info(reader));
    });
    receiver
        .recv_timeout(INFO_TIMEOUT)
        .map_err(|_| anyhow!("bubblewrap reported no pid namespace within 5 s"))?
}

/// The first JSON value on `reader`: its `pid-namespace`, once `child-pid`
/// shows the namespace's first process started.
fn parse_info(reader: impl Read) -> Result<u64> {
    let mut values = serde_json::Deserializer::from_reader(reader.take(64 * 1024))
        .into_iter::<serde_json::Value>();
    let info = values
        .next()
        .context("bubblewrap exited before reporting its pid namespace")?
        .context("parse the bubblewrap info JSON")?;
    let field = |name: &str| info.get(name).and_then(serde_json::Value::as_u64);
    field("child-pid").context("bubblewrap info names no child-pid")?;
    field("pid-namespace").context("bubblewrap info names no pid-namespace")
}

/// At most [`STDERR_TAIL_BYTES`] of a retired boundary's stderr, waiting at
/// most one second.
fn stderr_tail(stderr: std::process::ChildStderr) -> String {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.take(STDERR_TAIL_BYTES).read_to_end(&mut bytes);
        let _ = sender.send(String::from_utf8_lossy(&bytes).into_owned());
    });
    receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap_or_default()
}
