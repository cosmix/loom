//! Criterion hazard lints: each error and warning next to a twin that passes.

use crate::plan::schema::tests::{create_valid_metadata, make_stage};
use crate::plan::schema::types::{AcceptanceCriterion, StageDefinition, WiringTest};
use crate::plan::schema::validation::criterion_hazards::{scan, Hazard};
use crate::plan::schema::validation::{validate, validate_structural_preflight};

fn hazards(command: &str) -> Vec<Hazard> {
    scan(command, false).into_iter().collect()
}

fn assert_flags(command: &str, hazard: Hazard) {
    assert!(
        scan(command, false).contains(&hazard),
        "expected {hazard:?} in `{command}`, got {:?}",
        hazards(command)
    );
}

fn assert_clean(command: &str) {
    assert!(
        hazards(command).is_empty(),
        "expected no hazard in `{command}`, got {:?}",
        hazards(command)
    );
}

fn wiring_test(command: &str) -> WiringTest {
    WiringTest {
        name: "entry point".to_string(),
        command: command.to_string(),
        success_criteria: Default::default(),
        description: None,
    }
}

fn stage_with_acceptance(command: &str) -> StageDefinition {
    let mut stage = make_stage("feature", "Feature");
    stage.acceptance = vec![AcceptanceCriterion::Simple(command.to_string())];
    stage
}

fn validation_errors(stage: StageDefinition) -> Vec<String> {
    let mut metadata = create_valid_metadata();
    metadata.loom.stages = vec![stage];
    match validate(&metadata) {
        Ok(()) => Vec::new(),
        Err(errors) => errors.into_iter().map(|e| e.to_string()).collect(),
    }
}

#[test]
fn masked_exit_status_is_an_error() {
    assert_flags("cargo test || true", Hazard::MaskedExit);
    assert_flags("cargo test ||:", Hazard::MaskedExit);
    assert_flags("bash -c 'cargo test || true'", Hazard::MaskedExit);
    assert!(Hazard::MaskedExit.is_error());
}

