use super::*;
use keys::spec;

#[test]
fn summary_keys_resolve_defaults_with_default_origin() {
    let config = UserConfig::default();

    for (key, expected) in [
        ("plan.summary_backend", "claude"),
        ("plan.summary_codex_model", "gpt-6.1-terra"),
        ("plan.summary_codex_effort", "high"),
    ] {
        let (value, origin) = config.value_of(spec(key).unwrap());
        assert_eq!(value, ConfigValue::Text(expected.to_string()), "{key}");
        assert_eq!(origin, Origin::Default, "{key}");
    }
}

#[test]
fn summary_keys_resolve_document_values_with_set_origin() {
    let config = parse_document(
        "[plan]\nsummary_backend = \"codex\"\n\
         summary_codex_model = \"gpt-6.1-sol\"\nsummary_codex_effort = \"xhigh\"\n",
    )
    .unwrap();

    for (key, expected) in [
        ("plan.summary_backend", "codex"),
        ("plan.summary_codex_model", "gpt-6.1-sol"),
        ("plan.summary_codex_effort", "xhigh"),
    ] {
        let (value, origin) = config.value_of(spec(key).unwrap());
        assert_eq!(value, ConfigValue::Text(expected.to_string()), "{key}");
        assert_eq!(origin, Origin::Set, "{key}");
    }
}

#[test]
fn summary_keys_parse_valid_values_and_reject_unknown_variants() {
    for (key, valid) in [
        ("plan.summary_backend", "codex"),
        ("plan.summary_codex_model", "gpt-6.1-sol"),
        ("plan.summary_codex_effort", "xhigh"),
    ] {
        let key_spec = spec(key).unwrap();
        assert_eq!(
            key_spec.parse(valid).unwrap(),
            ConfigValue::Text(valid.to_string()),
            "{key}"
        );
        let err = key_spec.parse("bogus").unwrap_err().to_string();
        assert!(err.contains(key), "{err}");
        assert!(err.contains("bogus"), "{err}");
    }
}
