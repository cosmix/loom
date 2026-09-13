use super::{digest, ContentClass, Identity, Intent, Receipt, MAX_RECEIPT_BYTES, RECEIPT_SCHEMA};
use anyhow::{ensure, Context, Result};
use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::Path;

use crate::fs::locking::locked_dir_update;
use crate::models::forward_receipt::locator::read_bounded_prefix;

const MAX_RECORD_BYTES: usize = MAX_RECEIPT_BYTES * 2;
const MAX_EPOCH_BYTES: usize = 128;
const MAX_DIRECTORY_ENTRIES: usize = 64;
pub(super) const MAX_PROVEN_TOOL_USES: usize = 8;

pub(super) struct PendingIntent {
    pub intent: Intent,
    name: String,
}

pub(super) fn write_intent(root: &Path, identity: &Identity, intent: &Intent) -> Result<()> {
    ensure_root(root)?;
    let name = intent_name(identity, &intent.nonce);
    locked_dir_update(root, || {
        let pending = pending_intents(root, identity)?;
        if !pending.is_empty() {
            return discard_all(root, &pending);
        }
        reject_non_regular(&root.join(&name))?;
        write_atomic_no_follow(root, &name, &serde_json::to_string(intent)?)
    })
}

pub(super) fn load_single_intent(
    root: &Path,
    identity: &Identity,
) -> Result<Option<PendingIntent>> {
    if !root.exists() {
        return Ok(None);
    }
    ensure_root(root)?;
    locked_dir_update(root, || {
        let pending = pending_intents(root, identity)?;
        if pending.len() == 1 {
            return Ok(pending.into_iter().next());
        }
        discard_all(root, &pending)?;
        Ok(None)
    })
}

pub(super) fn discard_pending(root: &Path, pending: &PendingIntent) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    ensure_root(root)?;
    locked_dir_update(root, || remove_existing(&root.join(&pending.name)))
}

pub(super) fn load_receipt(root: &Path, identity: &Identity) -> Result<Option<Receipt>> {
    if !root.exists() {
        return Ok(None);
    }
    ensure_root(root)?;
    load(root, &receipt_name(identity))
}

pub(super) fn complete(root: &Path, receipt: &Receipt, expected: &PendingIntent) -> Result<bool> {
    ensure_root(root)?;
    locked_dir_update(root, || {
        let pending = pending_intents(root, &receipt.identity)?;
        if pending.len() != 1 {
            discard_all(root, &pending)?;
            return Ok(false);
        }
        let pending = &pending[0];
        if pending.name != expected.name || pending.intent != expected.intent {
            return Ok(false);
        }
        let name = receipt_name(&receipt.identity);
        reject_non_regular(&root.join(&name))?;
        let next = match load_receipt(root, &receipt.identity)? {
            Some(prior) => merge_receipts(prior, receipt),
            None => receipt.clone(),
        };
        write_atomic_no_follow(root, &name, &serde_json::to_string(&next)?)?;
        remove_existing(&root.join(&pending.name))?;
        Ok(true)
    })
}

pub(super) fn read_epoch(root: &Path, session_id: &str) -> Result<String> {
    let path = root.join(epoch_name(session_id));
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("0".to_owned()),
        Err(error) => Err(error.into()),
        Ok(metadata) => {
            ensure!(
                metadata.file_type().is_file(),
                "receipt epoch is not regular"
            );
            let token = read_bounded_prefix(&path, MAX_EPOCH_BYTES + 1)?;
            ensure!(token.len() <= MAX_EPOCH_BYTES, "receipt epoch exceeds cap");
            ensure!(is_epoch_token(&token), "receipt epoch is malformed");
            Ok(token)
        }
    }
}

pub(super) fn rotate_epoch(root: &Path, session_id: &str, token: &str) -> Result<()> {
    ensure!(is_epoch_token(token), "receipt epoch is malformed");
    ensure_root(root)?;
    let name = epoch_name(session_id);
    locked_dir_update(root, || write_atomic_no_follow(root, &name, token))
}

fn pending_intents(root: &Path, identity: &Identity) -> Result<Vec<PendingIntent>> {
    let prefix = intent_prefix(identity);
    let mut pending = Vec::new();
    let entries = fs::read_dir(root)?;
    for entry in entries.take(MAX_DIRECTORY_ENTRIES) {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(&prefix) || !name.ends_with(".json") {
            continue;
        }
        let path = entry.path();
        reject_non_regular(&path)?;
        let Some(intent) = load::<Intent>(root, name)? else {
            continue;
        };
        if intent.identity == *identity {
            pending.push(PendingIntent {
                intent,
                name: name.to_owned(),
            });
        }
        if pending.len() >= MAX_PROVEN_TOOL_USES {
            break;
        }
    }
    Ok(pending)
}

