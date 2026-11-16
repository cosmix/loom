use std::{
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};

use super::native_args::{parse_worker, refuse, MessageArgs, StartArgs};
use crate::{
    codex::{configured_backend, CodexBackend, CODEX_EFFORTS, CODEX_MODELS},
    commands::common::resolve_work_dir,
    native_worker::ids::{
        validate_invocation_id, validate_message_id, validate_unit_id, MAX_BRIEF_BYTES,
        MAX_MESSAGE_BYTES,
    },
    relay::{
        emit::{mode, EnvSnapshot, RelayContext, RelayMode, RelaySink, StdSink},
        RequestKind, WorkerControlPayload,
    },
    utils::sha256_hex,
};

pub(super) struct Site {
    pub mode: RelayMode,
    pub cwd: PathBuf,
}

pub(super) fn run_start(args: StartArgs) -> Result<i32> {
    let relay_mode = mode(&EnvSnapshot::from_process_env());
    let cwd = std::env::current_dir()?;
    // An inaccessible host config is left for the daemon to validate.
    let native_backend = resolve_work_dir()
        .and_then(|dir| Ok(dir.root().canonicalize()?))
        .map(|dir| configured_backend(&dir) == CodexBackend::Native)
        .unwrap_or(true);
    start_in(
        args,
        &Site {
            mode: relay_mode,
            cwd,
        },
        native_backend,
        &mut std::io::stderr(),
        &mut StdSink::default(),
    )
}

pub(super) fn start_in(
    args: StartArgs,
    site: &Site,
    native_backend: bool,
    err: &mut dyn Write,
    sink: &mut dyn RelaySink,
) -> Result<i32> {
    let context = match relay_context(&site.mode, "start") {
        Ok(context) => context,
        Err(reason) => return Ok(refuse(err, "start", &reason)),
    };
    let payload = match start_payload(args, native_backend) {
        Ok(payload) => payload,
        Err(error) => return Ok(refuse(err, "start", &format!("{error:#}"))),
    };
    relay_request(
        context,
        &site.cwd,
        payload,
        "loom subagents start",
        sink,
        err,
    )
}

fn start_payload(args: StartArgs, native_backend: bool) -> Result<WorkerControlPayload> {
    validate_unit_id(&args.unit).context("invalid --unit-id")?;
    if !CODEX_MODELS.contains(&args.model.as_str()) {
        bail!("unknown --model '{}'", args.model);
    }
    if !CODEX_EFFORTS.contains(&args.effort.as_str()) {
        bail!("unknown --effort '{}'", args.effort);
    }
    if !native_backend {
        bail!("codex.backend is legacy; set it to native in .loom/work/config.toml or ~/.loom/config.toml");
    }
    if !args.brief.is_absolute() {
        bail!("--brief must be an absolute path");
    }
    let meta = std::fs::symlink_metadata(&args.brief).context("cannot read --brief")?;
    if !meta.is_file() {
        bail!("--brief must be a regular file, not a symlink");
    }
    if meta.len() > MAX_BRIEF_BYTES {
        bail!("--brief exceeds {MAX_BRIEF_BYTES} bytes");
    }
    let text = read_text_file(&args.brief, MAX_BRIEF_BYTES).context("invalid --brief")?;
    Ok(WorkerControlPayload::Start {
        unit_id: args.unit,
        brief_path: args.brief,
        brief_sha256: sha256_hex(text.as_bytes()),
        model: args.model,
        effort: args.effort,
    })
}

pub(super) fn relay_context<'a>(
    mode: &'a RelayMode,
    command: &str,
) -> Result<&'a RelayContext, String> {
    match mode {
        RelayMode::Relay(context) => Ok(context),
        RelayMode::Operator => Err(format!("{command} runs only inside a stage session")),
        RelayMode::Legacy => Err(
            "this session cannot relay (no LOOM_SCRATCH_DIR); start it from a stage session".into(),
        ),
    }
}

pub(super) fn relay_request(
    context: &RelayContext,
    cwd: &Path,
    payload: WorkerControlPayload,
    what: &str,
    sink: &mut dyn RelaySink,
    err: &mut dyn Write,
) -> Result<i32> {
    let command = what.strip_prefix("loom subagents ").unwrap_or(what);
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    if let Err(error) = context.check(
        RequestKind::WorkerControl,
        context.stage_id.as_deref(),
        cwd,
        uid,
    ) {
        return Ok(refuse(err, command, &format!("{error:#}")));
    }
    let line = match context.emit(
        RequestKind::WorkerControl,
        serde_json::to_value(&payload)?,
        what,
        false,
        sink,
    ) {
        Ok(line) => line,
        Err(error) => return Ok(refuse(err, command, &format!("{error:#}"))),
    };
    // A failed hint must never strand an already emitted ticket.
    let _ = writeln!(
        err,
        "Wait for it in your NEXT Bash call: loom request status {} --wait 90",
        line.id
    );
    Ok(0)
}

pub(super) fn run_message(args: MessageArgs) -> Result<i32> {
    let site = Site {
        mode: mode(&EnvSnapshot::from_process_env()),
        cwd: std::env::current_dir()?,
    };
    message_in(args, &site, &mut std::io::stderr(), &mut StdSink::default())
}

pub(super) fn message_in(
    args: MessageArgs,
    site: &Site,
    err: &mut dyn Write,
    sink: &mut dyn RelaySink,
) -> Result<i32> {
    let context = match relay_context(&site.mode, "message") {
        Ok(context) => context,
        Err(reason) => return Ok(refuse(err, "message", &reason)),
    };
    let payload = match message_payload(args) {
        Ok(payload) => payload,
        Err(error) => return Ok(refuse(err, "message", &format!("{error:#}"))),
    };
    relay_request(
        context,
        &site.cwd,
        payload,
        "loom subagents message",
        sink,
        err,
    )
}

fn message_payload(args: MessageArgs) -> Result<WorkerControlPayload> {
    let unit_id = parse_worker(&args.worker).map_err(anyhow::Error::msg)?;
    validate_invocation_id(&args.invocation).context("invalid --invocation")?;
    validate_message_id(&args.message_id).context("invalid --message-id")?;
    let text = match (args.text, args.file) {
        (Some(text), None) => text,
        (None, Some(path)) => {
            read_text_file(&path, MAX_MESSAGE_BYTES as u64).context("invalid --file")?
        }
        _ => bail!("supply exactly one of --text and --file"),
    };
    if text.is_empty() || text.len() > MAX_MESSAGE_BYTES {
        bail!("--text/--file must contain 1 to {MAX_MESSAGE_BYTES} bytes");
    }
    Ok(WorkerControlPayload::Message {
        unit_id,
        invocation_id: args.invocation,
        message_id: args.message_id,
        delivery: args.delivery.into(),
        text,
    })
}

fn read_text_file(path: &Path, max: u64) -> Result<String> {
    // O_NONBLOCK keeps `open` from waiting on a FIFO writer; the regular-file
    // check below then refuses anything but a file.
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        bail!("must be a regular file");
    }
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        bail!("exceeds {max} bytes");
    }
    String::from_utf8(bytes).context("must contain UTF-8 text")
}
