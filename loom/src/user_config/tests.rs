use super::*;
use keys::{spec, ValueKind};

// Every test here builds a `UserConfig` from a TOML string, or exercises
// `set_in` against a temp file path — never the real `$HOME` — so the suite
// stays deterministic under parallel execution.

#[test]
fn each_key_parses_a_valid_value() {
    assert_eq!(
        spec("update.check")
            .unwrap()
            .parse("true")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    assert_eq!(
        spec("update.check_interval_hours")
            .unwrap()
            .parse("6")
            .unwrap()
            .as_integer(),
        Some(6)
    );
    assert_eq!(
        spec("terminal.backend")
            .unwrap()
            .parse("tmux")
            .unwrap()
            .as_str(),
        Some("tmux")
    );
    assert_eq!(
        spec("context.ceiling_tokens")
            .unwrap()
            .parse("123456")
            .unwrap()
            .as_integer(),
        Some(123456)
    );
}

#[test]
fn each_key_rejects_a_type_mismatched_value() {
    for (key, bad) in [
        ("update.check", "maybe"),
        ("update.check_interval_hours", "not-a-number"),
        ("terminal.backend", "ssh"),
        ("context.ceiling_tokens", "-5"),
    ] {
        let err = spec(key).unwrap().parse(bad).unwrap_err().to_string();
        assert!(
            err.contains(key),
            "error for {key} should name the key: {err}"
        );
        assert!(
            err.contains(bad),
            "error for {key} should quote the offending text: {err}"
        );
    }
}

#[test]
fn unknown_key_lists_every_valid_key() {
    let err = spec("no.such.key").unwrap_err().to_string();
    assert!(err.contains("no.such.key"));
    for key in keys::KEYS {
        assert!(
            err.contains(key.name),
            "valid-key list missing {}: {err}",
            key.name
        );
    }
}

#[test]
fn keys_are_typed_as_documented() {
    assert_eq!(spec("update.check").unwrap().kind, ValueKind::Bool);
    assert_eq!(
        spec("update.check_interval_hours").unwrap().kind,
        ValueKind::U32
    );
    assert_eq!(
        spec("terminal.backend").unwrap().kind,
        ValueKind::Enum(&["native", "tmux"])
    );
    assert_eq!(spec("context.ceiling_tokens").unwrap().kind, ValueKind::U32);
}

#[test]
fn defaults_when_the_file_is_absent() {
    let config = UserConfig::default();
    assert!(config.update_check());
    assert_eq!(config.update_check_interval_hours(), 24);
    assert_eq!(config.terminal_backend(), SessionBackendKind::Native);
    assert_eq!(
        config.context_ceiling_tokens(),
        DEFAULT_CONTEXT_CEILING_TOKENS
    );
    assert_eq!(config.terminal_backend_set(), None);
    assert_eq!(config.context_ceiling_tokens_set(), None);
}

#[test]
fn parses_every_key_out_of_a_document() {
    let config = parse_document(
        "[update]\ncheck = false\ncheck_interval_hours = 6\n\n[terminal]\nbackend = \"tmux\"\n\n[context]\nceiling_tokens = 111111\n",
    )
    .unwrap();
    assert!(!config.update_check());
    assert_eq!(config.update_check_interval_hours(), 6);
    assert_eq!(config.terminal_backend(), SessionBackendKind::Tmux);
    assert_eq!(config.context_ceiling_tokens(), 111111);
}

#[test]
fn a_type_mismatched_field_in_the_document_is_an_error() {
    assert!(parse_document("[update]\ncheck = \"nope\"\n").is_err());
    assert!(parse_document("[context]\nceiling_tokens = \"nope\"\n").is_err());
    assert!(parse_document("[terminal]\nbackend = \"carrier-pigeon\"\n").is_err());
}

#[test]
fn origin_is_set_only_for_keys_the_document_wrote() {
    let config = parse_document("[update]\ncheck_interval_hours = 6\n").unwrap();

    let (value, origin) = config.value_of(spec("update.check_interval_hours").unwrap());
    assert_eq!(value, "6");
    assert_eq!(origin, Origin::Set);
    assert_eq!(origin.to_string(), "set");

    let (value, origin) = config.value_of(spec("terminal.backend").unwrap());
    assert_eq!(value, "native");
    assert_eq!(origin, Origin::Default);
    assert_eq!(origin.to_string(), "default");
}

#[test]
fn to_toml_string_renders_every_key_resolved() {
    let config = parse_document("[context]\nceiling_tokens = 55555\n").unwrap();
    let rendered = config.to_toml_string();

    // Section order: context, terminal, update.
    let context_at = rendered.find("[context]").unwrap();
    let terminal_at = rendered.find("[terminal]").unwrap();
    let update_at = rendered.find("[update]").unwrap();
    assert!(
        context_at < terminal_at && terminal_at < update_at,
        "{rendered}"
    );

    assert!(rendered.contains("ceiling_tokens = 55555"));
    assert!(rendered.contains("backend = \"native\""));
    assert!(rendered.contains("check = true"));
    assert!(rendered.contains("check_interval_hours = 24"));
}

#[test]
fn set_in_preserves_comments_and_unknown_keys() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    std::fs::write(
        &path,
        "# a comment worth keeping\n[terminal]\nbackend = \"native\"\nsome_future_key = \"kept\"\n",
    )
    .unwrap();

    set_in(
        &path,
        spec("terminal.backend").unwrap(),
        toml_edit::Value::from("tmux"),
    )
    .unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("# a comment worth keeping"), "{after}");
    assert!(after.contains("some_future_key = \"kept\""), "{after}");
    assert!(after.contains("backend = \"tmux\""), "{after}");
}

#[test]
fn value_of_has_an_arm_for_every_registered_key() {
    // `value_of`'s match on `spec.name` has an `unreachable!()` fallback arm
    // that nothing checks at compile time - a fifth key added to
    // `keys::KEYS` without a matching arm would panic `loom config --list`
    // at runtime. Looping over the real registry here means that panic
    // happens in this test instead of in the field.
    let config = UserConfig::default();
    for key in keys::KEYS {
        let (value, origin) = config.value_of(key);
        assert_eq!(origin, Origin::Default, "{}", key.name);
        match key.name {
            "update.check" => assert_eq!(value, "true"),
            "update.check_interval_hours" => assert_eq!(value, "24"),
            "terminal.backend" => assert_eq!(value, "native"),
            "context.ceiling_tokens" => {
                assert_eq!(value, DEFAULT_CONTEXT_CEILING_TOKENS.to_string())
            }
            other => panic!("no expected default wired up for key {other}"),
        }
    }
}

#[test]
fn set_in_creates_an_absent_section() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    // File does not exist yet - `set_in` creates it and the section.

    set_in(
        &path,
        spec("context.ceiling_tokens").unwrap(),
        toml_edit::Value::from(70000_i64),
    )
    .unwrap();

    let config = parse_document(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(config.context_ceiling_tokens(), 70000);
}
