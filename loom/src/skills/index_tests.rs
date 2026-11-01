use super::*;
use std::io::Write;
use tempfile::TempDir;

fn create_test_skill(dir: &Path, name: &str, description: &str, triggers: &[&str]) {
    let skill_dir = dir.join(name);
    fs::create_dir_all(&skill_dir).unwrap();

    let skill_file = skill_dir.join("SKILL.md");
    let mut file = fs::File::create(&skill_file).unwrap();

    writeln!(file, "---").unwrap();
    writeln!(file, "name: {name}").unwrap();
    writeln!(file, "description: {description}").unwrap();
    writeln!(file, "triggers:").unwrap();
    for trigger in triggers {
        writeln!(file, "  - {trigger}").unwrap();
    }
    writeln!(file, "---").unwrap();
    writeln!(file, "# {name}").unwrap();
}

#[test]
fn test_load_from_directory() {
    let temp_dir = TempDir::new().unwrap();
    let skills_dir = temp_dir.path();

    create_test_skill(
        skills_dir,
        "auth",
        "Authentication patterns",
        &["login", "password"],
    );
    create_test_skill(
        skills_dir,
        "testing",
        "Testing patterns",
        &["test", "unit test"],
    );

    let index = SkillIndex::load_from_directory(skills_dir).unwrap();

    assert_eq!(index.skill_count(), 2);
    assert!(!index.is_empty());
}

#[test]
fn test_load_from_nonexistent_directory() {
    let index = SkillIndex::load_from_directory(Path::new("/nonexistent/path")).unwrap();
    assert!(index.is_empty());
}

#[test]
fn test_parse_frontmatter() {
    let content = r#"---
name: test-skill
description: A test skill description
triggers:
  - trigger1
  - trigger2
---
# Test Skill Content
"#;

    let metadata = SkillIndex::parse_frontmatter(content).unwrap();
    assert_eq!(metadata.name, "test-skill");
    assert_eq!(metadata.description, "A test skill description");
    assert_eq!(metadata.triggers.len(), 2);
}

#[test]
fn test_parse_frontmatter_missing_open() {
    let content = "name: test\n---\nContent";
    assert!(SkillIndex::parse_frontmatter(content).is_err());
}

#[test]
fn test_parse_frontmatter_missing_close() {
    let content = "---\nname: test\nContent";
    assert!(SkillIndex::parse_frontmatter(content).is_err());
}

#[test]
fn test_match_skills_integration() {
    let temp_dir = TempDir::new().unwrap();
    let skills_dir = temp_dir.path();

    create_test_skill(
        skills_dir,
        "auth",
        "Authentication patterns",
        &["login", "password", "token", "refresh token"],
    );
    create_test_skill(
        skills_dir,
        "testing",
        "Testing patterns",
        &["test", "unit test", "integration test"],
    );

    let index = SkillIndex::load_from_directory(skills_dir).unwrap();

    // Match with phrase trigger
    let matches = index.match_skills("implement refresh token rotation", 5);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "auth");
    assert!(matches[0].score >= 2.0); // phrase match

    // Match with multiple word triggers
    let matches = index.match_skills("add login and password validation", 5);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "auth");
    assert!(matches[0].score >= 2.0); // two word matches
}

#[test]
fn test_match_skills_below_threshold() {
    let temp_dir = TempDir::new().unwrap();
    let skills_dir = temp_dir.path();

    create_test_skill(skills_dir, "auth", "Authentication patterns", &["login"]);

    let index = SkillIndex::load_from_directory(skills_dir).unwrap();

    // Single word match should not pass threshold of 2.0
    let matches = index.match_skills("implement login", 5);
    assert!(matches.is_empty());
}

#[test]
fn test_empty_index_match() {
    let index = SkillIndex::new();
    let matches = index.match_skills("any text", 5);
    assert!(matches.is_empty());
}

// --- CSV trigger_keywords parsing tests ---

#[test]
fn test_parse_csv_triggers_basic() {
    let result = parse_csv_triggers("login, password, token");
    assert_eq!(result, vec!["login", "password", "token"]);
}

#[test]
fn test_parse_csv_triggers_whitespace() {
    let result = parse_csv_triggers("  login , password ,  token  ");
    assert_eq!(result, vec!["login", "password", "token"]);
}

