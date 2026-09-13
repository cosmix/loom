use crate::models::forward_receipt::job_record::read_companion_job;
use crate::models::forward_receipt::locator::{
    default_companion_state_roots, default_task_output_roots, resolve_companion_locator,
    LocatorResolution,
};
use crate::models::forward_receipt::marker::{EndMarker, MarkerChannel, StartMarker};
use crate::models::forward_receipt::transcript::{
    self, evidence_channel, ForwardingInvocation, ForwardingResult,
};
use crate::models::forward_receipt::{
    is_safe_id, load_receipts, receipts_path, ForwardBackend, ForwardIdentity, ForwardObservation,
    ForwardState, FORWARD_RECEIPT_SCHEMA,
};
use anyhow::{ensure, Context, Result};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::{Read, Write},
};
const MAX_EXISTING_BYTES: u64 = 8 * 1024 * 1024;
struct Config {
    work_dir: PathBuf,
    stage_id: String,
    loom_session_id: String,
    task_roots: Vec<PathBuf>,
    companion_roots: Vec<PathBuf>,
}
pub fn forward_receipt(transcript: &Path) -> Result<()> {
    let Some(config) = Config::from_env() else {
        return Ok(());
    };
    if let Err(error) = process(transcript, &config) {
        let message = error.to_string().replace(['\n', '\r'], " ");
        eprintln!("loom hook forward-receipt: {message}");
    }
    Ok(())
}
impl Config {
    fn from_env() -> Option<Self> {
        let work_dir = PathBuf::from(std::env::var_os("LOOM_WORK_DIR")?);
        let stage_id = std::env::var("LOOM_STAGE_ID").ok()?;
        let loom_session_id = std::env::var("LOOM_SESSION_ID").ok()?;
        let home = std::env::var_os("HOME").map_or_else(PathBuf::new, PathBuf::from);
        let plugin_data = std::env::var_os("CLAUDE_PLUGIN_DATA").map(PathBuf::from);
        Some(Self {
            work_dir,
            stage_id,
            loom_session_id,
            task_roots: default_task_output_roots(),
            companion_roots: default_companion_state_roots(&home, plugin_data.as_deref()),
        })
    }
}
fn process(transcript: &Path, config: &Config) -> Result<()> {
    ensure!(config.work_dir.is_dir(), "work directory is unavailable");
    ensure!(is_safe_id(&config.stage_id), "unsafe stage id");
    ensure!(
        is_safe_id(&config.loom_session_id),
        "unsafe Loom session id"
    );
    let transcript = transcript::read(transcript)?;
    let mut observations = Vec::new();
    for call in &transcript.invocations {
        let Some(result) = call.result.as_ref() else {
            continue;
        };
        let identity = ForwardIdentity::new(
            &transcript.identity.parent_session_id,
            &transcript.identity.agent_id,
            &call.id,
            &config.stage_id,
            &config.loom_session_id,
        )?;
        observations.extend(observe(&identity, call, result, config));
    }
    if !observations.is_empty() {
        persist(config, &observations)?;
    }
    Ok(())
}
fn observe(
    identity: &ForwardIdentity,
    call: &ForwardingInvocation,
    result: &ForwardingResult,
    config: &Config,
) -> Vec<ForwardObservation> {
    let channel = evidence_channel(result, &config.task_roots, &identity.parent_session_id);
    let (start, end) = match channel {
        MarkerChannel::Streaming(start) | MarkerChannel::Started(start) => (start, None),
        MarkerChannel::Finished(start, end) => (start, Some(end)),
        MarkerChannel::Absent | MarkerChannel::Invalid(_) => return Vec::new(),
    };
    let backend = backend_evidence(&start, config);
    let mut observations = vec![observation(
        identity,
        call,
        &start,
        ForwardState::Running,
        call.timestamp,
        None,
        backend.thread.clone(),
        backend.locator.clone(),
    )];
    let terminal = terminal_evidence(start.backend, end, backend.authoritative);
    if let Some((state, exit_code)) = terminal {
        observations.push(observation(
            identity,
            call,
            &start,
            state,
            result.timestamp,
            exit_code,
            backend.thread,
            backend.locator,
        ));
    }
    observations
}

struct BackendEvidence {
    thread: Option<String>,
    locator: Option<String>,
    authoritative: Option<ForwardState>,
}

fn backend_evidence(start: &StartMarker, config: &Config) -> BackendEvidence {
    if start.backend == ForwardBackend::Direct {
        return BackendEvidence {
            thread: Some(start.backend_id.clone()),
            locator: None,
            authoritative: None,
        };
    }
    let LocatorResolution::Found(path) =
        resolve_companion_locator(&config.companion_roots, &start.backend_id)
    else {
        return BackendEvidence {
            thread: None,
            locator: None,
            authoritative: None,
        };
    };
    let job = read_companion_job(&path, &start.backend_id).ok();
    BackendEvidence {
        thread: job.as_ref().and_then(|job| job.thread_id.clone()),
        locator: Some(path.to_string_lossy().into_owned()),
        authoritative: job.map(|job| job.state()),
    }
}

