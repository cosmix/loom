//! Live mode event loop with daemon socket integration

use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::daemon::{read_message, write_message, Request, Response, StageInfo};
use crate::utils::{cleanup_terminal, install_terminal_panic_hook};

use super::activity::{ActivityLog, ActivityType};
use super::spinner::Spinner;

/// Connection timeout for daemon socket
const SOCKET_TIMEOUT: Duration = Duration::from_secs(2);

/// Live mode state
pub struct LiveMode {
    spinner: Spinner,
    activity: ActivityLog,
    running: Arc<AtomicBool>,
}

impl LiveMode {
    pub fn new() -> Self {
        Self {
            spinner: Spinner::new(),
            activity: ActivityLog::new(),
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Run live mode event loop
    pub fn run(&mut self, work_path: &Path) -> Result<()> {
        install_terminal_panic_hook();

        let socket_path = work_path.join("orchestrator.sock");
        let mut stream = self.connect(&socket_path)?;

        let running = self.running.clone();
        let stream_for_signal = stream
            .try_clone()
            .context("Failed to clone stream for signal handler")?;

        ctrlc::set_handler(move || {
            running.store(false, Ordering::SeqCst);
            let mut stream = stream_for_signal.try_clone().ok();
            if let Some(ref mut s) = stream {
                let _ = write_message(s, &Request::Unsubscribe);
            }
            cleanup_terminal();
            std::process::exit(0);
        })
        .context("Failed to set Ctrl+C handler")?;

        self.subscribe(&mut stream)?;

        self.render_header();

        while self.running.load(Ordering::SeqCst) {
            self.spinner.tick();

            match read_message(&mut stream) {
                Ok(response) => {
                    self.handle_response(response, work_path)?;
                }
                Err(e) => {
                    eprintln!("\n{}", "Connection to daemon lost".red());
                    eprintln!("{}", format!("Error: {e}").dimmed());
                    break;
                }
            }
        }

        let _ = write_message(&mut stream, &Request::Unsubscribe);
        cleanup_terminal();

        Ok(())
    }

    fn connect(&self, socket_path: &Path) -> Result<UnixStream> {
        let mut stream =
            UnixStream::connect(socket_path).context("Failed to connect to daemon socket")?;

        stream
            .set_read_timeout(Some(SOCKET_TIMEOUT))
            .context("Failed to set read timeout")?;
        stream
            .set_write_timeout(Some(SOCKET_TIMEOUT))
            .context("Failed to set write timeout")?;

        write_message(&mut stream, &Request::Ping).context("Failed to send Ping")?;

        let response: Response =
            read_message(&mut stream).context("Failed to read Ping response")?;

        match response {
            Response::Pong => {}
            Response::Error { message } => {
                anyhow::bail!("Daemon returned error: {message}");
            }
            _ => {
                anyhow::bail!("Unexpected response from daemon");
            }
        }

        stream
            .set_read_timeout(None)
            .context("Failed to clear read timeout")?;

        Ok(stream)
    }

    fn subscribe(&self, stream: &mut UnixStream) -> Result<()> {
        write_message(stream, &Request::SubscribeStatus)
            .context("Failed to send SubscribeStatus")?;

        let response: Response =
            read_message(stream).context("Failed to read subscription response")?;

        match response {
            Response::Ok => Ok(()),
            Response::Error { message } => {
                anyhow::bail!("Subscription failed: {message}");
            }
            _ => {
                anyhow::bail!("Unexpected subscription response");
            }
        }
    }

    fn render_header(&self) {
        println!(
            "\n{} {}",
            self.spinner.current(),
            "Live Status Dashboard".bold().blue()
        );
        println!("{}", "Press Ctrl+C to exit (daemon continues)".dimmed());
        println!("{}", "═".repeat(50));
    }

    fn handle_response(&mut self, response: Response, work_path: &Path) -> Result<()> {
        match response {
            Response::StatusUpdate {
                stages_executing,
                stages_pending,
                stages_completed,
                stages_blocked,
            } => {
                self.update_activity(&stages_executing, &stages_completed, &stages_blocked);
                self.render_status(
                    work_path,
                    &stages_executing,
                    &stages_pending,
                    &stages_completed,
                    &stages_blocked,
                );
            }
            Response::Error { message } => {
                eprintln!("\n{}", format!("Daemon error: {message}").red());
                self.running.store(false, Ordering::SeqCst);
            }
            _ => {}
        }
        Ok(())
    }

    fn update_activity(
        &mut self,
        executing: &[StageInfo],
        completed: &[StageInfo],
        blocked: &[StageInfo],
    ) {
        for stage in completed.iter().take(1) {
            self.activity.push(
                ActivityType::StageCompleted,
                format!("Stage {} completed", stage.id),
            );
        }
        for stage in blocked.iter().take(1) {
            self.activity.push(
                ActivityType::StageBlocked,
                format!("Stage {} blocked", stage.id),
            );
        }
        for stage in executing.iter().take(1) {
            self.activity.push(
                ActivityType::StageStarted,
                format!("Stage {} started", stage.id),
            );
        }
    }

    fn render_status(
        &self,
        _work_path: &Path,
        executing: &[StageInfo],
        pending: &[StageInfo],
        completed: &[StageInfo],
        blocked: &[StageInfo],
    ) {
        print!("\x1B[2J\x1B[1;1H");

        println!(
            "\n{} {}",
            self.spinner.current(),
            "Live Status Dashboard".bold().blue()
        );
        println!("{}", "Press Ctrl+C to exit (daemon continues)".dimmed());
        println!("{}", "═".repeat(50));

        let total = executing.len() + pending.len() + completed.len() + blocked.len();
        println!("\n{}", format!("Summary: {total} stages").bold());

        if !executing.is_empty() {
            println!(
                "\n{}",
                format!("● Executing ({})", executing.len()).blue().bold()
            );
            for stage in executing {
                let elapsed = chrono::Utc::now()
                    .signed_duration_since(stage.started_at)
                    .num_seconds();
                let time_info = format_elapsed(elapsed);
                println!(
                    "    {} {} {}",
                    stage.id.dimmed(),
                    stage.name,
                    time_info.dimmed()
                );
            }
        }

        if !pending.is_empty() {
            println!("\n{}", format!("○ Pending ({})", pending.len()).dimmed());
            for stage in pending.iter().take(5) {
                println!("    {}", stage.id.dimmed());
            }
            if pending.len() > 5 {
                println!("    ... {} more", pending.len() - 5);
            }
        }

        if !completed.is_empty() {
            println!("\n{}", format!("✓ Completed ({})", completed.len()).green());
            for stage in completed.iter().take(3) {
                println!("    {}", stage.id.dimmed());
            }
            if completed.len() > 3 {
                println!("    ... {} more", completed.len() - 3);
            }
        }

        if !blocked.is_empty() {
            println!(
                "\n{}",
                format!("✗ Blocked ({})", blocked.len()).red().bold()
            );
            for stage in blocked {
                println!("    {}", stage.id);
            }
        }

        if !self.activity.is_empty() {
            println!("\n{}", "Recent Activity".bold());
            for line in self.activity.render(5) {
                println!("  {line}");
            }
        }

        println!();
        io::stdout().flush().ok();
    }
}

impl Default for LiveMode {
    fn default() -> Self {
        Self::new()
    }
}

fn format_elapsed(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h{}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

/// Entry point for live mode
pub fn run_live_mode(work_path: &Path) -> Result<()> {
    let mut live_mode = LiveMode::new();
    live_mode.run(work_path)
}