fn discard_all(root: &Path, pending: &[PendingIntent]) -> Result<()> {
    for pending in pending {
        remove_existing(&root.join(&pending.name))?;
    }
    Ok(())
}

fn merge_receipts(prior: Receipt, next: &Receipt) -> Receipt {
    if prior.schema != RECEIPT_SCHEMA
        || prior.identity != next.identity
        || prior.generation != next.generation
    {
        return next.clone();
    }
    let mut ids = prior.tool_use_ids;
    for id in &next.tool_use_ids {
        if ids.len() < MAX_PROVEN_TOOL_USES && !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    Receipt {
        schema: RECEIPT_SCHEMA,
        identity: next.identity.clone(),
        generation: next.generation.clone(),
        content_class: merge_class(prior.content_class, next.content_class),
        result_hash: prior.result_hash,
        result_bytes: prior.result_bytes,
        tool_use_ids: ids,
        completed_at: next.completed_at.clone(),
    }
}

fn merge_class(left: ContentClass, right: ContentClass) -> ContentClass {
    if left == ContentClass::Media || right == ContentClass::Media {
        return ContentClass::Media;
    }
    ContentClass::Text
}

fn load<T: serde::de::DeserializeOwned>(root: &Path, name: &str) -> Result<Option<T>> {
    let path = root.join(name);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(metadata) => ensure!(
            metadata.file_type().is_file(),
            "receipt record is not regular"
        ),
    }
    let input = read_bounded_prefix(&path, MAX_RECORD_BYTES + 1)?;
    ensure!(
        input.len() <= MAX_RECORD_BYTES,
        "receipt record exceeds cap"
    );
    serde_json::from_str(&input)
        .map(Some)
        .context("decoding receipt record")
}

fn ensure_root(root: &Path) -> Result<()> {
    if !root.exists() {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        builder.mode(0o700);
        builder.create(root)?;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
    }
    let metadata = fs::symlink_metadata(root)?;
    ensure!(
        metadata.file_type().is_dir(),
        "receipt directory is not plain"
    );
    ensure!(
        metadata.uid() == effective_uid(),
        "receipt directory has another owner"
    );
    ensure!(
        metadata.permissions().mode() & 0o077 == 0,
        "receipt directory is exposed"
    );
    crate::fs::safe_write::safe_open_dirfd(root).map(|_| ())
}

fn write_atomic_no_follow(root: &Path, name: &str, content: &str) -> Result<()> {
    let dir = crate::fs::safe_write::safe_open_dirfd(root)?;
    let suffix = digest(uuid::Uuid::new_v4().as_bytes());
    let temporary = format!(".{name}.{suffix}.tmp");
    let temporary = Path::new(&temporary);
    crate::fs::safe_fs::safe_create_new_in_workdir(dir.as_raw_fd(), temporary, content.as_bytes())?;
    if let Err(error) =
        crate::fs::safe_fs::safe_rename_in_workdir(dir.as_raw_fd(), temporary, Path::new(name))
    {
        let _ = crate::fs::safe_write::safe_remove_in_workdir(dir.as_raw_fd(), temporary);
        return Err(error);
    }
    Ok(())
}

fn remove_existing(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(metadata) => {
            ensure!(
                metadata.file_type().is_file(),
                "receipt record is not regular"
            );
            fs::remove_file(path).map_err(Into::into)
        }
    }
}

fn reject_non_regular(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(metadata) => {
            ensure!(
                metadata.file_type().is_file(),
                "receipt record is not regular"
            );
            Ok(())
        }
    }
}

fn intent_name(identity: &Identity, nonce: &str) -> String {
    format!("{}{}.json", intent_prefix(identity), nonce)
}

fn intent_prefix(identity: &Identity) -> String {
    format!("intent-{}-", key(identity))
}

fn receipt_name(identity: &Identity) -> String {
    format!("receipt-{}.json", key(identity))
}

fn epoch_name(session_id: &str) -> String {
    format!("epoch-{}.token", digest(session_id.as_bytes()))
}

fn key(identity: &Identity) -> String {
    let value = format!(
        "{}\0{}\0{}\0{}",
        identity.session_id, identity.agent_id, identity.path, identity.range
    );
    digest(value.as_bytes())
}

fn is_epoch_token(token: &str) -> bool {
    (1..=MAX_EPOCH_BYTES).contains(&token.len())
        && token.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

fn effective_uid() -> u32 {
    // SAFETY: geteuid has no arguments and only reads process credentials.
    unsafe { libc::geteuid() }
}
