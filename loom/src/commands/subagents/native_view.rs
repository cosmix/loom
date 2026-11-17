use std::{io::Write, path::Path};

use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};

use super::{
    native_args::{parse_worker, refuse, ResultArgs, StatusArgs},
    native_control::work_root,
};
use crate::{
    native_worker::{
        ids::{MAX_RECORD_BYTES, REPORT_DEFAULT_BYTES},
        index,
        model::NativeUnitState,
        store,
    },
    relay::emit::EnvSnapshot,
    validation::validate_id,
};

pub(super) fn run_status(args: StatusArgs) -> Result<i32> {
    let env_stage = EnvSnapshot::from_process_env().stage_id;
    if args.stage.as_ref().or(env_stage.as_ref()).is_none() {
        return Ok(refuse(
            &mut std::io::stderr(),
            "status",
            "--stage or LOOM_STAGE_ID is required",
        ));
    }
    status_in(
        args,
        env_stage.as_deref(),
        &work_root()?,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
}

pub(super) fn run_result(args: ResultArgs) -> Result<i32> {
    let env_stage = EnvSnapshot::from_process_env().stage_id;
    if args.stage.as_ref().or(env_stage.as_ref()).is_none() {
        return Ok(refuse(
            &mut std::io::stderr(),
            "result",
            "--stage or LOOM_STAGE_ID is required",
        ));
    }
    result_in(
        args,
        env_stage.as_deref(),
        &work_root()?,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
}

fn read_state(
    worker: &str,
    stage: Option<&str>,
    work_dir: &Path,
    command: &str,
    err: &mut dyn Write,
) -> Result<NativeUnitState, i32> {
    let unit = parse_worker(worker).map_err(|reason| refuse(err, command, &reason))?;
    let stage =
        stage.ok_or_else(|| refuse(err, command, "--stage or LOOM_STAGE_ID is required"))?;
    validate_id(stage)
        .map_err(|error| refuse(err, command, &format!("invalid --stage: {error:#}")))?;
    match index::unit_state(work_dir, stage, &unit) {
        Ok(Some(state)) => Ok(state),
        Ok(None) => {
            let _ = writeln!(
                err,
                "no native worker record for codex:{unit} in stage {stage}"
            );
            Err(1)
        }
        Err(error) => {
            let _ = writeln!(err, "{error}");
            Err(1)
        }
    }
}

pub(super) fn status_in(
    args: StatusArgs,
    env_stage: Option<&str>,
    work_dir: &Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<i32> {
    let state = match read_state(
        &args.worker,
        args.stage.as_deref().or(env_stage),
        work_dir,
        "status",
        err,
    ) {
        Ok(state) => state,
        Err(code) => return Ok(code),
    };
    if args.json {
        writeln!(out, "{}", status_json(&state))?;
    } else {
        status_text(&state, out)?;
    }
    Ok(0)
}

fn status_json(state: &NativeUnitState) -> Value {
    let manifest = &state.manifest;
    json!({
        "unit_id": manifest.unit_id, "stage_id": manifest.stage_id,
        "invocation_id": manifest.invocation_id, "phase": state.phase,
        "outcome": state.outcome, "outcome_reason": state.outcome_reason,
        "ownership": state.ownership, "model": manifest.model, "effort": manifest.effort,
        "thread_id": state.thread_id,
        "cancel_requested": state.cancel_requested, "last_progress_at": state.last_progress_at,
        "report": state.report, "messages": state.messages,
    })
}

fn status_text(state: &NativeUnitState, out: &mut dyn Write) -> Result<()> {
    let manifest = &state.manifest;
    writeln!(out, "worker: codex:{}", manifest.unit_id)?;
    writeln!(out, "stage: {}", manifest.stage_id)?;
    writeln!(out, "invocation: {}", manifest.invocation_id)?;
    writeln!(out, "phase: {}", enum_text(&state.phase))?;
    writeln!(out, "outcome: {}", enum_text(&state.outcome))?;
    writeln!(
        out,
        "reason: {}",
        state.outcome_reason.as_deref().unwrap_or("-")
    )?;
    writeln!(out, "ownership: {}", enum_text(&state.ownership))?;
    writeln!(out, "model: {}", manifest.model)?;
    writeln!(out, "effort: {}", manifest.effort)?;
    writeln!(out, "thread: {}", state.thread_id.as_deref().unwrap_or("-"))?;
    writeln!(
        out,
        "cancel requested: {}",
        if state.cancel_requested { "yes" } else { "no" }
    )?;
    let ago = chrono::Utc::now()
        .signed_duration_since(state.last_progress_at)
        .num_seconds()
        .max(0);
    writeln!(
        out,
        "last progress: {} ({ago}s ago)",
        state.last_progress_at.to_rfc3339()
    )?;
    write_report_line(state, out)?;
    write_messages_line(state, out)
}

fn write_report_line(state: &NativeUnitState, out: &mut dyn Write) -> Result<()> {
    match &state.report {
        Some(report) => writeln!(
            out,
            "report: {} bytes, {}",
            report.bytes,
            if report.complete {
                "complete"
            } else {
                "partial"
            }
        )?,
        None => writeln!(out, "report: none")?,
    }
    Ok(())
}

fn write_messages_line(state: &NativeUnitState, out: &mut dyn Write) -> Result<()> {
    let messages = state
        .messages
        .iter()
        .map(|message| format!("{}={}", message.message_id, enum_text(&message.state)))
        .collect::<Vec<_>>()
        .join(" ");
    writeln!(
        out,
        "messages: {}",
        if messages.is_empty() {
            "none"
        } else {
            &messages
        }
    )?;
    Ok(())
}

fn enum_text(value: &impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "-".into())
}

pub(super) fn result_in(
    args: ResultArgs,
    env_stage: Option<&str>,
    work_dir: &Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<i32> {
    let state = match read_state(
        &args.worker,
        args.stage.as_deref().or(env_stage),
        work_dir,
        "result",
        err,
    ) {
        Ok(state) => state,
        Err(code) => return Ok(code),
    };
    let manifest = &state.manifest;
    let dir = store::invocation_dir(work_dir, &manifest.stage_id, &manifest.invocation_id);
    let max = if args.full {
        MAX_RECORD_BYTES as usize
    } else {
        REPORT_DEFAULT_BYTES
    };
    let (report, truncated) = match store::read_report(&dir, max) {
        Ok(Some(report)) => report,
        Ok(None) => {
            writeln!(
                err,
                "no report is stored for codex:{} ({}, phase {})",
                manifest.unit_id,
                manifest.invocation_id,
                enum_text(&state.phase)
            )?;
            return Ok(1);
        }
        Err(error) => {
            writeln!(err, "{error}")?;
            return Ok(1);
        }
    };
    write_result(out, &state, &report, truncated, args.json)?;
    Ok(0)
}

fn write_result(
    out: &mut dyn Write,
    state: &NativeUnitState,
    report: &str,
    truncated: bool,
    as_json: bool,
) -> Result<()> {
    let manifest = &state.manifest;
    if as_json {
        writeln!(
            out,
            "{}",
            json!({"unit_id": manifest.unit_id, "invocation_id": manifest.invocation_id,
            "phase": state.phase, "outcome": state.outcome,
            "complete": state.report.as_ref().is_some_and(|report| report.complete),
            "truncated": truncated, "report": report})
        )?;
    } else {
        write!(out, "{report}")?;
        if truncated {
            if !report.ends_with('\n') {
                writeln!(out)?;
            }
            writeln!(out, "[truncated: showing the first {} bytes of the report; run again with --full for all of it]", report.len())?;
        }
    }
    Ok(())
}
