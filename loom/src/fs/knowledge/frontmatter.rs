//! Read and update the leading YAML frontmatter of a knowledge document.

use crate::context::schema::LifecycleState;
use crate::parser::frontmatter::split_frontmatter_raw;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;

const KNOWN_KEYS: &[&str] = &["id", "aliases", "state", "sources", "verified"];

// Untyped serde values discard YAML tags. Reject them at every node so an
// update cannot silently change the meaning of an unknown value.
struct UntaggedValue(Value);

impl<'de> Deserialize<'de> for UntaggedValue {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let serde_saphyr::Tagged(value, tag) =
            serde_saphyr::Tagged::<UntaggedValueInner>::deserialize(deserializer)?;
        if tag.is_some() {
            return Err(serde::de::Error::custom(
                "YAML tags are unsupported in frontmatter",
            ));
        }
        Ok(Self(value.0))
    }
}

struct UntaggedValueInner(Value);

impl<'de> Deserialize<'de> for UntaggedValueInner {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserializer.deserialize_any(UntaggedValueVisitor)
    }
}

struct UntaggedValueVisitor;
impl<'de> serde::de::Visitor<'de> for UntaggedValueVisitor {
    type Value = UntaggedValueInner;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("an untagged YAML value")
    }
    fn visit_bool<E: serde::de::Error>(self, v: bool) -> std::result::Result<Self::Value, E> {
        Ok(UntaggedValueInner(v.into()))
    }
    fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
        Ok(UntaggedValueInner(v.into()))
    }
    fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
        Ok(UntaggedValueInner(v.into()))
    }
    fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
        serde_json::Number::from_f64(v)
            .map(|n| UntaggedValueInner(Value::Number(n)))
            .ok_or_else(|| E::custom("non-finite number"))
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<Self::Value, E> {
        Ok(UntaggedValueInner(v.into()))
    }
    fn visit_string<E: serde::de::Error>(self, v: String) -> std::result::Result<Self::Value, E> {
        Ok(UntaggedValueInner(v.into()))
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
        Ok(UntaggedValueInner(Value::Null))
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(
        self,
        mut seq: A,
    ) -> std::result::Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        while let Some(UntaggedValue(value)) = seq.next_element()? {
            values.push(value);
        }
        Ok(UntaggedValueInner(Value::Array(values)))
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(
        self,
        mut map: A,
    ) -> std::result::Result<Self::Value, A::Error> {
        let mut values = Map::new();
        while let Some((UntaggedValue(key), UntaggedValue(value))) = map.next_entry()? {
            let Value::String(key) = key else {
                return Err(serde::de::Error::custom("mapping key must be a string"));
            };
            if values.insert(key, value).is_some() {
                return Err(serde::de::Error::custom("duplicate mapping key"));
            }
        }
        Ok(UntaggedValueInner(Value::Object(values)))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Frontmatter {
    pub(crate) id: Option<String>,
    pub(crate) aliases: Vec<String>,
    pub(crate) state: Option<LifecycleState>,
    pub(crate) sources: Vec<String>,
    pub(crate) verified: Option<String>,
    unknown: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct KnownFrontmatter {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<LifecycleState>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    sources: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    verified: Option<String>,
}

impl Frontmatter {
    fn from_known(known: KnownFrontmatter, unknown: Map<String, Value>) -> Self {
        Self {
            id: known.id,
            aliases: known.aliases,
            state: known.state,
            sources: known.sources,
            verified: known.verified,
            unknown,
        }
    }

    fn parse(raw: &str) -> Result<Self> {
        let known = if raw.trim().is_empty() {
            KnownFrontmatter::default()
        } else {
            crate::yaml::from_str(raw).context("Invalid YAML frontmatter")?
        };
        let mut mapping: Map<String, Value> = if raw.trim().is_empty() {
            Map::new()
        } else {
            let UntaggedValue(Value::Object(mapping)) =
                crate::yaml::from_str(raw).context("Unsupported YAML frontmatter mapping")?
            else {
                bail!("YAML frontmatter must be a mapping");
            };
            mapping
        };
        for key in KNOWN_KEYS {
            mapping.remove(*key);
        }
        Ok(Self::from_known(known, mapping))
    }

    fn render_block(&self) -> Result<String> {
        let known = KnownFrontmatter {
            id: self.id.clone(),
            aliases: self.aliases.clone(),
            state: self.state,
            sources: self.sources.clone(),
            verified: self.verified.clone(),
        };
        let mut mapping = self.unknown.clone();
        let Value::Object(known) = serde_json::to_value(known)? else {
            bail!("Serialized frontmatter is not a mapping");
        };
        mapping.extend(known);
        let mut yaml =
            crate::yaml::to_string(&mapping).context("Failed to serialize frontmatter")?;
        let reparsed: Map<String, Value> =
            crate::yaml::from_str(&yaml).context("Serialized frontmatter is invalid")?;
        if reparsed != mapping {
            bail!("Serialized frontmatter does not preserve metadata");
        }
        if yaml == "{}\n" {
            yaml.clear();
        }
        Ok(format!("---\n{yaml}---\n"))
    }
}

/// Parse frontmatter for readers. Malformed YAML is stripped but contributes
/// no metadata, preserving the chunker's long-standing best-effort behavior.
pub(crate) fn split_frontmatter(text: &str) -> (Frontmatter, &str) {
    let Some((raw, body)) = split_frontmatter_raw(text).ok() else {
        return (Frontmatter::default(), text);
    };
    let metadata = Frontmatter::parse(raw).unwrap_or_else(|_| {
        crate::yaml::from_str(raw)
            .map(|known| Frontmatter::from_known(known, Map::new()))
            .unwrap_or_default()
    });
    (metadata, body)
}

pub(crate) fn file_frontmatter(bytes: &[u8]) -> Frontmatter {
    let text = String::from_utf8_lossy(bytes);
    split_frontmatter(&text).0
}

pub(crate) fn file_state(bytes: &[u8]) -> Option<LifecycleState> {
    file_frontmatter(bytes).state
}

/// Apply a fallible metadata mutation under the same parent-directory lock as
/// all other knowledge writers, returning the rendered frontmatter block, or
/// an empty string when no block was written.
///
/// A file with no leading `---` block that the mutation leaves empty (e.g. a
/// blurb-only annotation) is written back unchanged: no empty block is
/// introduced.
pub(crate) fn update_file<F>(path: &Path, update: F) -> Result<String>
where
    F: FnOnce(&mut Frontmatter) -> Result<()>,
{
    let mut rendered = String::new();
    crate::fs::locking::locked_update(path, |content| {
        if content.is_empty() {
            bail!(
                "Cannot annotate missing or empty knowledge file: {}",
                path.display()
            );
        }
        let (mut frontmatter, body, had_block) = match split_frontmatter_raw(&content).ok() {
            Some((raw, body)) => (Frontmatter::parse(raw)?, body, true),
            None => (Frontmatter::default(), content.as_str(), false),
        };
        update(&mut frontmatter)?;
        if !had_block && frontmatter == Frontmatter::default() {
            return Ok(content);
        }
        let block = frontmatter.render_block()?;
        rendered = block.clone();
        Ok(format!("{block}{body}"))
    })?;
    Ok(rendered)
}

#[cfg(test)]
#[path = "frontmatter_tests.rs"]
mod tests;
