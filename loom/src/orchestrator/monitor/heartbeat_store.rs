//! Heartbeat file storage helpers.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::heartbeat::{Heartbeat, JUDGE_STEM_SUFFIX};

/// Read a heartbeat file.
pub fn read_heartbeat(path: &Path) -> Result<Heartbeat> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read heartbeat file: {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse heartbeat file: {}", path.display()))
}

/// Write a heartbeat file.
pub fn write_heartbeat(work_dir: &Path, heartbeat: &Heartbeat) -> Result<PathBuf> {
    let heartbeat_dir = work_dir.join("heartbeat");
    if !heartbeat_dir.exists() {
        std::fs::create_dir_all(&heartbeat_dir).with_context(|| {
            format!(
                "Failed to create heartbeat directory: {}",
                heartbeat_dir.display()
            )
        })?;
    }

    let path = heartbeat_dir.join(format!("{}.json", heartbeat.stage_id));
    let content =
        serde_json::to_string_pretty(heartbeat).context("Failed to serialize heartbeat")?;
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write heartbeat file: {}", path.display()))?;
    Ok(path)
}

/// Remove a heartbeat file.
pub fn remove_heartbeat(work_dir: &Path, stage_id: &str) -> Result<()> {
    let path = heartbeat_path(work_dir, stage_id);
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove heartbeat file: {}", path.display()))?;
    }
    Ok(())
}

/// Get the heartbeat path for a stage.
pub fn heartbeat_path(work_dir: &Path, stage_id: &str) -> PathBuf {
    work_dir.join("heartbeat").join(format!("{stage_id}.json"))
}

/// Get the distinct adjudication-heartbeat path for a stage.
pub fn judge_heartbeat_path(work_dir: &Path, stage_id: &str) -> PathBuf {
    work_dir
        .join("heartbeat")
        .join(format!("{stage_id}{JUDGE_STEM_SUFFIX}.json"))
}

/// Remove a stage's judge heartbeat file if it exists.
pub fn cleanup_judge_heartbeat(work_dir: &Path, stage_id: &str) {
    let path = judge_heartbeat_path(work_dir, stage_id);
    if let Err(error) = std::fs::remove_file(&path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                target: "loom::adjudication",
                stage = %stage_id,
                path = %path.display(),
                %error,
                "failed to remove the judge heartbeat",
            );
        }
    }
}

/// Read the latest resident-token measurement for a stage.
pub fn stage_context_tokens(work_dir: &Path, stage_id: &str) -> Option<u32> {
    read_heartbeat(&heartbeat_path(work_dir, stage_id))
        .ok()
        .and_then(|heartbeat| heartbeat.context_tokens)
}
