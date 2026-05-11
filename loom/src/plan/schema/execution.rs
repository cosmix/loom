//! Execution backend and runtime configuration types.
//!
//! Defines the canonical `BackendType` enum, plan-level execution preferences
//! (container forwarding, additional mounts), per-stage backend overrides, and
//! the project-level execution config persisted to `.work/config.toml`.
//!
//! Layering:
//! - `BackendType` — the single canonical backend identifier (re-exported by
//!   `orchestrator::terminal`).
//! - `PlanExecutionConfig` / `PlanContainerConfig` — plan-level container
//!   preferences declared in the plan YAML.
//! - `StageExecutionConfig` — per-stage backend override.
//! - `ProjectExecutionConfig` / `ProjectContainerConfig` — project-level
//!   selection persisted to `.work/config.toml` (chosen at `loom init` /
//!   container provisioning time).

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
    /// Containerised execution — sessions run inside a managed container.
    Container,
}

impl fmt::Display for BackendType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendType::Native => write!(f, "native"),
            BackendType::Container => write!(f, "container"),
        }
    }
}

impl FromStr for BackendType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "native" => Ok(BackendType::Native),
            "container" => Ok(BackendType::Container),
            other => Err(anyhow!(
                "Unknown backend type: '{other}'. Expected 'native' or 'container'"
            )),
        }
    }
}

/// Plan-level execution configuration declared in plan YAML.
///
/// Note: backend selection lives at the project level (`.work/config.toml`,
/// see [`ProjectExecutionConfig`]). The plan only declares container-runtime
/// preferences (forwarded credentials, additional bind mounts).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanExecutionConfig {
    /// Container-runtime preferences when the project backend is `Container`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<PlanContainerConfig>,
}

/// Container preferences declared at the plan level.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanContainerConfig {
    /// Credential paths/identifiers to forward into the container (e.g.,
    /// `"~/.aws/credentials"`, `"GH_TOKEN"`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forward_credentials: Vec<String>,
    /// Additional host paths to bind-mount into the container.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_mounts: Vec<String>,
}

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
    /// Container metadata (only meaningful when `backend == Container`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<ProjectContainerConfig>,
}

/// Project-level container metadata persisted to `.work/config.toml`.
///
/// Captures which container runtime is in use and which image is currently
/// provisioned, so the daemon can detect drift between provisioning runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectContainerConfig {
    /// Container runtime binary (e.g., `"docker"`, `"podman"`).
    pub runtime: String,
    /// Fingerprint of the inputs that produced the current image
    /// (Dockerfile + build args). Used to detect when a rebuild is needed.
    pub fingerprint: String,
    /// Digest (`sha256:...`) of the image currently provisioned for the project.
    pub image_digest: String,
    /// Credentials forwarded at provisioning time (frozen for audit).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forward_credentials: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_type_display() {
        assert_eq!(BackendType::Native.to_string(), "native");
        assert_eq!(BackendType::Container.to_string(), "container");
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
        assert_eq!(
            "container".parse::<BackendType>().unwrap(),
            BackendType::Container
        );
        assert_eq!(
            "CONTAINER".parse::<BackendType>().unwrap(),
            BackendType::Container
        );
        assert!("invalid".parse::<BackendType>().is_err());
    }

    #[test]
    fn backend_type_default_is_native() {
        assert_eq!(BackendType::default(), BackendType::Native);
    }

    #[test]
    fn backend_type_serde_kebab_case() {
        // Container should serialise as "container" (kebab-case applied to
        // single-word variants is identity).
        let yaml = serde_yaml::to_string(&BackendType::Container).unwrap();
        assert!(yaml.contains("container"));
        let back: BackendType = serde_yaml::from_str("container").unwrap();
        assert_eq!(back, BackendType::Container);
    }

    #[test]
    fn plan_execution_config_round_trip() {
        let cfg = PlanExecutionConfig {
            container: Some(PlanContainerConfig {
                forward_credentials: vec!["GH_TOKEN".to_string()],
                additional_mounts: vec!["/tmp/work:/work".to_string()],
            }),
        };
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let back: PlanExecutionConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn plan_execution_config_omits_empty_container() {
        let cfg = PlanExecutionConfig::default();
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        assert!(!yaml.contains("container"));
    }

    #[test]
    fn stage_execution_config_round_trip() {
        let cfg = StageExecutionConfig {
            backend: Some(BackendType::Container),
        };
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let back: StageExecutionConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, cfg);
        assert!(yaml.contains("backend: container"));
    }

    #[test]
    fn project_execution_config_round_trip() {
        let cfg = ProjectExecutionConfig {
            backend: BackendType::Container,
            container: Some(ProjectContainerConfig {
                runtime: "docker".to_string(),
                fingerprint: "abc123".to_string(),
                image_digest: "sha256:deadbeef".to_string(),
                forward_credentials: vec!["GH_TOKEN".to_string()],
            }),
        };
        let toml_str = toml::to_string(&cfg).unwrap();
        let back: ProjectExecutionConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn project_execution_config_default_is_native_no_container() {
        let cfg = ProjectExecutionConfig::default();
        assert_eq!(cfg.backend, BackendType::Native);
        assert!(cfg.container.is_none());
    }
}
