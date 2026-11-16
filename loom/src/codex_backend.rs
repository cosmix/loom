//! Backend selection and availability of Codex stage worker lanes.

use std::path::Path;

use super::{codex_lane_status, find_codex_path, CODEX_CLI_MISSING};
use crate::user_config::UserConfig;

/// Configured Codex worker backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodexBackend {
    #[default]
    Legacy,
    Native,
}

/// Available launch lane for a stage that licenses Codex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexLane {
    Unavailable(String),
    Legacy,
    Native,
}

impl Default for CodexLane {
    fn default() -> Self {
        Self::Unavailable(String::new())
    }
}

/// Resolve the project setting over the user setting and built-in default.
pub fn configured_backend(work_dir: &Path) -> CodexBackend {
    let backend = crate::fs::work_dir::config_sections::read_codex_config(work_dir)
        .backend()
        .map(str::to_owned)
        .unwrap_or_else(|| UserConfig::load().codex_backend().to_owned());
    match backend.as_str() {
        "native" => CodexBackend::Native,
        _ => CodexBackend::Legacy,
    }
}

pub(super) fn lane_from(
    backend: CodexBackend,
    cli_found: bool,
    legacy_status: Result<(), String>,
    qualified: impl FnOnce() -> Result<(), String>,
) -> CodexLane {
    match backend {
        CodexBackend::Legacy => match legacy_status {
            Ok(()) => CodexLane::Legacy,
            Err(reason) => CodexLane::Unavailable(reason),
        },
        CodexBackend::Native if !cli_found => CodexLane::Unavailable(CODEX_CLI_MISSING.to_owned()),
        CodexBackend::Native => match qualified() {
            Ok(()) => CodexLane::Native,
            Err(reason) => CodexLane::Unavailable(reason),
        },
    }
}

/// Select the configured lane only when its requirements are available.
pub fn codex_lane(work_dir: &Path) -> CodexLane {
    lane_from(
        configured_backend(work_dir),
        find_codex_path().is_ok(),
        codex_lane_status().map_err(|error| error.to_string()),
        crate::native_worker::boundary::native_backend_qualified,
    )
}
