//! Environment lints of `plan verify`: registry domains here, the repository's
//! pre-commit hook and JS provision lints in `repo_lints`. Each case runs
//! through the lint entry point `run`, the path `plan verify` takes.

use std::ffi::OsStr;
use std::path::Path;

use tempfile::TempDir;

use crate::git::runner::run_git_with_env;
use crate::plan::schema::validation::v2_lints::{run, LintContext, LintFinding};
use crate::plan::schema::{
    AcceptanceCriterion, LoomConfig, LoomMetadata, NetworkConfig, StageDefinition,
    StageSandboxConfig, StageType,
};

#[path = "v2_lint_environment_repo_tests.rs"]
mod repo_lints;

const NEEDS: &str = ", which needs ";

fn stage(id: &str, commands: &[&str]) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: id.to_string(),
        working_dir: ".".to_string(),
        stage_type: Some(StageType::Standard),
        acceptance: commands
            .iter()
            .map(|command| AcceptanceCriterion::Simple(command.to_string()))
            .collect(),
        ..Default::default()
    }
}

fn plan(version: u32, stages: Vec<StageDefinition>) -> LoomMetadata {
    LoomMetadata {
        loom: LoomConfig {
            version,
            stages,
            ..Default::default()
        },
    }
}

fn lint(metadata: &LoomMetadata, repo_root: Option<&Path>) -> (Vec<LintFinding>, Vec<String>) {
    let mut notes = Vec::new();
    let ctx = LintContext {
        metadata,
        repo_root,
    };
    let findings = run(&ctx, &mut notes);
    (findings, notes)
}

/// The messages of the findings that contain `needle`.
fn messages(metadata: &LoomMetadata, repo_root: Option<&Path>, needle: &str) -> Vec<String> {
    let (findings, _) = lint(metadata, repo_root);
    findings
        .into_iter()
        .map(|finding| finding.message)
        .filter(|message| message.contains(needle))
        .collect()
}

fn network(domains: &[&str]) -> NetworkConfig {
    NetworkConfig {
        allowed_domains: domains.iter().map(|domain| domain.to_string()).collect(),
        ..Default::default()
    }
}

/// A one-stage plan whose plan-level network allows `domains`.
fn plan_allowing(domains: &[&str], commands: &[&str]) -> LoomMetadata {
    let mut metadata = plan(2, vec![stage("feature", commands)]);
    metadata.loom.sandbox.network = network(domains);
    metadata
}

fn write(root: &Path, relative: &str, body: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create dirs");
    std::fs::write(path, body).expect("write file");
}

/// Run git in `dir` with the machine's global and system config pointed at missing
/// files, so a developer's `core.hooksPath` or `init.templateDir` cannot reach the
/// setup. The process environment is left alone.
fn isolated_git(args: &[&str], dir: &Path) {
    let no_global = dir.join(".no-global-config");
    let no_system = dir.join(".no-system-config");
    let env: [(&str, &OsStr); 3] = [
        ("GIT_CONFIG_GLOBAL", no_global.as_os_str()),
        ("GIT_CONFIG_SYSTEM", no_system.as_os_str()),
        ("GIT_CONFIG_NOSYSTEM", OsStr::new("1")),
    ];
    let output = run_git_with_env(args, &env, dir).expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A fresh repository whose LOCAL `core.hooksPath` names a directory that does not
/// exist, so the hook lint finds no hook whatever the machine's global or system
/// config says (the local scope wins). Tests that need a hook override the setting.
fn git_repo() -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    isolated_git(&["init", "-q"], dir.path());
    isolated_git(&["config", "core.hooksPath", "no-such-hooks"], dir.path());
    dir
}

#[test]
fn every_registry_command_reports_its_domains() {
    let rows = [
        ("bunx tsc --noEmit", "bunx", "registry.npmjs.org"),
        ("npx eslint .", "npx", "registry.npmjs.org"),
        ("bun x vitest", "bun x", "registry.npmjs.org"),
        ("pnpm dlx tsc", "pnpm dlx", "registry.npmjs.org"),
        ("yarn dlx tsc", "yarn dlx", "registry.npmjs.org"),
        ("npm install", "npm install", "registry.npmjs.org"),
        ("npm i left-pad", "npm i", "registry.npmjs.org"),
        ("npm ci", "npm ci", "registry.npmjs.org"),
        ("bun add zod", "bun add", "registry.npmjs.org"),
        ("pnpm install", "pnpm install", "registry.npmjs.org"),
        ("yarn add zod", "yarn add", "registry.npmjs.org"),
        ("cargo install ripgrep", "cargo install", "static.crates.io"),
        ("cargo fetch --locked", "cargo fetch", "index.crates.io"),
        ("uv sync", "uv sync", "files.pythonhosted.org"),
        ("uv add requests", "uv add", "pypi.org"),
        ("uv pip install requests", "uv pip install", "pypi.org"),
        ("uvx ruff check", "uvx", "pypi.org"),
        ("pip install -r r.txt", "pip install", "pypi.org"),
        ("pip3 install x", "pip3 install", "pypi.org"),
        (
            "python -m pip install x",
            "python -m pip install",
            "pypi.org",
        ),
        (
            "python3 -m pip install x",
            "python3 -m pip install",
            "pypi.org",
        ),
        ("go get ./...", "go get", "proxy.golang.org"),
        ("go mod download", "go mod download", "proxy.golang.org"),
    ];
    for (command, label, domain) in rows {
        let found = messages(&plan_allowing(&[], &[command]), None, NEEDS);
        assert_eq!(found.len(), 1, "{command}: {found:?}");
        assert!(found[0].contains(&format!("runs `{label}`")), "{found:?}");
        assert!(found[0].contains(domain), "{command}: {found:?}");
        assert!(found[0].contains("`provision` entry"), "{found:?}");
    }
}

