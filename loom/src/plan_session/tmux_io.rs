//! Keyboard input into a planning session pane: bracketed paste of text and named key presses.
//!
//! Every command runs on the session's private server (`-L <socket>`), is bounded, and counts
//! stderr with exit 0 as a failure (`mistakes/tmux-backend.md`: tmux reports some failures that
//! way). `load-buffer -w` is never used: it reaches the clipboard and crashes tmux 3.6a while a
//! client is attached.

use std::io::Write;
use std::process::{Output, Stdio};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};

use crate::orchestrator::terminal::tmux::tmux_control_command;
use crate::process::run_bounded_output;

/// The bound on one tmux command.
const TMUX_IO_TIMEOUT: Duration = Duration::from_secs(5);
/// The named paste buffer; `paste-buffer -d` deletes it again.
const INPUT_BUFFER: &str = "loom-input";

/// Paste `text` into `target` as one bracketed paste and submit it with Enter.
///
/// The caller sanitised `text` (`plan_session::input::sanitize`); a C0 control other than a
/// newline or a tab would reach the pane as a keystroke.
pub fn paste_text(socket: &str, target: &str, text: &str) -> Result<()> {
    debug_assert!(
        !text
            .chars()
            .any(|c| c.is_ascii_control() && c != '\x7f' && c != '\n' && c != '\t'),
        "paste_text received an unsanitised C0 control character"
    );
    load_buffer(socket, text)?;
    let paste = ["paste-buffer", "-p", "-d", "-b", INPUT_BUFFER, "-t", target];
    run(socket, &paste)?;
    press(socket, target, "Enter")
}

/// Press the named key (`Escape`, `Enter`, ...) once in `target`.
pub fn press(socket: &str, target: &str, key: &str) -> Result<()> {
    run(socket, &["send-keys", "-t", target, key]).map(drop)
}

/// The pid of the process in `target`'s pane; an error when that process already exited
/// (the stage servers keep `remain-on-exit on`, so a dead pane still reports its old pid).
pub fn pane_pid(socket: &str, target: &str) -> Result<u32> {
    let format = "#{pane_pid}:#{pane_dead}:#{pane_dead_status}";
    let output = run(socket, &["display-message", "-p", "-t", target, format])?;
    let line = String::from_utf8_lossy(&output.stdout);
    parse_pane(line.trim()).with_context(|| format!("tmux pane of '{target}' on socket '{socket}'"))
}

/// Parse `<pid>:<dead>:<status>` as `display-message` printed it.
fn parse_pane(line: &str) -> Result<u32> {
    let mut fields = line.splitn(3, ':');
    let (pid, dead, status) = (fields.next(), fields.next(), fields.next());
    if dead == Some("1") {
        let status = status.filter(|s| !s.is_empty()).unwrap_or("unknown");
        bail!("the pane's process already exited (status {status})");
    }
    let pid = pid.unwrap_or_default();
    match pid.parse::<u32>() {
        Ok(pid) if pid > 0 => Ok(pid),
        _ => bail!("tmux printed no pane pid: {line:?}"),
    }
}

/// `load-buffer -b loom-input -` with `text` on stdin.
///
/// The text (up to 100 KB) exceeds a pipe buffer, so a feeder thread writes it while tmux
/// reads. The parent's read end lives in `command`: dropping `command` before the join lets a
/// tmux that stopped reading early fail the feeder's write instead of blocking it forever.
fn load_buffer(socket: &str, text: &str) -> Result<()> {
    let (reader, mut writer) = std::io::pipe().context("cannot create the paste pipe")?;
    let args = ["-L", socket, "load-buffer", "-b", INPUT_BUFFER, "-"];
    let mut command = tmux_control_command(&args);
    command.stdin(Stdio::from(reader));
    let bytes = text.as_bytes().to_vec();
    let feeder = std::thread::Builder::new()
        .name("loom-paste-feeder".to_owned())
        .spawn(move || writer.write_all(&bytes))
        .context("cannot start the paste feeder thread")?;
    let operation = format!("tmux load-buffer ({socket})");
    let output = run_bounded_output(&mut command, TMUX_IO_TIMEOUT, operation.clone());
    drop(command);
    let fed = feeder
        .join()
        .map_err(|_| anyhow!("the paste feeder thread panicked"))?;
    check(&operation, output.with_context(|| operation.clone())?)?;
    fed.with_context(|| format!("{operation}: cannot write the text to tmux"))
}

/// Run `tmux -L <socket> <args>` under the bound and require a clean result.
fn run(socket: &str, args: &[&str]) -> Result<Output> {
    let mut full: Vec<&str> = vec!["-L", socket];
    full.extend_from_slice(args);
    let mut command = tmux_control_command(&full);
    let verb = args.first().copied().unwrap_or("tmux");
    let operation = format!("tmux {verb} ({socket})");
    let output = run_bounded_output(&mut command, TMUX_IO_TIMEOUT, operation.clone())
        .with_context(|| operation.clone())?;
    check(&operation, output)
}

/// A non-zero exit or any stderr is a failure, even with exit 0.
fn check(operation: &str, output: Output) -> Result<Output> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() || !stderr.trim().is_empty() {
        bail!("{operation} failed ({}): {}", output.status, stderr.trim());
    }
    Ok(output)
}

#[cfg(test)]
#[path = "tmux_io_tests.rs"]
pub(crate) mod tests;