#[test]
fn test_parse_csv_triggers_empty_entries() {
    let result = parse_csv_triggers("login,,password,  ,token");
    assert_eq!(result, vec!["login", "password", "token"]);
}

#[test]
fn test_parse_csv_triggers_empty_string() {
    let result = parse_csv_triggers("");
    assert!(result.is_empty());
}

// --- Description-embedded trigger extraction tests ---

#[test]
fn test_extract_description_triggers_uppercase() {
    let desc = "Authentication patterns.\n\nTRIGGERS: login, logout, password";
    let result = extract_description_triggers(desc);
    assert_eq!(result, vec!["login", "logout", "password"]);
}

#[test]
fn test_extract_description_triggers_keyword_format() {
    let desc = "Debugging tool. Trigger keywords: debug, bug, error, crash";
    let result = extract_description_triggers(desc);
    assert_eq!(result, vec!["debug", "bug", "error", "crash"]);
}

#[test]
fn test_extract_description_triggers_none() {
    let desc = "A simple skill with no trigger information";
    let result = extract_description_triggers(desc);
    assert!(result.is_empty());
}

#[test]
fn test_extract_description_triggers_multiline() {
    let desc = "Some description.\n\nUSE WHEN: things.\n\nTRIGGERS: a, b, c.";
    let result = extract_description_triggers(desc);
    // Trailing punctuation is stripped from triggers
    assert_eq!(result, vec!["a", "b", "c"]);
}

#[test]
fn unicode_description_markers_load_without_offset_changes() {
    let temp_dir = TempDir::new().unwrap();
    for (name, description) in [
        ("keywords", "İTrigger keywords:é, café."),
        ("triggers", "ıİtRiGgErS:é, café."),
    ] {
        create_test_skill_description_triggers(temp_dir.path(), name, description);
        assert_eq!(extract_description_triggers(description), ["é", "café"]);
    }
    let index = SkillIndex::load_from_directory(temp_dir.path()).unwrap();
    assert_eq!(index.skill_count(), 2);
    assert_eq!(index.match_skills("é café", 5).len(), 2);
}

#[test]
fn explicit_triggers_bypass_unicode_description_fallback() {
    let temp_dir = TempDir::new().unwrap();
    let skill_dir = temp_dir.path().join("explicit");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        concat!(
            "---\nname: explicit\ndescription: 'İTrigger keywords:é, café'\n",
            "triggers: [alpha, beta]\ntrigger-keywords: gamma, delta\n---\n"
        ),
    )
    .unwrap();
    let index = SkillIndex::load_from_directory(temp_dir.path()).unwrap();
    assert_eq!(index.match_skills("alpha beta", 5).len(), 1);
    assert!(index.match_skills("é café gamma delta", 5).is_empty());
}

// --- trigger-keywords YAML field tests ---

fn create_test_skill_with_trigger_keywords(
    dir: &Path,
    name: &str,
    description: &str,
    trigger_keywords: &str,
) {
    let skill_dir = dir.join(name);
    fs::create_dir_all(&skill_dir).unwrap();

    let skill_file = skill_dir.join("SKILL.md");
    let mut file = fs::File::create(&skill_file).unwrap();

    writeln!(file, "---").unwrap();
    writeln!(file, "name: {name}").unwrap();
    writeln!(file, "description: {description}").unwrap();
    writeln!(file, "trigger-keywords: {trigger_keywords}").unwrap();
    writeln!(file, "---").unwrap();
    writeln!(file, "# {name}").unwrap();
}

fn create_test_skill_description_triggers(dir: &Path, name: &str, description: &str) {
    let skill_dir = dir.join(name);
    fs::create_dir_all(&skill_dir).unwrap();

    let skill_file = skill_dir.join("SKILL.md");
    let mut file = fs::File::create(&skill_file).unwrap();

    // Use YAML quoted string to avoid colon parsing issues
    writeln!(file, "---").unwrap();
    writeln!(file, "name: {name}").unwrap();
    writeln!(
        file,
        "description: \"{}\"",
        description.replace('"', "\\\"")
    )
    .unwrap();
    writeln!(file, "---").unwrap();
    writeln!(file, "# {name}").unwrap();
}

