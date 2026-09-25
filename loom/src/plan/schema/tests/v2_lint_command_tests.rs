//! Command-level `plan verify` lints: each flagged form next to its clean twin.

use regex::{Regex, RegexBuilder};

use crate::plan::schema::{
    LoomMetadata, NetworkConfig, StageSandboxConfig, StageType, WiringCheck,
};
use crate::verify::goal_backward::wiring::PATTERN_SIZE_LIMIT;

use super::{lint_with_notes, matching, plan, stage};

fn standard(commands: &[&str]) -> LoomMetadata {
    plan(vec![stage("feature", StageType::Standard, commands)])
}

fn wiring(pattern: &str, literal: bool) -> WiringCheck {
    WiringCheck {
        source: "src/lib.rs".to_string(),
        pattern: pattern.to_string(),
        description: "entry point is wired".to_string(),
        literal,
    }
}

#[test]
fn known_loom_subcommands_are_clean() {
    let metadata = standard(&[
        "loom knowledge check --strict --baseline doc/baseline.txt",
        "loom stage complete feature",
        "loom --version",
        "loom knowledge --help",
        "./target/debug/loom status",
    ]);
    assert!(matching(&metadata, None, "usage error").is_empty());
}

#[test]
fn loom_parent_without_its_subcommand_is_flagged() {
    let metadata = standard(&["loom knowledge"]);
    let found = matching(&metadata, None, "without the subcommand it requires");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].error_in_v2);
}

#[test]
fn unknown_loom_subcommand_inside_sh_c_is_found() {
    let metadata = standard(&["sh -c 'loom knowledge verify --strict'"]);
    assert_eq!(matching(&metadata, None, "is not a subcommand").len(), 1);
}

#[test]
fn unknown_loom_subcommand_without_cli_stage_is_still_error_in_v2() {
    let metadata = standard(&["loom foo bar"]);
    let found = matching(&metadata, None, "is not a subcommand");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].error_in_v2);
}

#[test]
fn unknown_loom_subcommand_when_plan_adds_cli_files_is_a_warning() {
    let mut adds_cli = stage("add-cli", StageType::Standard, &[]);
    adds_cli.artifacts = vec!["loom/src/cli/foo.rs".to_string()];
    let calls_it = stage("feature", StageType::Standard, &["loom foo bar"]);
    let metadata = plan(vec![adds_cli, calls_it]);
    let found = matching(&metadata, None, "is not a subcommand");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(!found[0].error_in_v2);
    assert!(found[0].message.contains("loom/src/cli"));
}

#[test]
fn bare_glob_files_entry_touches_every_dir_is_a_warning() {
    let mut adds_cli = stage("add-cli", StageType::Standard, &[]);
    adds_cli.files = vec!["**".to_string()];
    let calls_it = stage("feature", StageType::Standard, &["loom foo bar"]);
    let metadata = plan(vec![adds_cli, calls_it]);
    let found = matching(&metadata, None, "is not a subcommand");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(!found[0].error_in_v2);
    assert!(found[0].message.contains("loom/src/cli"));
}

#[test]
fn protected_dash_patterns_are_clean() {
    let metadata = standard(&[
        r#"rg -qF -e "--out" src/x.rs"#,
        r#"rg -qF -- "--out" src/x.rs"#,
        r#"rg -g "-generated" pattern src"#,
        "rg --glob='-x' pattern src",
        r#"grep -q -- "-flag" src/x.rs"#,
    ]);
    assert!(matching(&metadata, None, "reads it as a flag").is_empty());
}

#[test]
fn search_pattern_that_fails_to_compile_is_error_in_v2() {
    let metadata = standard(&[r#"rg -q "foo(" src"#, r#"grep -qE "bar(" src"#]);
    let found = matching(&metadata, None, "does not compile");
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|finding| finding.error_in_v2));
}

#[test]
fn fixed_escaped_and_basic_patterns_are_not_compiled() {
    let metadata = standard(&[
        r#"rg -qF "foo(" src"#,
        r#"rg -q "foo\(" src"#,
        r#"grep -q "foo(" src"#,
        r#"rg -qP "(?<=a)b" src"#,
    ]);
    assert!(matching(&metadata, None, "does not compile").is_empty());
}

#[test]
fn double_bracket_in_search_pattern_warns() {
    let metadata = standard(&[r#"rg -q "[[bin]]" Cargo.toml"#]);
    let found = matching(&metadata, None, "contains `[[`");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(!found[0].error_in_v2);
}

#[test]
fn literal_and_posix_brackets_are_clean() {
    let metadata = standard(&[
        r#"rg -qF "[[bin]]" Cargo.toml"#,
        r#"rg -q "\[\[bin\]\]" Cargo.toml"#,
        r#"grep -q "[[:alpha:]]x" src/x.rs"#,
    ]);
    assert!(matching(&metadata, None, "contains `[[`").is_empty());
}

#[test]
fn wiring_double_bracket_warns_unless_literal() {
    let mut flagged = stage("feature", StageType::Standard, &[]);
    flagged.wiring = vec![wiring("[[stage]]", false), wiring("[[stage]]", true)];
    let found = matching(&plan(vec![flagged]), None, "contains `[[`");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].message.starts_with("Wiring #1 "));
}

#[test]
fn wiring_pattern_past_the_verifier_size_limit_is_error_in_v2() {
    // The smallest repetition of the Unicode class the verifier's 1 MiB limit
    // rejects, while `Regex::new` (and so `validate()`) still accepts it.
    let oversized = (1..64)
        .map(|count| format!(r"\w{{{count}}}"))
        .find(|pattern| {
            RegexBuilder::new(pattern)
                .size_limit(PATTERN_SIZE_LIMIT)
                .build()
                .is_err()
        })
        .expect("a repetition past 1 MiB");
    assert!(
        Regex::new(&oversized).is_ok(),
        "{oversized} must pass validate()"
    );
    let mut flagged = stage("feature", StageType::Standard, &[]);
    flagged.wiring = vec![wiring(&oversized, false), wiring(&oversized, true)];
    let found = matching(&plan(vec![flagged]), None, "size limit");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].error_in_v2);
}

#[test]
fn ordinary_wiring_pattern_is_clean() {
    let mut clean = stage("feature", StageType::Standard, &[]);
    clean.wiring = vec![wiring(r"fn\s+run\b", false)];
    let (findings, _) = lint_with_notes(&plan(vec![clean]), None);
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn network_binary_with_a_granted_domain_is_clean() {
    let mut granted = stage("feature", StageType::Standard, &["curl https://x.example"]);
    granted.sandbox = StageSandboxConfig {
        network: Some(NetworkConfig {
            allowed_domains: vec!["x.example".to_string()],
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut plan_granted = plan(vec![stage("other", StageType::Standard, &["gh pr view"])]);
    plan_granted.loom.sandbox.network.additional_domains = vec!["github.com".to_string()];
    for metadata in [plan(vec![granted]), plan_granted] {
        assert!(matching(&metadata, None, "allows no network domain").is_empty());
    }
}

#[test]
fn ungrantable_resource_is_error_in_v2() {
    let metadata = standard(&["tmux ls", "loom map --outline src/lib.rs"]);
    let found = matching(&metadata, None, "no sandbox grant reaches");
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|finding| finding.error_in_v2));
}

#[test]
fn grantable_commands_are_clean() {
    let metadata = standard(&["echo tmux-free", "loom knowledge check"]);
    assert!(matching(&metadata, None, "no sandbox grant reaches").is_empty());
}
