//! Execution backend and runtime configuration types.
//!
//! Defines the canonical `BackendType` enum, plan-level execution preferences,
//! per-stage backend overrides, and the project-level execution config
//! persisted to `.work/config.toml`.
//!
//! Layering:
//! - `BackendType` — the single canonical backend identifier (re-exported by
//!   `orchestrator::terminal`).
//! - `PlanExecutionConfig` — plan-level execution preferences declared in the
//!   plan YAML.
//! - `StageExecutionConfig` — per-stage backend override.
//! - `ProjectExecutionConfig` — project-level backend selection persisted to
//!   `.work/config.toml` (chosen at `loom init` time).

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Selects the terminal/runtime backend used to execute a stage session.
///
/// This is the SINGLE canonical definition; `orchestrator::terminal`
/// re-exports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendType {
    /// Native terminal windows — each session in its own terminal emulator.
    #[default]
    Native,
}

impl fmt::Display for BackendType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendType::Native => write!(f, "native"),
        }
    }
}

impl FromStr for BackendType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "native" => Ok(BackendType::Native),
            other => Err(anyhow!(
                "Unknown backend type: '{other}'. Expected 'native'"
            )),
        }
    }
}

/// Plan-level execution configuration declared in plan YAML.
///
/// Note: backend selection lives at the project level (`.work/config.toml`,
/// see [`ProjectExecutionConfig`]). Retained as scaffolding for future
/// plan-level execution preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanExecutionConfig {}

/// Per-stage execution configuration. Overrides project defaults when set.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StageExecutionConfig {
    /// Optional backend override for this stage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<BackendType>,
}

/// Project-level execution configuration persisted to `.work/config.toml`
/// under the `[project_execution]` table.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectExecutionConfig {
    /// Selected backend for this project.
    #[serde(default)]
    pub backend: BackendType,
}

/// Maximum length permitted for a git identity value (`user.name` /
/// `user.email`). Git's own object model has no formal limit, but values
/// beyond this are not legitimate operator input and the cap makes
/// downstream env-injection and TOML round-trip predictable.
pub const GIT_IDENTITY_MAX_LEN: usize = 256;

/// Validate a git identity value used for `GIT_AUTHOR_*` / `GIT_COMMITTER_*`
/// env injection. Rejects empty strings, control characters (including the
/// embedded newlines that produce malformed commit objects), and values
/// longer than [`GIT_IDENTITY_MAX_LEN`]. The caller treats a rejection as
/// "fall back to git defaults" (drops the value to `None`).
pub fn validate_git_identity(value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(anyhow!("git identity value is empty"));
    }
    if value.len() > GIT_IDENTITY_MAX_LEN {
        return Err(anyhow!(
            "git identity value exceeds {GIT_IDENTITY_MAX_LEN} bytes ({} bytes given)",
            value.len()
        ));
    }
    if let Some(c) = value.chars().find(|c| c.is_control()) {
        return Err(anyhow!(
            "git identity value contains control character U+{:04X}",
            c as u32
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_type_display() {
        assert_eq!(BackendType::Native.to_string(), "native");
    }

    #[test]
    fn backend_type_from_str() {
        assert_eq!(
            "native".parse::<BackendType>().unwrap(),
            BackendType::Native
        );
        assert_eq!(
            "Native".parse::<BackendType>().unwrap(),
            BackendType::Native
        );
        // "container" is no longer a valid backend; it must fail to parse.
        assert!("container".parse::<BackendType>().is_err());
        assert!("invalid".parse::<BackendType>().is_err());
    }

    #[test]
    fn backend_type_default_is_native() {
        assert_eq!(BackendType::default(), BackendType::Native);
    }

    #[test]
    fn backend_type_serde_kebab_case() {
        let yaml = serde_yaml::to_string(&BackendType::Native).unwrap();
        assert!(yaml.contains("native"));
        let back: BackendType = serde_yaml::from_str("native").unwrap();
        assert_eq!(back, BackendType::Native);
    }

    #[test]
    fn plan_execution_config_round_trip() {
        let cfg = PlanExecutionConfig::default();
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let back: PlanExecutionConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn stage_execution_config_round_trip() {
        let cfg = StageExecutionConfig {
            backend: Some(BackendType::Native),
        };
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let back: StageExecutionConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, cfg);
        assert!(yaml.contains("backend: native"));
    }

    #[test]
    fn project_execution_config_round_trip() {
        let cfg = ProjectExecutionConfig {
            backend: BackendType::Native,
        };
        let toml_str = toml::to_string(&cfg).unwrap();
        let back: ProjectExecutionConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn project_execution_config_default_is_native() {
        let cfg = ProjectExecutionConfig::default();
        assert_eq!(cfg.backend, BackendType::Native);
    }

    #[test]
    fn validate_git_identity_accepts_normal_values() {
        validate_git_identity("Alice Dev").unwrap();
        validate_git_identity("alice@example.com").unwrap();
        validate_git_identity("Renée O'Brien").unwrap();
    }

    #[test]
    fn validate_git_identity_rejects_empty() {
        assert!(validate_git_identity("").is_err());
    }

    #[test]
    fn validate_git_identity_rejects_newline() {
        assert!(validate_git_identity("Alice\nGIT_PASSWORD=hunter2").is_err());
        assert!(validate_git_identity("Alice\rDev").is_err());
    }

    #[test]
    fn validate_git_identity_rejects_nul() {
        assert!(validate_git_identity("Alice\0Dev").is_err());
    }

    #[test]
    fn validate_git_identity_rejects_oversize() {
        let huge = "a".repeat(GIT_IDENTITY_MAX_LEN + 1);
        assert!(validate_git_identity(&huge).is_err());
        let max = "a".repeat(GIT_IDENTITY_MAX_LEN);
        validate_git_identity(&max).unwrap();
    }
}
