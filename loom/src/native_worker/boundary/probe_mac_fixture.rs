//! The hermetic scratch a macOS probe launches in, and bounded reads of what
//! the launched process says. Nothing here names the operator's home, Codex
//! home or credentials: every path lives in one scratch directory.
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{ChildStderr, ChildStdin, ChildStdout};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use serde_json::Value;

use super::judge::{response_to, take_line, CODEX_HOME_ENTRIES, HOST_HOME_ENTRIES};
use crate::native_worker::boundary::BoundarySpec;
use crate::stage_cache::StageCache;

/// The stage the fixture's cache is resolved for.
const FIXTURE_STAGE: &str = "native-qualify";

/// A fixture credential: a file Codex can read, not a credential.
const FIXTURE_AUTH: &str = "{\"OPENAI_API_KEY\":\"loom-qualification-fixture\"}\n";

/// How much of a process's stderr a refusal quotes.
const STDERR_TAIL_BYTES: usize = 2000;

/// How long one poll waits before the deadline is looked at again.
const POLL_SLICE: Duration = Duration::from_millis(100);

/// One probe's scratch: a worktree, a host home beside a host Codex home
/// holding a fixture auth file, a stage cache, and a bound proxy socket.
pub(super) struct Fixture {
    pub(super) worktree: PathBuf,
    /// Outside every root the boundary grants; holds a `secret` file.
    pub(super) host_home: PathBuf,
    /// `<host_home>/.codex`, holding only the fixture `auth.json`.
    pub(super) codex_home: PathBuf,
    /// The canonical stage cache root, below which the launch makes its private
    /// invocation root.
    pub(super) cache_root: PathBuf,
    cache: StageCache,
    proxy_socket: PathBuf,
    loom_program: PathBuf,
    /// Held so the proxy socket path exists for the probe's life.
    _proxy: UnixListener,
    /// Removed last, after every other field is dropped.
    _scratch: tempfile::TempDir,
}

fn private_dir(path: &Path) -> Result<(), String> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|error| format!("probe scratch {}: {error}", path.display()))
}

fn write_file(path: &Path, body: &str) -> Result<(), String> {
    std::fs::write(path, body).map_err(|error| format!("probe scratch {}: {error}", path.display()))
}

impl Fixture {
    /// A scratch whose boundary runs `loom_program` as its bridge, else this
    /// process's own executable.
    pub(super) fn create(loom_program: Option<&Path>) -> Result<Self, String> {
        let scratch = tempfile::Builder::new()
            .prefix("loom-probe-")
            .tempdir()
            .map_err(|error| format!("probe scratch directory: {error}"))?;
        let root = scratch
            .path()
            .canonicalize()
            .map_err(|error| format!("probe scratch directory: {error}"))?;
        let (worktree, host_home) = (root.join("worktree"), root.join("home"));
        let codex_home = host_home.join(".codex");
        let socket_dir = root.join("proxy");
        for dir in [&worktree, &codex_home, &socket_dir] {
            private_dir(dir)?;
        }
        write_file(&codex_home.join("auth.json"), FIXTURE_AUTH)?;
        write_file(&host_home.join("secret"), "not for the boundary\n")?;
        let proxy_socket = socket_dir.join("p.sock");
        let proxy = UnixListener::bind(&proxy_socket)
            .map_err(|error| format!("probe proxy socket {}: {error}", proxy_socket.display()))?;
        let cache = StageCache::resolve(
            &root.join("repo"),
            FIXTURE_STAGE,
            &root,
            Some(&root.join("cache")),
        )
        .map_err(|error| format!("probe stage cache: {error:#}"))?;
        let cache_root = cache
            .ensure()
            .map_err(|error| format!("probe stage cache: {error:#}"))?;
        let loom_program = match loom_program {
            Some(path) => path.to_path_buf(),
            None => super::super::loom_program::current()
                .map_err(|error| format!("cannot resolve the loom executable: {error}"))?,
        };
        Ok(Self {
            worktree,
            host_home,
            codex_home,
            cache: cache.at_root(cache_root.clone()),
            cache_root,
            proxy_socket,
            loom_program,
            _proxy: proxy,
            _scratch: scratch,
        })
    }

    /// The production boundary request running `program`, under a fresh
    /// invocation id.
    pub(super) fn spec(&self, program: &Path) -> Result<BoundarySpec, String> {
        let invocation = crate::native_worker::ids::mint_invocation_id()
            .map_err(|error| format!("cannot mint an invocation id: {error}"))?;
        Ok(BoundarySpec {
            worktree: self.worktree.clone(),
            codex_home: self.codex_home.clone(),
            codex_program: program.to_path_buf(),
            proxy_socket: self.proxy_socket.clone(),
            loom_program: self.loom_program.clone(),
            stage_cache: Some(self.cache.clone()),
            invocation,
        })
    }

    /// `Ok` when the boundary left the fixture host home and its Codex home as
    /// they were made.
    pub(super) fn host_untouched(&self) -> Result<(), String> {
        super::judge::judge_untouched(&self.host_home, &HOST_HOME_ENTRIES)?;
        super::judge::judge_untouched(&self.codex_home, &CODEX_HOME_ENTRIES)
    }
}

/// A launched process's stdout as lines and its stderr as a bounded tail, read
/// without ever blocking past a deadline.
#[derive(Default)]
pub(super) struct Pipes {
    out: Vec<u8>,
    out_closed: bool,
    err: Vec<u8>,
    err_closed: bool,
}

