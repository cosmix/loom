//! One dashboard child per planning server, with holder-only bootstrap tokens.

use std::io::{self, BufRead, BufReader, Read};
use std::net::{IpAddr, SocketAddr};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;

use crate::models::session::SessionBackendKind;
use crate::plan_server::browser::EnvView;
use crate::plan_server::context::PlanContext;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::shutdown::ShutdownHook;

pub use super::dashboard_parse::parse_dashboard_startup;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const STARTUP_LINES: usize = 20;
const MAX_LINE_BYTES: u64 = 16 * 1024;
// Extensions has get/insert rather than get-or-insert; serialize installation.
static INSTALL: Mutex<()> = Mutex::new(());

#[derive(Default)]
struct Dashboard {
    state: Mutex<DashboardState>,
}

#[derive(Default)]
struct DashboardState {
    child: Option<DashboardChild>,
    stopping: bool,
}

struct DashboardChild {
    process: Child,
    url: String,
}

impl Drop for DashboardChild {
    fn drop(&mut self) {
        // Descendants may retain stdout: kill the child's entire process group.
        if let Ok(pid) = i32::try_from(self.process.id()) {
            let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
        }
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

impl Dashboard {
    fn stop(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.stopping = true;
        state.child = None;
    }

    fn url(&self, ctx: &PlanContext, loom_bin: &Path, timeout: Duration) -> io::Result<String> {
        // Hold through startup so simultaneous first requests share one child.
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.stopping || ctx.is_stopping() {
            return Err(io::Error::other("planning server is stopping"));
        }
        if let Some(child) = &state.child {
            return Ok(child.url.clone());
        }
        let config = crate::fs::work_dir::read_terminal_config(&ctx.root().join(".loom/work"))
            .map_err(io::Error::other)?;
        let child = start_dashboard(
            ctx.root(),
            loom_bin,
            ctx.bind().local.ip(),
            config.backend == SessionBackendKind::Tmux,
            timeout,
        )?;
        let url = child.url.clone();
        state.child = Some(child);
        Ok(url)
    }
}

pub(crate) fn install_dashboard(ctx: &PlanContext) {
    let _install = INSTALL.lock().unwrap_or_else(PoisonError::into_inner);
    if ctx.extensions().get::<Dashboard>().is_some() {
        return;
    }
    let dashboard = Arc::new(Dashboard::default());
    ctx.extensions().insert(Arc::clone(&dashboard));
    let drain = Arc::clone(&dashboard);
    ctx.shutdown().register(ShutdownHook {
        name: "dashboard",
        drain: Box::new(move || drain.stop()),
        abort: Box::new(move || dashboard.stop()),
    });
}

/// The child-reported URL, including its token; callers must redact broadcasts.
pub(crate) fn current_url(ctx: &PlanContext) -> Option<String> {
    let dashboard = ctx.extensions().get::<Dashboard>()?;
    let state = dashboard
        .state
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    state.child.as_ref().map(|child| child.url.clone())
}

pub(super) fn post_dashboard(req: &ApiRequest<'_>) -> ApiResponse {
    match std::env::current_exe() {
        Ok(loom_bin) => post_dashboard_with_bin(req, &loom_bin, STARTUP_TIMEOUT),
        Err(_) => unavailable(),
    }
}

fn post_dashboard_with_bin(
    req: &ApiRequest<'_>,
    loom_bin: &Path,
    timeout: Duration,
) -> ApiResponse {
    install_dashboard(req.ctx);
    let Some(dashboard) = req.ctx.extensions().get::<Dashboard>() else {
        return unavailable();
    };
    match dashboard.url(req.ctx, loom_bin, timeout) {
        Ok(url) => {
            let env = EnvView::current();
            dashboard_response(req, &url, env.ssh_connection || env.ssh_tty)
        }
        Err(_) => unavailable(),
    }
}

fn unavailable() -> ApiResponse {
    ApiResponse::error(
        502,
        "dashboard_unavailable",
        "the dashboard could not be started",
    )
}

fn dashboard_response(req: &ApiRequest<'_>, url: &str, ssh: bool) -> ApiResponse {
    let visible = if req.ctx.authoring().is_holder(req.browser) {
        url.to_owned()
    } else {
        super::dashboard_parse::strip_token(url)
    };
    let mut response = serde_json::json!({ "url": visible });
    if ssh {
        if let Some(port) = dashboard_port(url) {
            response["tunnel"] =
                crate::plan_server::startup::tunnel_hint(port, &crate::plan_server::hostname())
                    .into();
        }
    }
    ApiResponse::json(200, &response)
}

fn dashboard_port(url: &str) -> Option<u16> {
    let authority = url.strip_prefix("http://")?.split('/').next()?;
    Some(authority.parse::<SocketAddr>().ok()?.port())
}

fn start_dashboard(
    root: &Path,
    loom_bin: &Path,
    host: IpAddr,
    terminals: bool,
    timeout: Duration,
) -> io::Result<DashboardChild> {
    let deadline = Instant::now() + timeout;
    let mut command = Command::new(loom_bin);
    command.args(["status", "--web", "--host", &host.to_string()]);
    if terminals {
        command.arg("--terminals");
    }
    let process = command
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
    let mut child = DashboardChild {
        process,
        url: String::new(),
    };
    let stdout = child
        .process
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("dashboard stdout unavailable"))?;
    let (sender, receiver) = mpsc::sync_channel(STARTUP_LINES);
    thread::Builder::new()
        .name("plan-dashboard-stdout".into())
        .spawn(move || drain_stdout(stdout, sender))?;
    child.url = startup_url(&receiver, deadline)?;
    Ok(child)
}

fn startup_url(
    receiver: &mpsc::Receiver<io::Result<String>>,
    deadline: Instant,
) -> io::Result<String> {
    for _ in 0..STARTUP_LINES {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "dashboard startup timed out")
            })?;
        let line = receiver
            .recv_timeout(remaining)
            .map_err(|_| io::Error::other("dashboard startup unavailable"))??;
        if let Some(url) = parse_dashboard_startup(&[line.as_str()]) {
            return Ok(url);
        }
    }
    Err(io::Error::other("no dashboard URL in startup output"))
}

fn drain_stdout(stdout: impl Read, sender: mpsc::SyncSender<io::Result<String>>) {
    let mut reader = BufReader::new(stdout);
    for _ in 0..STARTUP_LINES {
        let line = read_startup_line(&mut reader);
        if matches!(line, Ok(None)) {
            return;
        }
        let line = line.and_then(|line| line.ok_or_else(|| io::Error::other("stdout closed")));
        let failed = line.is_err();
        if sender.send(line).is_err() || failed {
            break;
        }
    }
    drop(sender);
    // The dashboard continues logging for its lifetime; never leave its pipe full.
    let _ = io::copy(&mut reader, &mut io::sink());
}

fn read_startup_line(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut bytes = Vec::new();
    let count = reader
        .take(MAX_LINE_BYTES + 1)
        .read_until(b'\n', &mut bytes)?;
    if count == 0 {
        return Ok(None);
    }
    if bytes.len() > MAX_LINE_BYTES as usize {
        return Err(io::Error::other("dashboard startup line too long"));
    }
    String::from_utf8(bytes).map(Some).map_err(io::Error::other)
}

#[cfg(test)]
mod tests;
