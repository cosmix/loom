//! Hidden native-worker commands: `supervise`, which the daemon starts for one
//! invocation, and `worker-exec`, which runs inside the worker boundary.
//!
//! Exit codes: 0 success, 1 the run failed (its record says why), 2 refused.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;

const EXIT_FAILED: i32 = 1;
const EXIT_REFUSED: i32 = 2;

/// `loom subagents supervise --dir <invocation dir>`.
pub fn supervise(dir: PathBuf) -> Result<()> {
    if ["LOOM_SESSION_ID", "LOOM_STAGE_ID"]
        .iter()
        .any(|name| std::env::var_os(name).is_some())
    {
        eprintln!("supervise runs only from the loom daemon");
        std::process::exit(EXIT_REFUSED);
    }
    exit_on_failure(
        "supervise",
        crate::native_worker::supervisor::run_from_dir(&dir),
    )
}

/// `loom subagents worker-exec --proxy-socket <path> [--listener-fd <fd> --control-fd <fd>]
/// -- <program> [args...]`. With the inherited descriptors it is the macOS bridge inside the
/// Seatbelt profile (`bridge::run_confined`); without them the Linux in-boundary bridge.
/// The child's precise status is not forwarded: `ExitCode` has no integer view.
pub fn worker_exec(
    proxy_socket: PathBuf,
    inherited: Option<crate::native_worker::bridge::Inherited>,
    command: Vec<OsString>,
) -> Result<()> {
    let Some((program, args)) = command.split_first() else {
        eprintln!("worker-exec: no program given");
        std::process::exit(EXIT_REFUSED);
    };
    let program = Path::new(program);
    let result = match inherited {
        Some(inherited) => {
            crate::native_worker::bridge::run_confined(&proxy_socket, inherited, program, args)
        }
        None => crate::native_worker::bridge::run(&proxy_socket, program, args),
    };
    exit_on_failure("worker-exec", result)
}

fn exit_on_failure(name: &str, result: Result<ExitCode>) -> Result<()> {
    match result {
        Ok(code) if code == ExitCode::SUCCESS => Ok(()),
        Ok(_) => std::process::exit(EXIT_FAILED),
        Err(e) => {
            eprintln!("{name}: {e:#}");
            std::process::exit(EXIT_REFUSED);
        }
    }
}
