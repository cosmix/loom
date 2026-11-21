use std::{
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use super::{
    native_args::{parse_worker, refuse, CancelArgs, ReleaseArgs},
    native_start::{relay_context, relay_request, Site},
};
use crate::{
    commands::common::resolve_work_dir,
    native_worker::{
        admission::{accept_cancel, CancelRequestIn},
        boundary::Quiescence,
        ids::validate_invocation_id,
        index,
        model::{NativeEvent, Ownership},
        store, supervisor,
    },
    relay::{
        emit::{mode, EnvSnapshot, StdSink},
        WorkerControlPayload,
    },
    validation::validate_id,
};

pub(super) fn work_root() -> Result<PathBuf> {
    resolve_work_dir()?
        .root()
        .canonicalize()
        .context("cannot resolve the work directory")
}

pub(super) fn run_cancel(args: CancelArgs) -> Result<i32> {
    let env = EnvSnapshot::from_process_env();
    let site = Site {
        mode: mode(&env),
        cwd: std::env::current_dir()?,
    };
    let work_dir = if env.session_id.is_some() {
        PathBuf::new()
    } else {
        work_root()?
    };
    cancel_in(
        args,
        env.session_id.as_ref().map(|_| &site),
        &work_dir,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
        &mut StdSink::default(),
    )
}

pub(super) fn cancel_in(
    args: CancelArgs,
    session: Option<&Site>,
    work_dir: &Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
    sink: &mut dyn crate::relay::emit::RelaySink,
) -> Result<i32> {
    if let Some(site) = session {
        return cancel_session(args, site, err, sink);
    }
    let stage = match args.stage.as_deref() {
        Some(stage) => stage,
        None => {
            return Ok(refuse(
                err,
                "cancel",
                "--stage is required in the operator shell",
            ))
        }
    };
    if args.invocation.is_some() {
        return Ok(refuse(err, "cancel", "--invocation is for a stage session"));
    }
    if let Err(error) = validate_id(stage) {
        return Ok(refuse(
            err,
            "cancel",
            &format!("invalid --stage: {error:#}"),
        ));
    }
    let request = match cancel_request(&args) {
        Ok(request) => request,
        Err(reason) => return Ok(refuse(err, "cancel", &reason)),
    };
    match accept_cancel(work_dir, stage, &request, "operator") {
        Ok(note) => {
            writeln!(out, "{note}")?;
            Ok(0)
        }
        Err(reason) => Ok(refuse(err, "cancel", &reason)),
    }
}

fn cancel_session(
    args: CancelArgs,
    site: &Site,
    err: &mut dyn Write,
    sink: &mut dyn crate::relay::emit::RelaySink,
) -> Result<i32> {
    if args.stage.is_some() {
        return Ok(refuse(err, "cancel", "--stage is for the operator shell"));
    }
    let Some(invocation_id) = args.invocation.as_deref() else {
        return Ok(refuse(
            err,
            "cancel",
            "--invocation is required inside a stage session",
        ));
    };
    let request = match cancel_request(&args) {
        Ok(request) => request,
        Err(reason) => return Ok(refuse(err, "cancel", &reason)),
    };
    let context = match relay_context(&site.mode, "cancel") {
        Ok(context) => context,
        Err(reason) => return Ok(refuse(err, "cancel", &reason)),
    };
    let payload = WorkerControlPayload::Cancel {
        unit_id: request.unit_id,
        invocation_id: invocation_id.to_owned(),
        reason: request.reason,
    };
    relay_request(
        context,
        &site.cwd,
        payload,
        "loom subagents cancel",
        sink,
        err,
    )
}

fn cancel_request(args: &CancelArgs) -> Result<CancelRequestIn, String> {
    let unit_id = parse_worker(&args.worker)?;
    if let Some(id) = &args.invocation {
        validate_invocation_id(id).map_err(|error| format!("invalid --invocation: {error}"))?;
    }
    let reason = args.reason.trim();
    if reason.is_empty() || reason.len() > 1024 {
        return Err("--reason must contain 1 to 1024 bytes after trimming".into());
    }
    Ok(CancelRequestIn {
        unit_id,
        invocation_id: args.invocation.clone(),
        reason: reason.into(),
    })
}

pub(super) fn run_release(args: ReleaseArgs) -> Result<i32> {
    let in_session = EnvSnapshot::from_process_env().session_id.is_some();
    let work_dir = if in_session {
        PathBuf::new()
    } else {
        work_root()?
    };
    release_in(
        args,
        in_session,
        &work_dir,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
}

pub(super) fn release_in(
    args: ReleaseArgs,
    in_session: bool,
    work_dir: &Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<i32> {
    if in_session {
        return Ok(refuse(err, "release", "release is an operator command"));
    }
    if let Err(reason) = release_gate(&args, work_dir)? {
        return Ok(refuse(err, "release", &reason));
    }
    let dir = store::invocation_dir(work_dir, &args.stage, &args.invocation);
    store::EventLog::open(&dir)?.append(NativeEvent::OwnershipReleased {
        by: "operator".into(),
    })?;
    writeln!(out, "released {}", args.invocation)?;
    Ok(0)
}

/// `Ok(Err(reason))` is a refusal; `Err` is an I/O failure outside the record.
/// Only the named invocation's record is read: a damaged sibling cannot block its release.
fn release_gate(args: &ReleaseArgs, work_dir: &Path) -> Result<Result<(), String>> {
    if let Err(error) = validate_id(&args.stage) {
        return Ok(Err(format!("invalid --stage: {error:#}")));
    }
    if let Err(error) = validate_invocation_id(&args.invocation) {
        return Ok(Err(format!("invalid --invocation: {error}")));
    }
    let dir = store::invocation_dir(work_dir, &args.stage, &args.invocation);
    let state = match index::invocation_state(work_dir, &args.stage, &args.invocation) {
        Ok(Some(state)) => state,
        Ok(None) => return Ok(Err("no such invocation".into())),
        Err(error) => return Ok(Err(damaged(&dir, &error))),
    };
    if state.ownership != Ownership::Unverified {
        let ownership = match state.ownership {
            Ownership::Held => "held",
            _ => "released",
        };
        return Ok(Err(format!("nothing to release: ownership is {ownership}")));
    }
    let identity = match store::read_boundary(&dir) {
        Ok(identity) => identity,
        Err(error) => return Ok(Err(damaged(&dir, &error))),
    };
    // On Linux an operator may release a spawn that recorded no identity; a
    // macOS spawn without a macOS identity is refused (no proof can run).
    if cfg!(target_os = "macos") {
        if let Some(detail) = supervisor::unprovable_spawn(&dir, identity.as_ref()) {
            return Ok(Err(unprovable(&dir, &detail)));
        }
    }
    let Some(identity) = identity else {
        return Ok(Ok(()));
    };
    // The same read-only proof as daemon recovery; a proven release removes
    // the private invocation root before the record says released.
    match supervisor::prove_and_reclaim(&identity) {
        Ok(Quiescence::Released) => Ok(Ok(())),
        Ok(Quiescence::Unverified(detail)) => {
            Ok(Err(format!("still not provably stopped: {detail}")))
        }
        Err(detail) => Ok(Err(detail)),
    }
}

fn unprovable(dir: &Path, detail: &str) -> String {
    let invocation = dir
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    format!(
        "{detail}; confirm no process of this worker still runs (look for {}={invocation} in \
         process environments), then remove {} and the stage cache's native/{invocation} \
         directory",
        crate::native_worker::boundary::quiescence_scan::BOUNDARY_MARKER_VAR,
        dir.display()
    )
}

fn damaged(dir: &Path, error: &store::StoreError) -> String {
    format!(
        "the record {} is damaged ({error}); confirm no process of this worker still runs, then remove that directory",
        dir.display()
    )
}
