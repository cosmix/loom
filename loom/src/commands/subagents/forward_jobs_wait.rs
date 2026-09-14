//! `loom subagents wait` receipt lookup, polling, and rendering.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{ensure, Result};
use serde::Serialize;

use crate::models::forward_receipt::locator::{
    default_companion_state_roots, default_task_output_roots,
};
use crate::models::forward_receipt::{is_safe_id, load_receipts, receipts_path, ForwardState};

use super::forward_jobs::{load_index_with_roots, receipt_state};

const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Serialize)]
struct WaitOutput {
    receipt_id: String,
    backend_id: Option<String>,
    state: ForwardState,
}

pub(super) fn wait(receipt_id: String, timeout_secs: u64, json: bool) -> Result<()> {
    ensure!(
        valid_receipt_id(&receipt_id),
        "receipt must be 64 lowercase hexadecimal characters"
    );
    let work_dir = crate::commands::common::work_dir_path()?;
    let stage_id = wait_stage(&work_dir, &receipt_id);
    let output = wait_until(&work_dir, stage_id.as_deref(), &receipt_id, timeout_secs);
    println!("{}", format_wait_output(&output, json)?);
    match wait_exit_code(output.state) {
        0 => Ok(()),
        code => std::process::exit(code),
    }
}

fn format_wait_output(output: &WaitOutput, json: bool) -> Result<String> {
    if json {
        return Ok(serde_json::to_string(output)?);
    }
    Ok(format!(
        "{} {} {}",
        output.receipt_id,
        output.backend_id.as_deref().unwrap_or("unknown"),
        output.state.label()
    ))
}

fn wait_until(
    work_dir: &Path,
    stage_id: Option<&str>,
    receipt_id: &str,
    timeout_secs: u64,
) -> WaitOutput {
    let home = dirs::home_dir().unwrap_or_default();
    let plugin_data = env::var_os("CLAUDE_PLUGIN_DATA").map(PathBuf::from);
    wait_until_with_roots(
        work_dir,
        stage_id,
        receipt_id,
        timeout_secs,
        &default_companion_state_roots(&home, plugin_data.as_deref()),
    )
}

fn wait_until_with_roots(
    work_dir: &Path,
    stage_id: Option<&str>,
    receipt_id: &str,
    timeout_secs: u64,
    companion_roots: &[PathBuf],
) -> WaitOutput {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let output = read_wait_output(work_dir, stage_id, receipt_id, companion_roots);
        if output.state.is_terminal() || Instant::now() >= deadline {
            return output;
        }
        thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
    }
}

fn read_wait_output(
    work_dir: &Path,
    stage_id: Option<&str>,
    receipt_id: &str,
    companion_roots: &[PathBuf],
) -> WaitOutput {
    let stage_id = stage_id
        .map(str::to_owned)
        .or_else(|| wait_stage(work_dir, receipt_id));
    let Some(stage_id) = stage_id.as_deref() else {
        return unknown_wait(receipt_id);
    };
    let index = load_index_with_roots(
        work_dir,
        stage_id,
        None,
        companion_roots.to_vec(),
        default_task_output_roots(),
    );
    let receipt = index.receipt_by_id(receipt_id);
    match receipt {
        Some(receipt) => WaitOutput {
            receipt_id: receipt_id.to_owned(),
            backend_id: Some(receipt.backend_id.clone()),
            state: receipt_state(&index, receipt),
        },
        None => unknown_wait(receipt_id),
    }
}

fn wait_stage(work_dir: &Path, receipt_id: &str) -> Option<String> {
    env::var("LOOM_STAGE_ID")
        .ok()
        .filter(|stage| is_safe_id(stage))
        .or_else(|| {
            fs::read_dir(work_dir.join("subagents"))
                .ok()?
                .flatten()
                .find_map(|entry| {
                    let stage = entry.file_name().to_str()?.to_owned();
                    let path = receipts_path(work_dir, &stage).ok()?;
                    load_receipts(&path).ok()?.get(receipt_id).map(|_| stage)
                })
        })
}

fn unknown_wait(receipt_id: &str) -> WaitOutput {
    WaitOutput {
        receipt_id: receipt_id.to_owned(),
        backend_id: None,
        state: ForwardState::Unknown,
    }
}

fn wait_exit_code(state: ForwardState) -> i32 {
    match state {
        ForwardState::Succeeded => 0,
        ForwardState::Failed | ForwardState::Canceled | ForwardState::TimedOut => 1,
        ForwardState::Queued | ForwardState::Running | ForwardState::Unknown => 2,
    }
}

fn valid_receipt_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[cfg(test)]
#[path = "forward_jobs_wait_tests.rs"]
mod tests;