fn terminal_evidence(
    backend: ForwardBackend,
    end: Option<EndMarker>,
    authoritative: Option<ForwardState>,
) -> Option<(ForwardState, Option<i32>)> {
    match backend {
        ForwardBackend::Direct => end.map(|end| (end.outcome, Some(end.exit_code))),
        ForwardBackend::Companion => {
            authoritative
                .filter(|state| state.is_terminal())
                .and_then(|state| {
                    end.as_ref()
                        .is_none_or(|end| end.outcome == state)
                        .then(|| (state, end.map(|value| value.exit_code)))
                })
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn observation(
    identity: &ForwardIdentity,
    call: &ForwardingInvocation,
    start: &StartMarker,
    state: ForwardState,
    observed_at: DateTime<Utc>,
    exit_code: Option<i32>,
    codex_thread_id: Option<String>,
    locator: Option<String>,
) -> ForwardObservation {
    ForwardObservation {
        schema: FORWARD_RECEIPT_SCHEMA,
        receipt_id: identity.receipt_id(),
        parent_session_id: identity.parent_session_id.clone(),
        agent_id: identity.agent_id.clone(),
        tool_use_id: identity.tool_use_id.clone(),
        stage_id: identity.stage_id.clone(),
        loom_session_id: identity.loom_session_id.clone(),
        backend: start.backend,
        backend_id: start.backend_id.clone(),
        state,
        observed_at,
        exit_code,
        codex_thread_id,
        locator,
        model: Some(call.model.clone()),
        effort: Some(call.effort.clone()),
    }
}
fn persist(config: &Config, observations: &[ForwardObservation]) -> Result<()> {
    let path = receipts_path(&config.work_dir, &config.stage_id)?;
    ensure_receipts_dir(config)?;
    reject_non_regular(&path)?;
    let _lock = lock_receipts(&path)?;
    reject_non_regular(&path)?;
    let loaded = load_receipts(&path)?;
    ensure!(
        !loaded.truncated && loaded.malformed == 0,
        "existing receipts are not appendable"
    );
    let existing = read_existing(&path)?;
    let append = encode_new_observations(observations, &existing)?;
    if !append.is_empty() {
        append_receipts(&path, &append)?;
    }
    Ok(())
}

fn lock_receipts(path: &Path) -> Result<fs::File> {
    let lock_path = path.with_extension("jsonl.lock");
    reject_non_regular(&lock_path)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&lock_path)
        .context("opening receipt lock")?;
    lock.lock_exclusive().context("locking receipt file")?;
    Ok(lock)
}

fn encode_new_observations(
    observations: &[ForwardObservation],
    existing: &[ForwardObservation],
) -> Result<String> {
    let mut append = String::new();
    for observation in observations
        .iter()
        .filter(|value| !existing.contains(value))
    {
        append.push_str(&observation.encode_line()?);
        append.push('\n');
    }
    Ok(append)
}

fn append_receipts(path: &Path, append: &str) -> Result<()> {
    let was_missing = !path.exists();
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .context("opening forward receipts")?;
    ensure!(
        file.metadata()?.is_file(),
        "forward receipts are not a regular file"
    );
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(append.as_bytes())
        .context("appending forward receipts")?;
    file.sync_data().context("syncing forward receipts")?;
    if was_missing {
        if let Some(parent) = path.parent() {
            if let Ok(directory) = fs::File::open(parent) {
                let _ = directory.sync_all();
            }
        }
    }
    Ok(())
}
fn ensure_receipts_dir(config: &Config) -> Result<()> {
    let subagents = plain_child(&config.work_dir, "subagents")?;
    plain_child(&subagents, &config.stage_id)?;
    Ok(())
}
fn plain_child(parent: &Path, name: &str) -> Result<PathBuf> {
    let path = parent.join(name);
    if !path.exists() {
        DirBuilder::new().mode(0o700).create(&path)?;
    }
    ensure!(
        fs::symlink_metadata(&path)?.file_type().is_dir(),
        "receipt directory is not plain"
    );
    Ok(path)
}
fn reject_non_regular(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.file_type().is_file(),
                "receipt path is not a regular file"
            );
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
fn read_existing(path: &Path) -> Result<Vec<ForwardObservation>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let mut input = String::new();
    Read::by_ref(&mut file)
        .take(MAX_EXISTING_BYTES)
        .read_to_string(&mut input)?;
    input.lines().map(ForwardObservation::decode_line).collect()
}
#[cfg(test)]
#[path = "tests_forward_receipt.rs"]
mod tests;
