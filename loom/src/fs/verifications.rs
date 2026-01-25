//! Verification result persistence
//!
//! Stores goal-backward verification results for stages.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Record of a verification run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRecord {
    /// Stage that was verified
    pub stage_id: String,
    /// When verification was run
    pub timestamp: DateTime<Utc>,
    /// Whether all verifications passed
    pub passed: bool,
    /// Individual gap records
    pub gaps: Vec<GapRecord>,
}

/// Record of a single verification gap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapRecord {
    /// Type of gap (TruthFailed, ArtifactMissing, etc.)
    pub gap_type: String,
    /// Description of the gap
    pub description: String,
    /// Suggested fix
    pub suggestion: String,
}

impl VerificationRecord {
    /// Create a new verification record
    pub fn new(stage_id: &str, passed: bool, gaps: Vec<GapRecord>) -> Self {
        Self {
            stage_id: stage_id.to_string(),
            timestamp: Utc::now(),
            passed,
            gaps,
        }
    }
}

/// Store a verification result
pub fn store_verification(
    stage_id: &str,
    record: &VerificationRecord,
    work_dir: &Path,
) -> Result<()> {
    let verifications_dir = work_dir.join("verifications");
    fs::create_dir_all(&verifications_dir)
        .context("Failed to create verifications directory")?;

    let path = verifications_dir.join(format!("{stage_id}.json"));
    let json = serde_json::to_string_pretty(record)
        .context("Failed to serialize verification record")?;
    fs::write(&path, json)
        .with_context(|| format!("Failed to write verification record: {}", path.display()))?;

    Ok(())
}

/// Load a verification result
pub fn load_verification(stage_id: &str, work_dir: &Path) -> Result<Option<VerificationRecord>> {
    let path = work_dir.join("verifications").join(format!("{stage_id}.json"));

    if !path.exists() {
        return Ok(None);
    }

    let json = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read verification record: {}", path.display()))?;
    let record = serde_json::from_str(&json)
        .with_context(|| format!("Failed to parse verification record: {}", path.display()))?;

    Ok(Some(record))
}

/// List all verification records
pub fn list_verifications(work_dir: &Path) -> Result<Vec<VerificationRecord>> {
    let verifications_dir = work_dir.join("verifications");

    if !verifications_dir.exists() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    for entry in fs::read_dir(&verifications_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            let json = fs::read_to_string(&path)?;
            if let Ok(record) = serde_json::from_str::<VerificationRecord>(&json) {
                records.push(record);
            }
        }
    }

    // Sort by timestamp descending
    records.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(records)
}

/// Delete a verification record
pub fn delete_verification(stage_id: &str, work_dir: &Path) -> Result<()> {
    let path = work_dir.join("verifications").join(format!("{stage_id}.json"));
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to delete verification record: {}", path.display()))?;
    }
    Ok(())
}
