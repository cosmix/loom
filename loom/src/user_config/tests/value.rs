//! Coverage for [`ConfigValue`]: round trips through the two TOML
//! representations, `Display`, `to_toml_literal`, `checked`, and the
//! free-text `ValueKind::String` arm no registered key exercises yet.

use super::super::keys::ValueKind;
use super::super::{ConfigValue, KeySpec};

const BOOL_KEY: &str = "update.check";
const NUMBER_KEY: &str = "context.ceiling_tokens";
const ENUM_KEY: &str = "terminal.backend";
const ENUM_VARIANTS: &[&str] = &["native", "tmux"];

/// A `ValueKind::String` key. No entry in `keys::KEYS` uses this kind yet, so
/// tests that want to drive it need their own [`KeySpec`].
fn string_key_spec() -> KeySpec {
    KeySpec {
        name: "test.free_text",
        section: "test",
        field: "free_text",
        kind: ValueKind::String,
        help: "test-only free-text key",
    }
}

/// The `toml_edit::Value` [`ConfigValue::to_toml_edit`] produced, read back
/// out as the `toml` crate's own `Value` type — the shape
/// [`ConfigValue::from_toml_value`] actually consumes.
fn to_toml_crate_value(edit: &toml_edit::Value) -> toml::Value {
    if let Some(b) = edit.as_bool() {
        toml::Value::Boolean(b)
    } else if let Some(n) = edit.as_integer() {
        toml::Value::Integer(n)
    } else if let Some(s) = edit.as_str() {
        toml::Value::String(s.to_owned())
    } else {
        panic!("unexpected toml_edit::Value shape: {edit:?}");
    }
}

#[test]
fn round_trips_through_to_toml_edit_and_from_toml_value() {
    let cases: [(ValueKind, &str, &str); 4] = [
        (ValueKind::Bool, BOOL_KEY, "true"),
        (ValueKind::Number, NUMBER_KEY, "24"),
        (ValueKind::Enum(ENUM_VARIANTS), ENUM_KEY, "native"),
        (ValueKind::String, "test.free_text", "opus"),
    ];
    for (kind, name, raw) in cases {
        let parsed = ConfigValue::parse(&kind, name, raw).unwrap();
        let toml_value = to_toml_crate_value(&parsed.to_toml_edit());
        let round_tripped = ConfigValue::from_toml_value(&kind, name, &toml_value).unwrap();
        assert_eq!(parsed, round_tripped, "round trip mismatch for {name}");
    }
}

#[test]
fn display_reproduces_value_ofs_pre_typed_output() {
    assert_eq!(ConfigValue::Bool(true).to_string(), "true");
    assert_eq!(ConfigValue::Number(24).to_string(), "24");
    assert_eq!(
        ConfigValue::Text("native".to_string()).to_string(),
        "native"
    );
    assert_eq!(ConfigValue::Text("opus".to_string()).to_string(), "opus");
}

#[test]
fn to_toml_literal_per_kind() {
    assert_eq!(ConfigValue::Bool(true).to_toml_literal(), "true");
    assert_eq!(ConfigValue::Number(24).to_toml_literal(), "24");
    assert_eq!(
        ConfigValue::Text("native".to_string()).to_toml_literal(),
        "\"native\""
    );
}

#[test]
fn to_toml_literal_escapes_an_embedded_quote_via_toml_edit() {
    let value = ConfigValue::Text("has \"quotes\" inside".to_string());
    let literal = value.to_toml_literal();

    // Prove the escaping came from toml_edit itself, not a hand-rolled
    // format!: the literal must round-trip through a real TOML document back
    // to the original unescaped string.
    let doc: toml_edit::DocumentMut = format!("v = {literal}\n").parse().unwrap();
    assert_eq!(doc["v"].as_str(), Some("has \"quotes\" inside"));
}

#[test]
fn checked_accepts_the_matching_variant() {
    assert_eq!(
        ConfigValue::Bool(true)
            .checked(&ValueKind::Bool, BOOL_KEY)
            .unwrap(),
        ConfigValue::Bool(true)
    );
    assert_eq!(
        ConfigValue::Number(24)
            .checked(&ValueKind::Number, NUMBER_KEY)
            .unwrap(),
        ConfigValue::Number(24)
    );
    assert_eq!(
        ConfigValue::Text("native".to_string())
            .checked(&ValueKind::Enum(ENUM_VARIANTS), ENUM_KEY)
            .unwrap(),
        ConfigValue::Text("native".to_string())
    );
    assert_eq!(
        ConfigValue::Text("anything at all".to_string())
            .checked(&ValueKind::String, "test.free_text")
            .unwrap(),
        ConfigValue::Text("anything at all".to_string())
    );
}