#[test]
fn commands_that_fetch_nothing_are_clean() {
    let commands = [
        "npm test",
        "npm run build",
        "bun test",
        "cargo build",
        "cargo test --lib",
        "go test ./...",
        "pip list",
        "uv run pytest",
        "python script.py pip install",
        "xargs -0 rg foo",
        "xargs",
    ];
    for command in commands {
        let found = messages(&plan_allowing(&[], &[command]), None, NEEDS);
        assert!(found.is_empty(), "{command}: {found:?}");
    }
}

#[test]
fn xargs_options_are_skipped() {
    let commands = [
        "git ls-files -z | xargs -0 bunx lint",
        "git ls-files | xargs -n 1 -I X -P 2 bunx lint X",
        "echo a | xargs -r npx prettier",
    ];
    for command in commands {
        let found = messages(&plan_allowing(&[], &[command]), None, NEEDS);
        assert_eq!(found.len(), 1, "{command}: {found:?}");
    }
}

#[test]
fn registry_domain_patterns_follow_the_sandbox_rules() {
    let allowed = [
        vec!["registry.npmjs.org"],
        vec!["*.npmjs.org"],
        vec!["*"],
        vec!["Registry.NPMJS.org"],
    ];
    for domains in allowed {
        let found = messages(&plan_allowing(&domains, &["bunx tsc"]), None, NEEDS);
        assert!(found.is_empty(), "{domains:?}: {found:?}");
    }
    let refused = [
        vec!["npmjs.org"],
        vec!["*.registry.npmjs.org"],
        vec!["crates.io"],
    ];
    for domains in refused {
        let found = messages(&plan_allowing(&domains, &["bunx tsc"]), None, NEEDS);
        assert_eq!(found.len(), 1, "{domains:?}: {found:?}");
    }
}

#[test]
fn only_the_missing_domains_are_named() {
    let metadata = plan_allowing(&["crates.io"], &["cargo install ripgrep"]);
    let found = messages(&metadata, None, NEEDS);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("does not allow index.crates.io, static.crates.io:"));
    let mut extended = metadata;
    extended.loom.sandbox.network.additional_domains = vec!["*.crates.io".to_string()];
    assert!(messages(&extended, None, NEEDS).is_empty());
}

#[test]
fn a_stage_network_replaces_the_plan_network() {
    let mut narrowed = plan_allowing(&["registry.npmjs.org"], &["bunx tsc"]);
    narrowed.loom.stages[0].sandbox = StageSandboxConfig {
        network: Some(network(&["crates.io"])),
        ..Default::default()
    };
    assert_eq!(messages(&narrowed, None, NEEDS).len(), 1);

    let mut widened = plan_allowing(&[], &["bunx tsc"]);
    widened.loom.stages[0].sandbox = StageSandboxConfig {
        network: Some(network(&["registry.npmjs.org"])),
        ..Default::default()
    };
    assert!(messages(&widened, None, NEEDS).is_empty());
}

#[test]
fn a_disabled_sandbox_needs_no_domain() {
    let mut stage_off = plan_allowing(&[], &["bunx tsc"]);
    stage_off.loom.stages[0].sandbox.enabled = Some(false);
    let mut plan_off = plan_allowing(&[], &["bunx tsc"]);
    plan_off.loom.sandbox.enabled = false;
    let mut stage_on = plan_off.clone();
    stage_on.loom.stages[0].sandbox.enabled = Some(true);
    assert!(messages(&stage_off, None, NEEDS).is_empty());
    assert!(messages(&plan_off, None, NEEDS).is_empty());
    assert_eq!(messages(&stage_on, None, NEEDS).len(), 1);
}

#[test]
fn before_stage_runs_outside_the_sandbox() {
    let mut metadata = plan_allowing(&[], &[]);
    let checks = "- command: bunx tsc";
    metadata.loom.stages[0].before_stage = crate::yaml::from_str(checks).expect("check");
    assert!(messages(&metadata, None, NEEDS).is_empty());
    metadata.loom.stages[0].after_stage = crate::yaml::from_str(checks).expect("check");
    assert_eq!(messages(&metadata, None, NEEDS).len(), 1);
}

#[test]
fn one_finding_per_command_and_tool() {
    let metadata = plan_allowing(&[], &["bunx a && bunx b && npm install && $(bunx c)"]);
    let found = messages(&metadata, None, NEEDS);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found
        .iter()
        .all(|message| message.contains("Acceptance criterion #1")));
}

#[test]
fn registry_installs_are_not_reported_twice_without_a_domain() {
    let metadata = plan_allowing(&[], &["npm install", "curl https://x.example"]);
    let (findings, _) = lint(&metadata, None);
    let installs = findings
        .iter()
        .filter(|f| f.message.contains("`npm install`"));
    assert_eq!(installs.count(), 1, "{findings:?}");
    let network = findings
        .iter()
        .filter(|f| f.message.contains("allows no network domain"));
    assert_eq!(network.count(), 1, "{findings:?}");
    assert!(findings.iter().all(|finding| finding.error_in_v2));
}