#[test]
fn masked_exit_inside_quoted_pattern_is_not_a_hazard() {
    assert_clean(r#"rg -q "|| true" loom-hooks/check.sh"#);
    assert_clean("cargo test || cargo test --release");
}

#[test]
fn home_from_expansion_is_an_error() {
    assert_flags("HOME=$TMPDIR cargo test", Hazard::HomeFromExpansion);
    assert_flags(r#"export HOME="$(pwd)/h""#, Hazard::HomeFromExpansion);
    assert_flags("declare -x HOME=$H", Hazard::HomeFromExpansion);
    assert_flags("local HOME=$x", Hazard::HomeFromExpansion);
    assert!(Hazard::HomeFromExpansion.is_error());
}

#[test]
fn home_from_literal_or_quoted_pattern_is_not_a_hazard() {
    assert_clean("HOME=/home/tester cargo test");
    assert_clean(r#"rg -q 'HOME=$TMPDIR' loom-hooks/check.sh"#);
}

#[test]
fn home_shaped_word_outside_assignment_position_is_not_a_hazard() {
    // `HOME=...` only sets the environment in the shell-assignment prefix
    // before the command name. The same shape as a plain argument (an
    // `export` operand, a tool's own `-v NAME=value` flag) sets something
    // else entirely and is not this hazard.
    assert_clean("awk -v HOME=$dir '{print}' f");
}

#[test]
fn bare_mktemp_dir_is_an_error() {
    assert_flags(r#"d=$(mktemp -d) && test -d "$d""#, Hazard::BareMktempDir);
    assert_flags("mktemp -dt checks", Hazard::BareMktempDir);
    assert!(Hazard::BareMktempDir.is_error());
}

#[test]
fn mktemp_dir_under_tmpdir_is_not_a_hazard() {
    assert_clean(r#"d=$(mktemp -d "${TMPDIR:-/tmp}/c.XXXXXX")"#);
    assert_clean(r#"f=$(mktemp "${TMPDIR:-/tmp}/out.XXXXXX")"#);
}

#[test]
fn network_binaries_warn() {
    assert_flags("curl -fsS https://example.com", Hazard::Network("curl"));
    assert_flags("wget -q https://example.com", Hazard::Network("wget"));
    assert_flags("gh pr view 1", Hazard::Network("gh"));
    assert_flags("npm install && npm test", Hazard::Network("npm install"));
    assert_flags("bun install", Hazard::Network("bun install"));
    assert_flags("cargo install ripgrep", Hazard::Network("cargo install"));
    assert_flags("cargo audit", Hazard::Network("cargo audit"));
    assert!(!Hazard::Network("curl").is_error());
}

#[test]
fn network_names_outside_command_position_do_not_warn() {
    assert_clean("cargo audit --no-fetch");
    assert_clean("rg -q curl loom-hooks/fetch.sh");
    assert_clean("bun test src/install.test.ts");
}

#[test]
fn plan_paths_warn_unless_they_are_the_pattern() {
    assert_flags("test -f doc/plans/PLAN-feature.md", Hazard::PlanPath);
    assert_clean(r#"rg -q "doc/plans/" src/plan_paths.rs"#);
}

#[test]
fn vitest_name_filter_warns() {
    assert_flags(r#"bunx vitest -t "adds""#, Hazard::VitestNameFilter);
    assert_flags("vitest --testNamePattern=adds", Hazard::VitestNameFilter);
    assert_clean("bunx vitest run src/math.test.ts");
}

#[test]
fn pipestatus_warns() {
    assert_flags(
        "cargo test | tee test.log; exit ${PIPESTATUS[0]}",
        Hazard::PipeStatus,
    );
    assert_clean("rg -q PIPESTATUS loom-hooks/run.sh");
}

#[test]
fn rg_replace_flag_warns() {
    assert_flags(r#"rg -rn "fn main" src"#, Hazard::RgReplace);
    assert_flags("rg --replace=x main src", Hazard::RgReplace);
    assert_clean(r#"rg -n "fn main" src"#);
    assert_clean("grep -rn main src");
    assert_clean(r#"rg -q -e "-r" src"#);
}

#[test]
fn test_runner_warns_only_inside_wiring_tests() {
    for runner in [
        "cargo test --lib",
        "cargo nextest run",
        "bun test",
        "go test ./...",
        "uv run pytest tests",
        "bunx vitest run",
    ] {
        assert!(
            scan(runner, true).contains(&Hazard::TestRunnerInWiring),
            "{runner}"
        );
        assert!(
            !scan(runner, false).contains(&Hazard::TestRunnerInWiring),
            "{runner}"
        );
    }
    assert!(scan("./target/debug/loom --help", true).is_empty());
}

#[test]
fn hardcoded_tmp_warns_unless_it_is_the_tmpdir_value() {
    assert_flags("mkdir -p /tmp/checks", Hazard::HardcodedTmp);
    assert_flags("cargo test > /tmp/out.log", Hazard::HardcodedTmp);
    assert_clean("TMPDIR=/tmp/checks cargo test --lib");
    assert_clean(r#"mkdir -p "${TMPDIR:-/tmp}/checks""#);
}

#[test]
fn heredoc_bodies_and_comments_are_data() {
    assert_clean("cat > s.sh <<'EOF'\ncurl x || true\nEOF\ncargo test");
    assert_clean("cargo test # || true");
}

#[test]
fn validate_reports_hazard_errors_for_every_command_kind() {
    let errors = validation_errors(stage_with_acceptance("cargo test || true"));
    assert!(
        errors
            .iter()
            .any(|e| e.contains("Stage 'feature'") && e.contains("Acceptance criterion #1")),
        "{errors:?}"
    );

    let mut stage = stage_with_acceptance("cargo test");
    stage.setup = vec!["d=$(mktemp -d)".to_string()];
    stage.wiring_tests = vec![wiring_test("HOME=$TMPDIR ./target/debug/loom --help")];
    let errors = validation_errors(stage);
    assert!(
        errors.iter().any(|e| e.contains("Setup command #1")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("Wiring test #1 'entry point'")),
        "{errors:?}"
    );
}

#[test]
fn validate_accepts_clean_commands() {
    let mut stage = stage_with_acceptance(r#"rg -q "|| true" src/lib.rs"#);
    stage.setup = vec![r#"mkdir -p "${TMPDIR:-/tmp}/checks""#.to_string()];
    stage.wiring_tests = vec![wiring_test("./target/debug/loom --help")];
    assert_eq!(validation_errors(stage), Vec::<String>::new());
}

#[test]
fn preflight_reports_hazard_warnings_for_every_command_kind() {
    let mut stage = stage_with_acceptance("curl -fsS https://example.com");
    stage.setup = vec!["mkdir -p /tmp/checks".to_string()];
    stage.wiring_tests = vec![wiring_test("cargo test --lib wiring")];
    let warnings = validate_structural_preflight(&[stage], None);
    let has = |needle: &str, text: &str| {
        warnings
            .iter()
            .any(|w| w.contains(needle) && w.contains(text))
    };
    assert!(
        has("Acceptance criterion #1", "network access"),
        "{warnings:?}"
    );
    assert!(has("Setup command #1", "hardcodes /tmp/"), "{warnings:?}");
    assert!(has("Wiring test #1", "test runner"), "{warnings:?}");
}

#[test]
fn preflight_keeps_hazard_errors_out_of_warnings() {
    let stage = stage_with_acceptance("cargo test || true");
    let warnings = validate_structural_preflight(&[stage], None);
    assert!(
        warnings
            .iter()
            .all(|w| !w.contains("masks its exit status")),
        "{warnings:?}"
    );
}
