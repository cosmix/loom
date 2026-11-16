//! User-tier Codex worker backend selection.

use anyhow::Result;
use toml_edit::DocumentMut;

use super::{parse, ConfigValue, KeySpec, Origin, UserConfig};

/// Supported Codex worker backends.
pub const CODEX_BACKENDS: &[&str] = &["legacy", "native"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct CodexSection {
    backend: Option<String>,
}

impl CodexSection {
    pub(super) fn parse(doc: &DocumentMut) -> Result<Self> {
        Ok(Self {
            backend: parse::get_enum(doc, "codex", "backend", CODEX_BACKENDS)?,
        })
    }
}

impl UserConfig {
    /// Configured backend, defaulting to legacy forwarders.
    pub fn codex_backend(&self) -> &str {
        self.codex_backend_set().unwrap_or("legacy")
    }

    /// The backend explicitly set in the user config, if any.
    pub fn codex_backend_set(&self) -> Option<&str> {
        self.codex.backend.as_deref()
    }

    pub(super) fn codex_value_of(&self, spec: &KeySpec) -> Option<(ConfigValue, Origin)> {
        Some(match spec.name {
            "codex.backend" => (
                ConfigValue::Text(self.codex_backend().to_owned()),
                self.origin_of(self.codex.backend.as_ref()),
            ),
            _ => return None,
        })
    }

    pub(super) fn codex_toml(&self) -> String {
        format!(
            "[codex]\nbackend = {}\n",
            ConfigValue::Text(self.codex_backend().to_owned()).to_toml_literal()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_config::{keys::spec, parse_document};

    #[test]
    fn default_backend_is_legacy() {
        let config = UserConfig::default();
        assert_eq!(config.codex_backend(), "legacy");
        assert_eq!(config.codex_backend_set(), None);
    }

    #[test]
    fn native_is_parsed_and_marked_set() {
        let config = parse_document("[codex]\nbackend = \"native\"\n").unwrap();
        assert_eq!(config.codex_backend(), "native");
        assert_eq!(config.codex_backend_set(), Some("native"));
        assert_eq!(
            config.value_of(spec("codex.backend").unwrap()).1,
            Origin::Set
        );
    }

    #[test]
    fn unknown_backend_names_the_key() {
        assert!(parse_document("[codex]\nbackend = \"bogus\"\n")
            .unwrap_err()
            .to_string()
            .contains("codex.backend"));
    }

    #[test]
    fn to_toml_string_renders_the_section_first() {
        let text = UserConfig::default().to_toml_string();
        assert!(text.starts_with("[codex]\nbackend = \"legacy\"\n"));
        assert!(text.find("[codex]").unwrap() < text.find("[context]").unwrap());
    }
}
