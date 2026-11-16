//! Project-tier Codex worker backend selection.

use std::path::Path;

use super::allowed::allowed_value;

/// Project overrides for the Codex worker backend.
#[derive(Debug, Clone, Default, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CodexConfig {
    backend: Option<String>,
}

impl CodexConfig {
    /// Parse the body of a `[codex]` section; `read_section` does this in production.
    #[cfg(test)]
    pub fn from_section_toml(body: &str) -> anyhow::Result<Self> {
        toml::from_str(body)
            .map_err(|e| anyhow::anyhow!("failed to parse [codex] section body: {e}"))
    }

    /// Return the configured value if it is supported.
    pub fn backend(&self) -> Option<&str> {
        allowed_value(
            self.backend.as_ref(),
            crate::user_config::CODEX_BACKENDS,
            "codex.backend",
        )
    }
}

/// Read the project override, falling back when absent or unreadable.
pub fn read_codex_config(work_dir: &Path) -> CodexConfig {
    super::read_section::<CodexConfig>(work_dir, "codex")
        .ok()
        .flatten()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessor_returns_set_value_and_drops_invalid_value() {
        let valid = CodexConfig::from_section_toml("backend = \"native\"").unwrap();
        let invalid = CodexConfig::from_section_toml("backend = \"bogus\"").unwrap();
        assert_eq!(valid.backend(), Some("native"));
        assert_eq!(invalid.backend(), None);
    }

    #[test]
    fn unknown_key_makes_section_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.toml"),
            "[codex]\nbackend = \"native\"\nunknown = 1\n",
        )
        .unwrap();
        assert_eq!(read_codex_config(dir.path()), CodexConfig::default());
    }

    #[test]
    fn missing_file_yields_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_codex_config(dir.path()), CodexConfig::default());
    }
}