#[test]
fn test_load_trigger_keywords_csv() {
    let temp_dir = TempDir::new().unwrap();
    let skills_dir = temp_dir.path();

    create_test_skill_with_trigger_keywords(
        skills_dir,
        "ci-cd",
        "CI/CD pipeline management",
        "pipeline, deploy, build, GitHub Actions",
    );

    let index = SkillIndex::load_from_directory(skills_dir).unwrap();
    assert_eq!(index.skill_count(), 1);

    // Two word matches should exceed threshold
    let matches = index.match_skills("deploy a new build pipeline", 5);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "ci-cd");
    assert!(matches[0].score >= 2.0);
}

#[test]
fn test_load_description_embedded_triggers() {
    let temp_dir = TempDir::new().unwrap();
    let skills_dir = temp_dir.path();

    create_test_skill_description_triggers(
        skills_dir,
        "debugging",
        "Systematic debugging. Trigger keywords: debug, bug, error, crash, investigate",
    );

    let index = SkillIndex::load_from_directory(skills_dir).unwrap();
    assert_eq!(index.skill_count(), 1);

    let matches = index.match_skills("debug the crash in login", 5);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "debugging");
    assert!(matches[0].score >= 2.0);
}

#[test]
fn test_yaml_triggers_take_priority_over_csv() {
    let temp_dir = TempDir::new().unwrap();
    let skills_dir = temp_dir.path();

    // Skill with both triggers array AND trigger-keywords — triggers should win
    let skill_dir = skills_dir.join("auth");
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_file = skill_dir.join("SKILL.md");
    let mut file = fs::File::create(&skill_file).unwrap();
    writeln!(file, "---").unwrap();
    writeln!(file, "name: auth").unwrap();
    writeln!(file, "description: Auth patterns").unwrap();
    writeln!(file, "triggers:").unwrap();
    writeln!(file, "  - login").unwrap();
    writeln!(file, "  - password").unwrap();
    writeln!(file, "trigger-keywords: deploy, build").unwrap();
    writeln!(file, "---").unwrap();

    let index = SkillIndex::load_from_directory(skills_dir).unwrap();

    // "login password" should match (from triggers array)
    let matches = index.match_skills("login with password", 5);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "auth");

    // "deploy build" should NOT match (trigger-keywords ignored when triggers exists)
    let matches = index.match_skills("deploy a build", 5);
    assert!(matches.is_empty());
}

#[test]
fn test_get_by_name() {
    let temp_dir = TempDir::new().unwrap();
    let skills_dir = temp_dir.path();

    create_test_skill(skills_dir, "auth", "Authentication patterns", &["login"]);
    create_test_skill(skills_dir, "testing", "Testing patterns", &["test"]);

    let index = SkillIndex::load_from_directory(skills_dir).unwrap();

    let auth = index.get_by_name("auth");
    assert!(auth.is_some());
    assert_eq!(auth.unwrap().name, "auth");
    assert_eq!(auth.unwrap().description, "Authentication patterns");

    let testing = index.get_by_name("testing");
    assert!(testing.is_some());

    let nonexistent = index.get_by_name("nonexistent");
    assert!(nonexistent.is_none());
}

#[test]
fn test_csv_triggers_take_priority_over_description() {
    let temp_dir = TempDir::new().unwrap();
    let skills_dir = temp_dir.path();

    // Skill with trigger-keywords CSV AND description triggers — CSV should win
    let skill_dir = skills_dir.join("mixed");
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_file = skill_dir.join("SKILL.md");
    let mut file = fs::File::create(&skill_file).unwrap();
    writeln!(file, "---").unwrap();
    writeln!(file, "name: mixed").unwrap();
    writeln!(
        file,
        "description: \"A skill. Trigger keywords: alpha, beta\""
    )
    .unwrap();
    writeln!(file, "trigger-keywords: gamma, delta").unwrap();
    writeln!(file, "---").unwrap();

    let index = SkillIndex::load_from_directory(skills_dir).unwrap();

    // "gamma delta" should match (from trigger-keywords CSV)
    let matches = index.match_skills("gamma and delta", 5);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "mixed");

    // "alpha beta" should NOT match (description triggers ignored when CSV exists)
    let matches = index.match_skills("alpha and beta", 5);
    assert!(matches.is_empty());
}
