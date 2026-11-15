use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::agent_state::is_safe_id;
use crate::models::forward_receipt::job_record::{validate_path, validate_text};
use crate::models::forward_receipt::parse_canonical_utc_millis;

const SUPPORTED_COMPANIONS: [&str; 1] = ["1.0.6"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexAuthorization {
    pub stage_id: String,
    pub loom_session_id: String,
    pub parent_session_id: String,
    pub forwarder_agent_id: String,
    pub unit_id: String,
    pub invocation_id: String,
    pub model: String,
    pub effort: String,
    pub authorized_at: DateTime<Utc>,
    pub selected_companion: PathBuf,
    pub companion_version: String,
    pub effective_state_root: PathBuf,
    pub workspace_root: PathBuf,
    pub tool_use_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorizationRow {
    v: u16,
    ts: String,
    stage_id: String,
    session_id: String,
    parent_session_id: String,
    forwarder_agent_id: String,
    tool_use_id: String,
    unit_id: String,
    invocation_id: String,
    model: String,
    effort: String,
    workspace_root: PathBuf,
    companion_version: String,
    companion_path: PathBuf,
    state_root: PathBuf,
}

impl CodexAuthorization {
    pub fn from_v2_value(value: &Value) -> Result<Self> {
        let row: AuthorizationRow = serde_json::from_value(value.clone())?;
        ensure!(row.v == 2, "unsupported Codex authorization version");
        validate_ids(&row)?;
        validate_invocation_id(&row.invocation_id)?;
        validate_text(&row.model, 128, "model")?;
        validate_text(&row.effort, 128, "effort")?;
        ensure!(
            SUPPORTED_COMPANIONS.contains(&row.companion_version.as_str()),
            "unsupported Codex companion version"
        );
        validate_path(Some(&row.workspace_root), "workspace_root")?;
        validate_path(Some(&row.companion_path), "companion_path")?;
        validate_path(Some(&row.state_root), "state_root")?;
        Ok(Self {
            stage_id: row.stage_id,
            loom_session_id: row.session_id,
            parent_session_id: row.parent_session_id,
            forwarder_agent_id: row.forwarder_agent_id,
            unit_id: row.unit_id,
            invocation_id: row.invocation_id,
            model: row.model,
            effort: row.effort,
            authorized_at: parse_authorized_at(&row.ts)?,
            selected_companion: row.companion_path,
            companion_version: row.companion_version,
            effective_state_root: row.state_root,
            workspace_root: row.workspace_root,
            tool_use_id: row.tool_use_id,
        })
    }

    pub fn encoded_session_id(&self) -> String {
        format!(
            "loom.v1:{}:{}:{}:{}",
            self.stage_id, self.loom_session_id, self.unit_id, self.invocation_id
        )
    }

    pub(super) fn validate_state_root(&self, expected: &Path) -> Result<()> {
        validate_canonical_path(&self.workspace_root, false, "workspace_root")?;
        validate_canonical_path(&self.selected_companion, true, "companion_path")?;
        validate_canonical_path(&self.effective_state_root, false, "state_root")?;
        ensure!(
            self.effective_state_root == expected,
            "authorization state root differs from daemon state root"
        );
        Ok(())
    }
}

pub(super) fn canonical_daemon_state_root() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is unavailable")?;
    let canonical_home = fs::canonicalize(PathBuf::from(home)).context("canonicalizing HOME")?;
    let state_root = canonical_home.join(".codex/plugin-data/state");
    if state_root.exists() {
        return fs::canonicalize(state_root).context("canonicalizing Codex state root");
    }
    Ok(state_root)
}

fn validate_ids(row: &AuthorizationRow) -> Result<()> {
    for (value, name) in [
        (&row.stage_id, "stage_id"),
        (&row.session_id, "session_id"),
        (&row.parent_session_id, "parent_session_id"),
        (&row.forwarder_agent_id, "forwarder_agent_id"),
        (&row.tool_use_id, "tool_use_id"),
        (&row.unit_id, "unit_id"),
    ] {
        ensure!(is_safe_id(value), "unsafe Codex authorization {name}");
    }
    Ok(())
}

fn validate_invocation_id(value: &str) -> Result<()> {
    let suffix = value
        .strip_prefix("inv-")
        .context("Codex invocation id lacks inv- prefix")?;
    ensure!(
        suffix.len() == 32
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "invalid Codex invocation nonce"
    );
    Ok(())
}

fn parse_authorized_at(value: &str) -> Result<DateTime<Utc>> {
    parse_canonical_utc_millis(value).context("invalid Codex authorization timestamp")
}

fn validate_canonical_path(path: &Path, file: bool, name: &str) -> Result<()> {
    let metadata =
        fs::metadata(path).with_context(|| format!("reading Codex authorization {name}"))?;
    ensure!(
        if file {
            metadata.is_file()
        } else {
            metadata.is_dir()
        },
        "Codex authorization {name} has wrong type"
    );
    ensure!(
        fs::canonicalize(path)? == path,
        "Codex authorization {name} is not canonical"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_row() -> Value {
        json!({
            "v": 2,
            "ts": "2026-09-14T10:00:00.000Z",
            "stage_id": "stage-1",
            "session_id": "session-1",
            "parent_session_id": "parent-1",
            "forwarder_agent_id": "agent-1",
            "tool_use_id": "tool-1",
            "unit_id": "unit-1",
            "invocation_id": "inv-0123456789abcdef0123456789abcdef",
            "model": "model",
            "effort": "high",
            "workspace_root": "/workspace",
            "companion_version": "1.0.6",
            "companion_path": "/plugin/companion.mjs",
            "state_root": "/state"
        })
    }

    #[test]
    fn authorization_paths_use_shared_validation() {
        CodexAuthorization::from_v2_value(&valid_row()).expect("valid authorization");
        let over_cap = format!("/{}", "x".repeat(4096));
        for field in ["workspace_root", "companion_path", "state_root"] {
            for (path, reason) in [
                ("/workspace\0tail", format!("NUL in companion {field}")),
                ("relative", format!("companion {field} is not absolute")),
                (
                    "/workspace/../elsewhere",
                    format!("companion {field} is not normalized"),
                ),
                (
                    over_cap.as_str(),
                    format!("invalid companion {field} length"),
                ),
            ] {
                let mut row = valid_row();
                row[field] = json!(path);
                assert_eq!(
                    CodexAuthorization::from_v2_value(&row)
                        .unwrap_err()
                        .to_string(),
                    reason
                );
            }
        }
    }

    #[test]
    fn authorization_text_uses_shared_errors_and_limits() {
        for field in ["model", "effort"] {
            let mut row = valid_row();
            row[field] = json!("x".repeat(128));
            CodexAuthorization::from_v2_value(&row).expect("text at byte cap");
            for (value, reason) in [
                (String::new(), format!("invalid companion {field} length")),
                ("é".repeat(65), format!("invalid companion {field} length")),
                ("x\0y".into(), format!("NUL in companion {field}")),
            ] {
                row[field] = json!(value);
                assert_eq!(
                    CodexAuthorization::from_v2_value(&row)
                        .unwrap_err()
                        .to_string(),
                    reason
                );
            }
        }
    }
}
