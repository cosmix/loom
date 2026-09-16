//! [`ConfigValue`]: a `~/.loom/config.toml` key's value in its own type
//! rather than its rendering.
//!
//! Before this module, [`super::UserConfig::value_of`] collapsed every key's
//! value to a `String` at one function, so every surface downstream — `loom
//! config -k`, the TUI, the dashboard API — inherited a string regardless of
//! whether the key was really a bool, an integer, or an enum. [`ConfigValue`]
//! keeps the read path typed end to end, matching the write path
//! ([`super::keys::KeySpec::parse`]), which already produced a typed
//! `toml_edit::Value`.

use anyhow::{bail, Result};

use super::keys::ValueKind;

/// A config value in its own type rather than its rendering.
///
/// `#[serde(untagged)]` makes the JSON wire native: `Bool(true)` serializes
/// as `true`, `Number(24)` as `24`, `Text("opus")` as `"opus"`. The variant
/// order matters — untagged deserialization tries variants in declaration
/// order, and a JSON `true` must not be considered for `Number` first.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum ConfigValue {
    Bool(bool),
    Number(u32),
    Text(String),
}

impl ConfigValue {
    /// Parse an operator-supplied string into the value `kind` accepts,
    /// erroring with `name`, the offending text, and the expected type.
    pub fn parse(kind: &ValueKind, name: &str, raw: &str) -> Result<Self> {
        match kind {
            ValueKind::Bool => Self::parse_bool(name, raw),
            ValueKind::Number => Self::parse_number(name, raw),
            ValueKind::Enum(variants) => Self::parse_enum(name, raw, variants),
            ValueKind::String => Ok(Self::Text(raw.to_owned())),
        }
    }

    fn parse_bool(name: &str, raw: &str) -> Result<Self> {
        raw.parse::<bool>()
            .map(Self::Bool)
            .map_err(|_| Self::bool_error(name, raw))
    }

    fn parse_number(name: &str, raw: &str) -> Result<Self> {
        raw.parse::<u32>()
            .map(Self::Number)
            .map_err(|_| Self::number_error(name, raw))
    }

    fn parse_enum(name: &str, raw: &str, variants: &[&str]) -> Result<Self> {
        if variants.contains(&raw) {
            Ok(Self::Text(raw.to_owned()))
        } else {
            Err(Self::enum_error(name, raw, variants))
        }
    }

    /// Error message for a value that fails [`Self::parse_bool`] or a
    /// [`Self::checked`] mismatch against [`ValueKind::Bool`] — the two paths
    /// share this so their wording cannot drift apart.
    fn bool_error(name: &str, raw: &str) -> anyhow::Error {
        anyhow::anyhow!("{name}: {raw:?} is not a bool (expected true or false)")
    }

    /// Error message for a value that fails [`Self::parse_number`] or a
    /// [`Self::checked`] mismatch against [`ValueKind::Number`].
    fn number_error(name: &str, raw: &str) -> anyhow::Error {
        anyhow::anyhow!("{name}: {raw:?} is not a u32 (expected a non-negative integer)")
    }

    /// Error message for a value that fails [`Self::parse_enum`] or a
    /// [`Self::checked`] mismatch against [`ValueKind::Enum`].
    fn enum_error(name: &str, raw: &str, variants: &[&str]) -> anyhow::Error {
        anyhow::anyhow!(
            "{name}: {raw:?} is not one of the expected values: {}",
            variants.join(", ")
        )
    }

    /// Error message for a [`Self::checked`] mismatch against
    /// [`ValueKind::String`]. [`Self::parse`] never fails for `String`
    /// (any text is valid), but a non-`Text` value (`Bool`/`Number`) checked
    /// against `String` still needs a rejection in the same wording shape.
    fn string_error(name: &str, raw: &str) -> anyhow::Error {
        anyhow::anyhow!("{name}: {raw:?} is not a string")
    }

    /// Convert a value already parsed out of a workspace config's
    /// `toml::Value` (the `toml` crate, not `toml_edit`) into the shape `kind`
    /// accepts. A shape mismatch is always an error naming `name` and the
    /// TOML type found — never a silent `None`.
    pub fn from_toml_value(kind: &ValueKind, name: &str, value: &toml::Value) -> Result<Self> {
        match kind {
            ValueKind::Bool => value
                .as_bool()
                .map(Self::Bool)
                .ok_or_else(|| Self::shape_error(name, "a bool", value)),
            ValueKind::Number => Self::number_from_toml(name, value),
            ValueKind::Enum(variants) => Self::enum_from_toml(name, value, variants),
            ValueKind::String => value
                .as_str()
                .map(|s| Self::Text(s.to_owned()))
                .ok_or_else(|| Self::shape_error(name, "a string", value)),
        }
    }