#[test]
fn checked_rejects_a_mismatch_with_the_same_message_parse_would_produce() {
    let text_vs_number = ConfigValue::Text("abc".to_string())
        .checked(&ValueKind::Number, NUMBER_KEY)
        .unwrap_err()
        .to_string();
    let parse_vs_number = ConfigValue::parse(&ValueKind::Number, NUMBER_KEY, "abc")
        .unwrap_err()
        .to_string();
    assert_eq!(text_vs_number, parse_vs_number);

    let number_vs_bool = ConfigValue::Number(42)
        .checked(&ValueKind::Bool, BOOL_KEY)
        .unwrap_err()
        .to_string();
    let parse_vs_bool = ConfigValue::parse(&ValueKind::Bool, BOOL_KEY, "42")
        .unwrap_err()
        .to_string();
    assert_eq!(number_vs_bool, parse_vs_bool);
    assert_eq!(
        number_vs_bool,
        format!("{BOOL_KEY}: \"42\" is not a bool (expected true or false)")
    );

    let bool_vs_enum = ConfigValue::Bool(true)
        .checked(&ValueKind::Enum(ENUM_VARIANTS), ENUM_KEY)
        .unwrap_err()
        .to_string();
    let parse_vs_enum = ConfigValue::parse(&ValueKind::Enum(ENUM_VARIANTS), ENUM_KEY, "true")
        .unwrap_err()
        .to_string();
    assert_eq!(bool_vs_enum, parse_vs_enum);
}

#[test]
fn checked_never_coerces_text_that_happens_to_reparse_as_the_target_kind() {
    // "false" and "42" both reparse cleanly, so a coercing `checked` would
    // have accepted them. It must still reject the variant mismatch.
    let text_false_vs_bool = ConfigValue::Text("false".to_string())
        .checked(&ValueKind::Bool, BOOL_KEY)
        .unwrap_err()
        .to_string();
    assert_eq!(
        text_false_vs_bool,
        format!("{BOOL_KEY}: \"false\" is not a bool (expected true or false)")
    );

    let text_42_vs_number = ConfigValue::Text("42".to_string())
        .checked(&ValueKind::Number, NUMBER_KEY)
        .unwrap_err()
        .to_string();
    assert_eq!(
        text_42_vs_number,
        format!("{NUMBER_KEY}: \"42\" is not a u32 (expected a non-negative integer)")
    );
}

#[test]
fn checked_rejects_bool_and_number_against_the_string_kind() {
    let string_key = "test.free_text";

    let bool_vs_string = ConfigValue::Bool(true)
        .checked(&ValueKind::String, string_key)
        .unwrap_err()
        .to_string();
    assert_eq!(
        bool_vs_string,
        format!("{string_key}: \"true\" is not a string")
    );

    let number_vs_string = ConfigValue::Number(3)
        .checked(&ValueKind::String, string_key)
        .unwrap_err()
        .to_string();
    assert_eq!(
        number_vs_string,
        format!("{string_key}: \"3\" is not a string")
    );
}

#[test]
fn checked_rejects_an_enum_value_not_in_the_variant_list() {
    let err = ConfigValue::Text("carrier-pigeon".to_string())
        .checked(&ValueKind::Enum(ENUM_VARIANTS), ENUM_KEY)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("is not one of the expected values: native, tmux"),
        "{err}"
    );
}

#[test]
fn from_toml_value_rejects_a_wrong_toml_type_naming_the_key() {
    let err = ConfigValue::from_toml_value(
        &ValueKind::Bool,
        BOOL_KEY,
        &toml::Value::String("nope".to_string()),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains(BOOL_KEY), "{err}");

    let err = ConfigValue::from_toml_value(
        &ValueKind::Number,
        NUMBER_KEY,
        &toml::Value::String("nope".to_string()),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains(NUMBER_KEY), "{err}");

    let err =
        ConfigValue::from_toml_value(&ValueKind::Number, NUMBER_KEY, &toml::Value::Integer(-1))
            .unwrap_err()
            .to_string();
    assert!(err.contains("is out of range for a u32"), "{err}");

    let err = ConfigValue::from_toml_value(
        &ValueKind::Enum(ENUM_VARIANTS),
        ENUM_KEY,
        &toml::Value::Integer(1),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains(ENUM_KEY), "{err}");
}

#[test]
fn the_string_kind_accepts_free_text() {
    let spec = string_key_spec();
    // Not a valid bool or integer, proving this genuinely falls into the
    // free-text arm rather than an earlier one.
    let raw = "my project";

    let parsed = ConfigValue::parse(&spec.kind, spec.name, raw).unwrap();
    assert_eq!(parsed, ConfigValue::Text(raw.to_string()));

    let toml_value = toml::Value::String(raw.to_string());
    let from_toml = ConfigValue::from_toml_value(&spec.kind, spec.name, &toml_value).unwrap();
    assert_eq!(from_toml, ConfigValue::Text(raw.to_string()));

    let checked = ConfigValue::Text(raw.to_string())
        .checked(&spec.kind, spec.name)
        .unwrap();
    assert_eq!(checked, ConfigValue::Text(raw.to_string()));

    assert_eq!(parsed.to_string(), raw);
    assert_eq!(parsed.to_toml_literal(), "\"my project\"");
}
