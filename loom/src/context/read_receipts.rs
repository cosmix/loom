//! Bounded, correlation-backed receipts for successful Claude `Read` calls.
//!
//! The shell attempt TSV is intentionally not consulted here.  A receipt only
//! exists after the matching `tool_result` appears beside the `tool_use` in a
//! bounded transcript window and the requested source generation is unchanged.

mod source;
mod storage;
mod transcript;

use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

const RECEIPT_SCHEMA: u8 = 2;
pub const MAX_RECEIPT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Prepare,
    Check,
    Complete,
}

#[derive(Debug, Clone)]
pub struct Payload {
    tool_name: String,
    input: Value,
    session_id: String,
    agent_id: String,
    cwd: PathBuf,
    transcript_path: Option<PathBuf>,
    tool_use_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Identity {
    session_id: String,
    agent_id: String,
    path: String,
    range: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Intent {
    schema: u8,
    identity: Identity,
    generation: String,
    nonce: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ContentClass {
    Text,
    Media,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Receipt {
    schema: u8,
    identity: Identity,
    generation: String,
    content_class: ContentClass,
    result_hash: String,
    result_bytes: usize,
    tool_use_ids: Vec<String>,
    completed_at: String,
}

pub struct ReceiptStore {
    root: PathBuf,
}

impl Payload {
    pub fn parse(raw: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(raw).ok()?;
        let input = value.get("tool_input")?.clone();
        let cwd = value.get("cwd")?.as_str()?.trim();
        let tool_use_id = parse_tool_use_id(&value)?;
        if cwd.is_empty() {
            return None;
        }
        Some(Self {
            tool_name: value.get("tool_name")?.as_str()?.to_owned(),
            input,
            session_id: normalized_id(value.get("session_id").and_then(Value::as_str), "unknown"),
            agent_id: normalized_id(value.get("agent_id").and_then(Value::as_str), "main"),
            cwd: PathBuf::from(cwd),
            transcript_path: value
                .get("transcript_path")
                .and_then(Value::as_str)
                .filter(|path| !path.trim().is_empty())
                .map(PathBuf::from),
            tool_use_id,
        })
    }
}

impl ReceiptStore {
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    fn for_payload(payload: &Payload) -> Option<Self> {
        Self::for_session(&payload.session_id)
    }

    fn for_session(session_id: &str) -> Option<Self> {
        if !is_safe_id(session_id) {
            return None;
        }
        if let Some(root) = staged_root() {
            return Some(Self::at(root));
        }
        let temp = std::env::temp_dir();
        let root = fallback_root(&temp, session_id);
        ensure_private_fallback_root(&temp, &root).ok()?;
        Some(Self::at(root))
    }

    pub fn prepare(&self, payload: &Payload) -> Result<()> {
        let epoch = self.epoch_token(&payload.session_id)?;
        self.prepare_with_epoch(payload, &epoch)
    }

    fn prepare_with_epoch(&self, payload: &Payload, epoch: &str) -> Result<()> {
        let Some(identity) = identity(payload) else {
            return Ok(());
        };
        let Some(generation) = source::generation(&identity, epoch) else {
            return Ok(());
        };
        let intent = Intent {
            schema: RECEIPT_SCHEMA,
            identity: identity.clone(),
            generation,
            nonce: short_nonce(),
        };
        storage::write_intent(&self.root, &identity, &intent)
    }

    pub fn complete(&self, payload: &Payload) -> Result<()> {
        let epoch = self.epoch_token(&payload.session_id)?;
        self.complete_with_epoch(payload, &epoch)
    }

    fn complete_with_epoch(&self, payload: &Payload, epoch: &str) -> Result<()> {
        let Some(identity) = identity(payload) else {
            return Ok(());
        };
        let Some(pending) = storage::load_single_intent(&self.root, &identity)? else {
            return Ok(());
        };
        let Some(generation) = source::generation(&identity, epoch) else {
            return storage::discard_pending(&self.root, &pending);
        };
        if pending.intent.schema != RECEIPT_SCHEMA
            || pending.intent.identity != identity
            || pending.intent.generation != generation
        {
            return storage::discard_pending(&self.root, &pending);
        }
        let Some(result) = transcript::correlated_result(payload) else {
            return storage::discard_pending(&self.root, &pending);
        };
        let receipt = Receipt {
            schema: RECEIPT_SCHEMA,
            identity,
            generation,
            content_class: result.class,
            result_hash: digest(&result.bytes),
            result_bytes: result.bytes.len(),
            tool_use_ids: vec![result.tool_use_id],
            completed_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        };
        storage::complete(&self.root, &receipt, &pending).map(|_| ())
    }

    pub fn check(&self, payload: &Payload) -> Result<Option<usize>> {
        let epoch = self.epoch_token(&payload.session_id)?;
        self.check_with_epoch(payload, &epoch)
    }

    fn check_with_epoch(&self, payload: &Payload, epoch: &str) -> Result<Option<usize>> {
        let Some(identity) = identity(payload) else {
            return Ok(None);
        };
        let Some(receipt) = storage::load_receipt(&self.root, &identity)? else {
            return Ok(None);
        };
        if receipt.schema != RECEIPT_SCHEMA
            || receipt.identity != identity
            || receipt.content_class != ContentClass::Text
            || !valid_tool_use_ids(&receipt.tool_use_ids)
        {
            return Ok(None);
        }
        let current = source::generation(&identity, epoch);
        Ok((current == Some(receipt.generation)).then_some(receipt.tool_use_ids.len()))
    }

    fn has_candidate(&self, payload: &Payload) -> Result<bool> {
        let Some(identity) = identity(payload) else {
            return Ok(false);
        };
        Ok(storage::load_receipt(&self.root, &identity)?.is_some())
    }
}

/// Execute one hook action. All recoverable failures deliberately become no-ops.
pub fn invoke(mode: Mode, raw: &str) -> Option<usize> {
    let payload = Payload::parse(raw)?;
    if payload.tool_name != "Read" {
        return None;
    }
    let store = ReceiptStore::for_payload(&payload)?;
    let Ok(epoch) = store.epoch_token(&payload.session_id) else {
        return None;
    };
    if mode == Mode::Check && !store.has_candidate(&payload).unwrap_or_default() {
        return None;
    }
    match mode {
        Mode::Prepare => {
            let _ = store.prepare_with_epoch(&payload, &epoch);
            None
        }
        Mode::Complete => {
            let _ = store.complete_with_epoch(&payload, &epoch);
            None
        }
        Mode::Check => store.check_with_epoch(&payload, &epoch).ok().flatten(),
    }
}

fn identity(payload: &Payload) -> Option<Identity> {
    if payload.tool_name != "Read" {
        return None;
    }
    let raw_path = payload.input.get("file_path")?.as_str()?;
    let path = source::normalized_path(&payload.cwd, raw_path)?;
    let range = source::range(&payload.input)?;
    Some(Identity {
        session_id: payload.session_id.clone(),
        agent_id: payload.agent_id.clone(),
        path,
        range,
    })
}

/// Rotate a session's compaction epoch without making PreCompact load-bearing.
pub fn rotate_epoch_for_session(session_id: &str) {
    let Some(store) = ReceiptStore::for_session(session_id) else {
        return;
    };
    let _ = store.rotate_epoch(session_id);
}

#[cfg(test)]
pub(crate) fn epoch_token_for_session(session_id: &str) -> Option<String> {
    let store = ReceiptStore::for_session(session_id)?;
    store.epoch_token(session_id).ok()
}

fn normalized_id(value: Option<&str>, fallback: &str) -> String {
    value
        .filter(|value| is_safe_id(value))
        .unwrap_or(fallback)
        .to_owned()
}

fn parse_tool_use_id(value: &Value) -> Option<Option<String>> {
    match value.get("tool_use_id") {
        None | Some(Value::Null) => Some(None),
        Some(Value::String(id)) if is_safe_id(id) => Some(Some(id.to_owned())),
        _ => None,
    }
}

fn is_safe_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn short_nonce() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn valid_tool_use_ids(ids: &[String]) -> bool {
    !ids.is_empty()
        && ids.len() <= storage::MAX_PROVEN_TOOL_USES
        && ids.iter().all(|id| is_safe_id(id))
        && ids
            .iter()
            .enumerate()
            .all(|(index, id)| !ids[..index].contains(id))
}

impl ReceiptStore {
    fn epoch_token(&self, session_id: &str) -> Result<String> {
        storage::read_epoch(&self.root, session_id)
    }

    fn rotate_epoch(&self, session_id: &str) -> Result<()> {
        storage::rotate_epoch(&self.root, session_id, &short_nonce())
    }
}

fn staged_root() -> Option<PathBuf> {
    let work_dir = PathBuf::from(std::env::var_os("LOOM_WORK_DIR")?);
    let session_id = std::env::var("LOOM_SESSION_ID").ok()?;
    let stage_id = std::env::var("LOOM_STAGE_ID").ok()?;
    if !work_dir.is_dir() || !is_safe_id(&session_id) || !is_safe_id(&stage_id) {
        return None;
    }
    Some(
        work_dir
            .join("hooks/reads")
            .join(session_id)
            .join("receipts"),
    )
}

fn fallback_root(temp: &Path, session_id: &str) -> PathBuf {
    let uid = effective_uid();
    let session = digest(session_id.as_bytes());
    temp.join("loom-reads")
        .join(format!("uid-{uid}"))
        .join(format!("session-{session}"))
        .join("receipts")
}

fn ensure_private_fallback_root(temp: &Path, root: &Path) -> Result<()> {
    let relative = root.strip_prefix(temp)?;
    let mut current = temp.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        create_or_validate_private_dir(&current)?;
    }
    Ok(())
}

fn create_or_validate_private_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        builder.create(path)?;
    }
    let metadata = std::fs::symlink_metadata(path)?;
    anyhow::ensure!(
        metadata.file_type().is_dir(),
        "receipt directory is not plain"
    );
    anyhow::ensure!(
        metadata.uid() == effective_uid(),
        "receipt directory has another owner"
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o077 == 0,
        "receipt directory is exposed"
    );
    crate::fs::safe_write::safe_open_dirfd(path).map(|_| ())
}

fn effective_uid() -> u32 {
    // SAFETY: geteuid has no arguments and only reads process credentials.
    unsafe { libc::geteuid() }
}

#[cfg(test)]
#[path = "read_receipts/tests.rs"]
mod tests;
