use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

fn write_skill(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file
}

#[test]
fn strip_quotes_handles_single_quote_characters() {
    assert_eq!(strip_quotes("\""), "\"");
    assert_eq!(strip_quotes("'"), "'");
    assert_eq!(strip_quotes("\"value\""), "value");
    assert_eq!(strip_quotes("'value'"), "value");
}

#[test]
fn name_match_strips_loom_prefix() {
    // Non-stopword keywords keep the prefix-match behavior.
    for (keyword, skill) in [
        ("rust", "loom-rust"),
        ("refactor", "loom-refactoring"),
        ("testing", "loom-testing"),
        ("security", "loom-security-audit"),
    ] {
        assert!(is_skill_name_match(keyword, skill));
    }
    assert!(!is_skill_name_match("rust", "loom-auth"));
    assert!(!is_skill_name_match("cd", "loom-argocd"));
    assert!(!is_skill_name_match("re", "loom-react"));
}

#[test]
fn parses_plain_keywords_field() {
    let skill = write_skill(concat!(
        "---\n",
        "name: loom-feature-flags\n",
        "description: Feature flag patterns.\n",
        "keywords: feature flag, feature toggle, LaunchDarkly, A/B test\n",
        "---\n",
        "\nBody.\n",
    ));
    let triggers = parse_skill_triggers(skill.path()).unwrap();
    for expected in ["feature flag", "feature toggle", "LaunchDarkly"] {
        assert!(triggers.iter().any(|trigger| trigger == expected));
    }
}

#[test]
fn keywords_augment_existing_triggers_list() {
    let skill = write_skill(concat!(
        "---\n",
        "name: loom-example\n",
        "triggers:\n",
        "  - foo\n",
        "  - bar\n",
        "keywords: baz, qux\n",
        "---\n",
    ));
    let triggers = parse_skill_triggers(skill.path()).unwrap();
    for expected in ["foo", "bar", "baz", "qux"] {
        assert!(
            triggers.iter().any(|trigger| trigger == expected),
            "missing {expected}: {triggers:?}"
        );
    }
}

#[test]
fn description_marker_variants_are_recognized() {
    let skill = write_skill(concat!(
        "---\n",
        "name: loom-logging-observability\n",
        "description: |\n",
        "  Comprehensive logging. Triggers for this skill - log, logging, OpenTelemetry, OTEL.\n",
        "---\n",
    ));
    let triggers = parse_skill_triggers(skill.path()).unwrap();
    for expected in ["log", "logging", "OpenTelemetry", "OTEL"] {
        assert!(
            triggers.iter().any(|trigger| trigger == expected),
            "missing {expected}: {triggers:?}"
        );
    }
}

#[test]
fn stopword_name_match_requires_exact_equality() {
    // A stopword keyword is exempted from the stopword filter only on EXACT
    // equality with the effective name - a mere prefix match must not let a
    // bare generic word solo-qualify a skill (the loom-model-evaluation
    // false positive the evidence rule targets).
    let stopwords: HashSet<&str> = STOPWORDS.iter().copied().collect();
    assert!(is_stopword("model", &stopwords));
    assert!(is_skill_name_match("model", "loom-model"));
    assert!(!is_skill_name_match("model", "loom-model-evaluation"));
    assert!(is_stopword("test", &stopwords));
    assert!(!is_skill_name_match("test", "loom-testing"));
    assert!(is_stopword("plan", &stopwords));
    assert!(!is_skill_name_match("plan", "loom-plan-writer"));
    assert!(is_stopword("build", &stopwords));
    assert!(!is_skill_name_match("build", "loom-auth"));
}

#[test]
fn add_triggers_drops_stopword_prefix_match_but_keeps_exact_match() {
    let stopwords: HashSet<&str> = STOPWORDS.iter().copied().collect();

    let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    add_triggers(
        &mut index,
        &stopwords,
        "loom-model-evaluation",
        vec!["model".to_string()],
    );
    assert!(
        !index.contains_key("model"),
        "stopword prefix match must not qualify the index: {index:?}"
    );

    let mut index = BTreeMap::new();
    add_triggers(
        &mut index,
        &stopwords,
        "loom-model",
        vec!["model".to_string()],
    );
    assert_eq!(
        index.get("model").map(Vec::as_slice),
        Some(&["loom-model".to_string()][..])
    );
}

#[test]
fn execute_propagates_index_write_failure() {
    let home = tempfile::tempdir().unwrap();
    let skill_dir = home.path().join(".claude/skills/example");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\ntriggers:\n  - example\n---\n",
    )
    .unwrap();
    std::fs::create_dir_all(home.path().join(".claude/hooks/loom/skill-keywords.json")).unwrap();

    let error = execute_in_agent_dir(&home.path().join(".claude"), true).unwrap_err();

    assert!(error.to_string().contains("Failed to write"));
}

#[test]
fn catalog_only_skill_is_indexed() {
    let home = tempfile::tempdir().unwrap();
    let skills_dir = home.path().join(".claude/skills");
    let catalog_dir = home.path().join(".claude/loom-skill-catalog");
    let skill_dir = catalog_dir.join("catalog-only");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\ntriggers:\n  - catalog\n---\n",
    )
    .unwrap();

    let (index, skill_count) = build_index(&[&skills_dir, &catalog_dir]).unwrap();

    assert_eq!(skill_count, 1);
    assert_eq!(
        index.get("catalog"),
        Some(&vec!["catalog-only".to_string()])
    );
}

#[test]
fn duplicate_skill_names_across_roots_are_not_duplicated() {
    let home = tempfile::tempdir().unwrap();
    let skills_dir = home.path().join(".claude/skills");
    let catalog_dir = home.path().join(".claude/loom-skill-catalog");
    for root in [&skills_dir, &catalog_dir] {
        let skill_dir = root.join("shared");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\ntriggers:\n  - shared\n---\n",
        )
        .unwrap();
    }

    let (index, skill_count) = build_index(&[&skills_dir, &catalog_dir]).unwrap();

    assert_eq!(skill_count, 1);
    assert_eq!(index.get("shared"), Some(&vec!["shared".to_string()]));
}

#[test]
fn execute_in_agent_dir_writes_index_under_given_directory() {
    let claude = tempfile::tempdir().unwrap();
    let skill_dir = claude.path().join("skills/example");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\ntriggers:\n  - example\n---\n",
    )
    .unwrap();

    execute_in_agent_dir(claude.path(), false).unwrap();

    let index =
        std::fs::read_to_string(claude.path().join("hooks/loom/skill-keywords.json")).unwrap();
    assert!(index.contains("example"));
}
