//! The verdicts of the macOS qualification probes, free of processes and
//! sandboxes: what the probes read back is judged here so every platform's
//! tests exercise the decisions.
use std::path::{Path, PathBuf};

use serde_json::Value;

/// What an operator inside a sandbox is told: Seatbelt profiles do not nest.
pub(super) const SANDBOX_REFUSAL: &str =
    "loom runs inside a sandbox; start the daemon from a plain shell";

/// The line the payload prints when every check held.
pub(super) const PAYLOAD_OK: &str = "loom-payload-ok";

/// The payload, run by `/bin/sh` through the production launch: `$1` is the
/// fixture worktree, `$2` a fixture host home outside every root the boundary
/// grants. Only shell builtins run, so the verdict does not depend on which
/// system binaries the profile exposes. The exit codes are read by
/// [`judge_payload`].
pub(super) const PAYLOAD_SCRIPT: &str = r#"printf ok > "$1/allowed" || exit 21
( : > "$2/denied-write" ) 2>/dev/null && exit 22
( IFS= read -r _line < "$2/secret" ) 2>/dev/null && exit 23
printf '%s\n' loom-payload-ok"#;

/// The names the fixture host home holds before the probe and must hold after:
/// the fixture Codex home (`.codex`, as `$HOME/.codex` is) and a secret file.
#[cfg(target_os = "macos")]
pub(super) const HOST_HOME_ENTRIES: [&str; 2] = [".codex", "secret"];

/// The names the fixture host Codex home holds before and after: only the
/// fixture auth file; Codex must keep its state in the controlled home.
#[cfg(target_os = "macos")]
pub(super) const CODEX_HOME_ENTRIES: [&str; 1] = ["auth.json"];

/// The refusal for the `sandbox-exec` probe: a nested-sandbox failure (exit 71,
/// `sandbox_apply: Operation not permitted`) names the operator's remedy.
pub(super) fn classify_sandbox_exec(blocker: &str) -> String {
    let nested = blocker.contains("exit status: 71") || blocker.contains("sandbox_apply");
    if nested {
        format!("sandbox-exec probe: {SANDBOX_REFUSAL} ({blocker})")
    } else {
        format!("sandbox-exec probe: {blocker}")
    }
}

/// `Ok` when the payload exited 0 and printed [`PAYLOAD_OK`] last.
pub(super) fn judge_payload(code: Option<i32>, stdout: &str) -> Result<(), String> {
    let reason = match code {
        Some(0) if stdout.lines().last().map(str::trim) == Some(PAYLOAD_OK) => return Ok(()),
        Some(0) => format!(
            "the payload did not report success (stdout: {})",
            tail(stdout)
        ),
        Some(21) => "the worktree is not writable inside the boundary".to_string(),
        Some(22) => "a write outside the boundary's roots was not denied".to_string(),
        Some(23) => "a read outside the boundary's roots was not denied".to_string(),
        Some(code) => format!("the payload exited {code} (stdout: {})", tail(stdout)),
        None => "the payload was killed by a signal".to_string(),
    };
    Err(format!("payload boundary probe: {reason}"))
}

/// The start of `text`, trimmed, for a refusal.
fn tail(text: &str) -> String {
    let text = text.trim();
    match text.char_indices().nth(200) {
        Some((end, _)) => format!("{}...", &text[..end]),
        None => text.to_string(),
    }
}

/// The result of the JSON-RPC response to request `id` in `line`: `None` for
/// any other frame (a notification, another request's response, not JSON).
pub(super) fn response_to(line: &str, id: u64) -> Option<Result<Value, String>> {
    let frame: Value = serde_json::from_str(line).ok()?;
    if frame.get("id")?.as_u64()? != id {
        return None;
    }
    if let Some(error) = frame.get("error") {
        let message = error.get("message").and_then(Value::as_str);
        return Some(Err(message.unwrap_or("an error without a message").into()));
    }
    Some(Ok(frame.get("result").cloned().unwrap_or(Value::Null)))
}

fn resolved(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn string_at<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer)?.as_str()
}

/// The home Codex reported when it honours the controlled `CODEX_HOME`.
///
/// `initialize` and `thread` are the results of the app-server's `initialize`
/// and `thread/start`. Codex must report a home below `cache_root` (the
/// fixture stage cache, where the launch makes the private home) and not
/// below `host_home`, and the thread's rollout path must lie in that home's
/// `sessions/`. Returns the reported home.
pub(super) fn judge_codex_home(
    initialize: &Value,
    thread: &Value,
    cache_root: &Path,
    host_home: &Path,
) -> Result<PathBuf, String> {
    let reported = string_at(initialize, "/codexHome")
        .ok_or("codex home probe: initialize did not report a codexHome")?;
    let home = resolved(Path::new(reported));
    if home.starts_with(resolved(host_home)) {
        return Err(format!(
            "codex home probe: codex reported a home inside the host home ({reported})"
        ));
    }
    if !home.starts_with(resolved(cache_root)) {
        return Err(format!(
            "codex home probe: codex reported {reported}, not the controlled CODEX_HOME below the \
             stage cache"
        ));
    }
    let rollout = string_at(thread, "/thread/path")
        .ok_or("codex home probe: thread/start reported no session path")?;
    if !resolved(Path::new(rollout)).starts_with(home.join("sessions")) {
        return Err(format!(
            "codex home probe: the session path {rollout} is not under the controlled home"
        ));
    }
    Ok(home)
}

/// `Ok` when `dir` holds exactly the `expected` entry names: the probe's
/// fixture host home is untouched.
pub(super) fn judge_untouched(dir: &Path, expected: &[&str]) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    if names == expected {
        return Ok(());
    }
    Err(format!(
        "the boundary touched the host home {}: it holds {names:?}, expected {expected:?}",
        dir.display()
    ))
}

/// One complete line off the front of `buffer`, without its newline.
pub(super) fn take_line(buffer: &mut Vec<u8>) -> Option<String> {
    let end = buffer.iter().position(|byte| *byte == b'\n')?;
    let line: Vec<u8> = buffer.drain(..=end).collect();
    Some(String::from_utf8_lossy(&line[..end]).into_owned())
}