    fn shape_error(name: &str, expected: &str, value: &toml::Value) -> anyhow::Error {
        anyhow::anyhow!("{name}: expected {expected}, found {}", value.type_str())
    }

    fn number_from_toml(name: &str, value: &toml::Value) -> Result<Self> {
        let int = value
            .as_integer()
            .ok_or_else(|| Self::shape_error(name, "an integer", value))?;
        u32::try_from(int)
            .map(Self::Number)
            .map_err(|_| anyhow::anyhow!("{name}: {int} is out of range for a u32"))
    }

    fn enum_from_toml(name: &str, value: &toml::Value, variants: &[&str]) -> Result<Self> {
        let raw = value
            .as_str()
            .ok_or_else(|| Self::shape_error(name, "a string", value))?;
        if variants.contains(&raw) {
            Ok(Self::Text(raw.to_owned()))
        } else {
            bail!(
                "{name}: {raw:?} is not one of the expected values: {}",
                variants.join(", ")
            )
        }
    }

    /// Validate a value that arrived already-deserialized (from JSON, via a
    /// dashboard write) against `kind`: the variant must match
    /// (`Bool`↔`Bool`, `Number`↔`Number`, `Text`↔`Enum`/`String`) and, for
    /// `Enum`, the text must be a listed variant.
    ///
    /// On a mismatch this errors — never coerces, even when this value's
    /// [`Display`] rendering happens to reparse as `kind` (`Text("false")`
    /// against `Bool` is still rejected). For `Bool`, `Number`, and `Enum`
    /// this reproduces the exact message [`Self::parse`] would have produced
    /// for the same `kind` given that rendering as the raw text — so a
    /// dashboard client sending `42` for `terminal.backend` gets "is not one
    /// of the expected values: native, tmux" rather than a distinct second
    /// wording. `String` has no `parse` failure to match, so the message is
    /// its own.
    ///
    /// [`Display`]: std::fmt::Display
    pub fn checked(self, kind: &ValueKind, name: &str) -> Result<Self> {
        let matches = match (&self, kind) {
            (Self::Bool(_), ValueKind::Bool) => true,
            (Self::Number(_), ValueKind::Number) => true,
            (Self::Text(_), ValueKind::String) => true,
            (Self::Text(text), ValueKind::Enum(variants)) => variants.contains(&text.as_str()),
            _ => false,
        };
        if matches {
            return Ok(self);
        }
        let raw = self.to_string();
        Err(match kind {
            ValueKind::Bool => Self::bool_error(name, &raw),
            ValueKind::Number => Self::number_error(name, &raw),
            ValueKind::Enum(variants) => Self::enum_error(name, &raw, variants),
            ValueKind::String => Self::string_error(name, &raw),
        })
    }

    /// This value as a `toml_edit::Value`, for writing into
    /// `~/.loom/config.toml`.
    pub fn to_toml_edit(&self) -> toml_edit::Value {
        match self {
            Self::Bool(b) => toml_edit::Value::from(*b),
            Self::Number(n) => toml_edit::Value::from(*n as i64),
            Self::Text(s) => toml_edit::Value::from(s.as_str()),
        }
    }

    /// The right-hand side of a TOML `key = <literal>` assignment. `Bool` and
    /// `Number` use [`Display`](std::fmt::Display); `Text` goes through
    /// [`Self::to_toml_edit`] so escaping and quoting come from `toml_edit`
    /// itself rather than a hand-rolled `format!("\"{s}\"")` that would drift
    /// the moment a value contains a quote.
    pub fn to_toml_literal(&self) -> String {
        match self {
            Self::Bool(_) | Self::Number(_) => self.to_string(),
            Self::Text(_) => self.to_toml_edit().to_string().trim().to_owned(),
        }
    }
}

impl std::fmt::Display for ConfigValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{b}"),
            Self::Number(n) => write!(f, "{n}"),
            Self::Text(s) => write!(f, "{s}"),
        }
    }
}