impl Pipes {
    /// The next complete stdout line, `None` at end of output; an error at
    /// `deadline`.
    pub(super) fn next_line(
        &mut self,
        stdout: &mut ChildStdout,
        stderr: &mut ChildStderr,
        deadline: Instant,
    ) -> Result<Option<String>, String> {
        loop {
            if let Some(line) = take_line(&mut self.out) {
                return Ok(Some(line));
            }
            if self.out_closed {
                return Ok(None);
            }
            self.pump(stdout, stderr, deadline)?;
        }
    }

    /// All stdout up to its end, or an error at `deadline`.
    pub(super) fn read_to_end(
        &mut self,
        stdout: &mut ChildStdout,
        stderr: &mut ChildStderr,
        deadline: Instant,
    ) -> Result<String, String> {
        while !self.out_closed {
            self.pump(stdout, stderr, deadline)?;
        }
        Ok(String::from_utf8_lossy(&std::mem::take(&mut self.out)).into_owned())
    }

    /// The end of what the process wrote to stderr.
    pub(super) fn stderr_tail(&self) -> String {
        String::from_utf8_lossy(&self.err).trim().to_string()
    }

    /// Waits for either open pipe to become readable and moves what it holds.
    fn pump(
        &mut self,
        stdout: &mut ChildStdout,
        stderr: &mut ChildStderr,
        deadline: Instant,
    ) -> Result<(), String> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("the probe's process did not answer in time".to_string());
        }
        let slice = u16::try_from(remaining.min(POLL_SLICE).as_millis()).unwrap_or(100);
        let mut fds = Vec::new();
        let (mut out_at, mut err_at) = (None, None);
        if !self.out_closed {
            out_at = Some(fds.len());
            fds.push(PollFd::new(stdout.as_fd(), PollFlags::POLLIN));
        }
        if !self.err_closed {
            err_at = Some(fds.len());
            fds.push(PollFd::new(stderr.as_fd(), PollFlags::POLLIN));
        }
        match poll(&mut fds, PollTimeout::from(slice)) {
            Ok(_) | Err(Errno::EINTR) => {}
            Err(errno) => return Err(format!("cannot wait on the probe's process: {errno}")),
        }
        let ready = |at: Option<usize>| {
            at.and_then(|index| fds[index].revents())
                .is_some_and(|events| !events.is_empty())
        };
        let (out_ready, err_ready) = (ready(out_at), ready(err_at));
        if out_ready {
            self.out_closed = fill(stdout, &mut self.out)?;
        }
        if err_ready {
            self.err_closed = fill(stderr, &mut self.err)?;
            let excess = self.err.len().saturating_sub(STDERR_TAIL_BYTES);
            self.err.drain(..excess);
        }
        Ok(())
    }
}

/// One read of a readable pipe into `buffer`; true at end of input.
fn fill(pipe: &mut impl Read, buffer: &mut Vec<u8>) -> Result<bool, String> {
    let mut chunk = [0u8; 4096];
    match pipe.read(&mut chunk) {
        Ok(0) => Ok(true),
        Ok(read) => {
            buffer.extend_from_slice(&chunk[..read]);
            Ok(false)
        }
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => Ok(false),
        Err(error) => Err(format!("cannot read the probe's process: {error}")),
    }
}

/// Writes `line` and a newline to the process's stdin.
fn send(stdin: &mut impl Write, line: &str) -> Result<(), String> {
    stdin
        .write_all(line.as_bytes())
        .and_then(|()| stdin.write_all(b"\n"))
        .and_then(|()| stdin.flush())
        .map_err(|error| format!("cannot write to the probe's process: {error}"))
}

/// A JSON-RPC conversation with a launched app-server, bounded by one deadline.
pub(super) struct Rpc<'a> {
    stdin: &'a mut ChildStdin,
    stdout: &'a mut ChildStdout,
    stderr: &'a mut ChildStderr,
    pipes: Pipes,
    deadline: Instant,
}

impl<'a> Rpc<'a> {
    pub(super) fn new(
        stdin: &'a mut ChildStdin,
        stdout: &'a mut ChildStdout,
        stderr: &'a mut ChildStderr,
        deadline: Instant,
    ) -> Self {
        Self {
            stdin,
            stdout,
            stderr,
            pipes: Pipes::default(),
            deadline,
        }
    }

    /// Sends a frame that expects no response.
    pub(super) fn notify(&mut self, frame: &Value) -> Result<(), String> {
        send(self.stdin, &frame.to_string())
    }

    /// Sends `frame` and returns the result of the response to request `id`;
    /// notifications and other frames in between are skipped.
    pub(super) fn request(&mut self, frame: &Value, id: u64) -> Result<Value, String> {
        send(self.stdin, &frame.to_string())?;
        loop {
            let line = self
                .pipes
                .next_line(self.stdout, self.stderr, self.deadline)?
                .ok_or("the app-server exited before it answered")?;
            if let Some(response) = response_to(&line, id) {
                return response.map_err(|message| format!("the app-server refused: {message}"));
            }
        }
    }

    /// The end of what the app-server wrote to stderr.
    pub(super) fn stderr_tail(&self) -> String {
        self.pipes.stderr_tail()
    }
}
