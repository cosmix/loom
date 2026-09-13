use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::models::forward_receipt::{is_safe_id, parse_canonical_utc_millis};

const MAX_PATH_BYTES: usize = 4096;
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
        validate_path(&row.workspace_root, "workspace_root")?;
        validate_path(&row.companion_path, "companion_path")?;
        validate_path(&row.state_root, "state_root")?;
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

fn validate_path(path: &Path, name: &str) -> Result<()> {
    let text = path
        .to_str()
        .with_context(|| format!("Codex authorization {name} is not UTF-8"))?;
    ensure!(
        path.is_absolute(),
        "Codex authorization {name} is not absolute"
    );
    ensure!(
        text.len() <= MAX_PATH_BYTES,
        "Codex authorization {name} exceeds cap"
    );
    ensure!(
        !path.components().any(|part| matches!(
            part,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )),
        "Codex authorization {name} is not normalized"
    );
    Ok(())
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

fn validate_text(value: &str, max: usize, name: &str) -> Result<()> {
    ensure!(
        (1..=max).contains(&value.len()),
        "invalid Codex authorization {name}"
    );
    ensure!(
        !value.as_bytes().contains(&0),
        "NUL in Codex authorization {name}"
    );
    Ok(())
}
