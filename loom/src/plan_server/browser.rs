//! Browser opener.

use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvView {
    pub ssh_connection: bool,
    pub ssh_tty: bool,
    pub display: bool,
    pub wayland: bool,
    pub linux: bool,
}

impl EnvView {
    pub fn current() -> EnvView {
        fn set(name: &str) -> bool {
            std::env::var_os(name).is_some_and(|value| !value.is_empty())
        }

        EnvView {
            ssh_connection: set("SSH_CONNECTION"),
            ssh_tty: set("SSH_TTY"),
            display: set("DISPLAY"),
            wayland: set("WAYLAND_DISPLAY"),
            linux: cfg!(target_os = "linux"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenDecision {
    Open,
    Skip(SkipReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    NoOpen,
    Ssh,
    NoDisplay,
}

pub fn decide(env: &EnvView, no_open: bool) -> OpenDecision {
    if no_open {
        OpenDecision::Skip(SkipReason::NoOpen)
    } else if env.ssh_connection || env.ssh_tty {
        OpenDecision::Skip(SkipReason::Ssh)
    } else if env.linux && !env.display && !env.wayland {
        OpenDecision::Skip(SkipReason::NoDisplay)
    } else {
        OpenDecision::Open
    }
}

pub fn open(url: &str) {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = Command::new(program)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}
