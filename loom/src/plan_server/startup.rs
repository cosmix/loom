//! Terminal output for planning server startup.

use std::path::Path;

use crate::context::untrusted::inline_safe;
use crate::plan_server::lock::LockRecord;
use crate::plan_server::wire::server::MissingPrereq;

pub struct Startup<'a> {
    pub root: &'a Path,
    pub urls: Vec<String>,
    pub port: u16,
    pub remote: bool,
    pub ssh: bool,
    pub hostname: String,
    pub read_only: &'a [MissingPrereq],
}

pub fn tunnel_hint(port: u16, hostname: &str) -> String {
    format!("ssh -L {port}:127.0.0.1:{port} {}", inline_safe(hostname))
}

pub fn startup_lines(s: &Startup<'_>) -> Vec<String> {
    let mut lines = vec![format!(
        "loom plan: {}",
        inline_safe(&s.root.to_string_lossy())
    )];
    lines.extend(s.urls.iter().cloned());
    if s.ssh {
        lines.push(format!(
            "On your machine: {}",
            tunnel_hint(s.port, &s.hostname)
        ));
    }
    if s.remote {
        lines.push("warning: plain HTTP; anyone who can see the traffic can read the token, then read every document and write through sessions. An SSH tunnel is the safer way to share.".into());
    }
    lines.extend(read_only_lines(s.read_only));
    lines.push("Ctrl-C to stop; a second Ctrl-C exits at once".into());
    lines
}

/// One line per missing prerequisite.
pub fn read_only_lines(problems: &[MissingPrereq]) -> Vec<String> {
    problems
        .iter()
        .map(|problem| format!("read-only: {}. {}", problem.message, problem.fix))
        .collect()
}

pub fn already_running_lines(record: &LockRecord) -> Vec<String> {
    let mut lines = vec![format!(
        "loom plan is already running for this project (pid {}).",
        record.pid
    )];
    match (&record.url, &record.token) {
        (Some(url), Some(token)) => {
            let url = crate::context::untrusted::inline_safe(url.trim_end_matches('/'));
            let token = crate::context::untrusted::inline_safe(token);
            lines.push(format!("Open {url}/?token={token}"));
        }
        _ => lines.push("It is still starting; run loom plan again in a moment.".into()),
    }
    lines
}
