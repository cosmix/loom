//! YAML (de)serialization wrapper around `serde-saphyr`.
//!
//! Parsing runs with `strict_booleans: true`: only `true`/`false` are booleans,
//! matching the YAML 1.2 behaviour loom's files were written against. Without it
//! serde-saphyr infers `yes`/`no`/`on`/`off` as booleans on every
//! `deserialize_any` path (`serde_json::Value`, untagged enums, flattened
//! structs), turning a string such as `on` into a bool.

use serde::{Deserialize, Serialize};
use serde_saphyr::{Options, SerializeError};

/// Deserialize `T` from YAML text.
pub fn from_str<'de, T: Deserialize<'de>>(input: &'de str) -> Result<T, serde_saphyr::Error> {
    let mut options = Options::default();
    options.strict_booleans = true;
    serde_saphyr::from_str_with_options(input, options)
}

/// Serialize `value` to YAML text.
pub fn to_string<T: Serialize>(value: &T) -> Result<String, SerializeError> {
    serde_saphyr::to_string(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    enum Kind {
        Unit,
        Newtype(String),
        Struct { a: u32, b: Option<String> },
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Doc {
        multi: String,
        lookalikes: Vec<String>,
        punctuated: String,
        some: Option<String>,
        none: Option<String>,
        nested: BTreeMap<String, BTreeMap<String, Vec<u32>>>,
        kinds: Vec<Kind>,
    }

    fn sample() -> Doc {
        let mut inner = BTreeMap::new();
        inner.insert("xs".to_string(), vec![1, 2, 3]);
        let mut nested = BTreeMap::new();
        nested.insert("outer".to_string(), inner);
        Doc {
            multi: "line one\nline two\n\nline four\n".to_string(),
            lookalikes: ["yes", "1.0", "null", "~", "", "true", "0x1f"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            punctuated: "key: value # not a comment".to_string(),
            some: Some("x".to_string()),
            none: None,
            nested,
            kinds: vec![
                Kind::Unit,
                Kind::Newtype("n".to_string()),
                Kind::Struct {
                    a: 7,
                    b: Some("s".to_string()),
                },
            ],
        }
    }

    #[test]
    fn yes_no_on_off_parse_as_strings() {
        let v: serde_json::Value = from_str("a: yes\nb: no\nc: on\nd: off\ne: true\n").unwrap();
        for (key, want) in [("a", "yes"), ("b", "no"), ("c", "on"), ("d", "off")] {
            assert_eq!(v[key], serde_json::Value::String(want.to_string()), "{key}");
        }
        assert_eq!(v["e"], serde_json::Value::Bool(true));
    }

    #[test]
    fn struct_round_trips_unchanged() {
        let doc = sample();
        let text = to_string(&doc).unwrap();
        let back: Doc = from_str(&text).unwrap();
        assert_eq!(back, doc, "yaml was:\n{text}");
    }

    #[test]
    fn lookalike_strings_stay_strings_through_json_value() {
        let text = to_string(&sample()).unwrap();
        let v: serde_json::Value = from_str(&text).unwrap();
        let items = v["lookalikes"].as_array().unwrap();
        assert!(items.iter().all(|i| i.is_string()), "yaml was:\n{text}");
    }
}
