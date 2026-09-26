use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

use super::super::helpers::loom_cmd;

const CLAUDE_OVERRIDE: &str = "LOOM_CLAUDECODE_INSTALL_DIR";
const CODEX_OVERRIDE: &str = "LOOM_CODEX_INSTALL_DIR";

fn install_command(home: &Path) -> Command {
    let mut command = loom_cmd();
    command
        .env("HOME", home)
        .env_remove(CLAUDE_OVERRIDE)
        .env_remove(CODEX_OVERRIDE)
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .arg("install-assets");
    command
}

fn run(mut command: Command) -> Output {
    let output = command.output().expect("run loom install-assets");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn assert_core_trees(claude: &Path, codex: &Path) {
    for path in [
        claude.join("CLAUDE.md"),
        claude.join("agents/loom-software-engineer.md"),
        claude.join("commands/pressure.md"),
        claude.join("hooks/loom/post-tool-use.sh"),
        claude.join("skills/loom-plan-writer/SKILL.md"),
        claude.join("hooks/loom/skill-keywords.json"),
        codex.join("AGENTS.md"),
        codex.join("hooks.json"),
        codex.join("hooks/loom/codex-apply-patch.sh"),
        codex.join("skills/pressure/SKILL.md"),
        codex.join("hooks/loom/skill-keywords.json"),
    ] {
        assert!(
            path.is_file(),
            "expected installed asset {}",
            path.display()
        );
    }
    assert_index(claude);
    assert_index(codex);
}

fn assert_index(root: &Path) {
    let path = root.join("hooks/loom/skill-keywords.json");
    let index: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).expect("valid skill index JSON");
    assert!(
        index.as_object().is_some_and(|entries| !entries.is_empty()),
        "expected a populated skill index at {}",
        path.display()
    );
}

fn assert_absent(path: &Path) {
    assert!(
        !path.exists(),
        "expected {} to remain untouched",
        path.display()
    );
}

fn default_completion_paths(home: &Path) -> [PathBuf; 3] {
    [
        home.join(".local/share/bash-completion/completions/loom"),
        home.join(".zfunc/_loom"),
        home.join(".config/fish/completions/loom.fish"),
    ]
}

#[test]
fn unset_and_empty_overrides_use_home_defaults_without_creating_completions() {
    for empty_overrides in [false, true] {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let mut command = install_command(&home);
        command
            .env("CLAUDE_CONFIG_DIR", temp.path().join("native-claude"))
            .env("CODEX_HOME", temp.path().join("native-codex"));
        if empty_overrides {
            command.env(CLAUDE_OVERRIDE, "").env(CODEX_OVERRIDE, "");
        }

        run(command);
        assert_core_trees(&home.join(".claude"), &home.join(".codex"));
        assert_absent(&temp.path().join("native-claude"));
        assert_absent(&temp.path().join("native-codex"));
        for completion in default_completion_paths(&home) {
            assert_absent(&completion);
        }
    }
}

#[test]
fn each_environment_override_resolves_independently() {
    for override_claude in [true, false] {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let custom = temp.path().join("custom agent root");
        fs::create_dir_all(&home).unwrap();
        let mut command = install_command(&home);
        if override_claude {
            command.env(CLAUDE_OVERRIDE, &custom);
            run(command);
            assert_core_trees(&custom, &home.join(".codex"));
            assert_absent(&home.join(".claude"));
        } else {
            command.env(CODEX_OVERRIDE, &custom);
            run(command);
            assert_core_trees(&home.join(".claude"), &custom);
            assert_absent(&home.join(".codex"));
        }
    }
}

fn seed_user_assets(claude: &Path, codex: &Path) -> [PathBuf; 3] {
    let paths = [
        claude.join("skills/my-custom/SKILL.md"),
        claude.join("agents/my-agent.md"),
        codex.join("skills/my-codex-skill/SKILL.md"),
    ];
    for path in &paths {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "user-owned\n").unwrap();
    }
    paths
}

#[test]
fn both_spaced_overrides_preserve_assets_layout_and_completion_refresh() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let claude = temp.path().join(" claude assets ");
    let codex = temp.path().join(" codex assets ");
    let user_assets = seed_user_assets(&claude, &codex);
    let completion = home.join(".zfunc/_loom");
    fs::create_dir_all(completion.parent().unwrap()).unwrap();
    fs::write(&completion, "stale completion\n").unwrap();

    let mut command = install_command(&home);
    command
        .env(CLAUDE_OVERRIDE, &claude)
        .env(CODEX_OVERRIDE, &codex)
        .args(["--skills", "all"]);
    run(command);

    assert_core_trees(&claude, &codex);
    assert!(claude.join("skills/loom-rust/SKILL.md").is_file());
    assert!(codex.join("skills/loom-rust/SKILL.md").is_file());
    assert_absent(&claude.join("loom-skill-catalog"));
    assert_absent(&codex.join("loom-skill-catalog"));
    assert!(fs::read_to_string(claude.join("loom-install.toml"))
        .unwrap()
        .contains("skills = \"all\""));
    for path in user_assets {
        assert_eq!(fs::read_to_string(path).unwrap(), "user-owned\n");
    }
    assert_absent(&home.join(".claude"));
    assert_absent(&home.join(".codex"));
    assert_ne!(
        fs::read_to_string(completion).unwrap(),
        "stale completion\n"
    );
}

fn seed_stale_completion(home: &Path) -> PathBuf {
    let completion = home.join(".zfunc/_loom");
    fs::create_dir_all(completion.parent().unwrap()).unwrap();
    fs::write(&completion, "do not refresh\n").unwrap();
    completion
}

#[test]
fn claude_flag_overrides_its_env_while_codex_uses_its_env() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let env_claude = temp.path().join("env-claude");
    let env_codex = temp.path().join("env-codex");
    let cli_claude = temp.path().join("cli-claude");
    let completion = seed_stale_completion(&home);
    let mut command = install_command(&home);
    command
        .env(CLAUDE_OVERRIDE, &env_claude)
        .env(CODEX_OVERRIDE, &env_codex)
        .args(["--claude-dir", cli_claude.to_str().unwrap()]);

    run(command);
    assert_core_trees(&cli_claude, &env_codex);
    assert_absent(&env_claude);
    assert_absent(&home.join(".claude"));
    assert_absent(&home.join(".codex"));
    assert_eq!(fs::read_to_string(completion).unwrap(), "do not refresh\n");
}

#[test]
fn codex_flag_overrides_its_env_while_claude_uses_default() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let env_codex = temp.path().join("env-codex");
    let cli_codex = temp.path().join("cli-codex");
    let completion = seed_stale_completion(&home);
    let mut command = install_command(&home);
    command
        .env(CODEX_OVERRIDE, &env_codex)
        .args(["--codex-dir", cli_codex.to_str().unwrap()]);

    run(command);
    assert_core_trees(&home.join(".claude"), &cli_codex);
    assert_absent(&env_codex);
    assert_absent(&home.join(".codex"));
    assert_eq!(fs::read_to_string(completion).unwrap(), "do not refresh\n");
}

#[test]
fn bare_reinstall_restores_files_deleted_from_recorded_custom_roots() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let claude = temp.path().join("recorded-claude");
    let codex = temp.path().join("recorded-codex");
    fs::create_dir_all(&home).unwrap();

    let mut command = install_command(&home);
    command
        .env(CLAUDE_OVERRIDE, &claude)
        .env(CODEX_OVERRIDE, &codex);
    run(command);
    assert_core_trees(&claude, &codex);

    let claude_agent = claude.join("agents/loom-software-engineer.md");
    let codex_agents_md = codex.join("AGENTS.md");
    fs::remove_file(&claude_agent).unwrap();
    fs::remove_file(&codex_agents_md).unwrap();

    run(install_command(&home));

    assert!(
        claude_agent.is_file(),
        "expected the recorded claude root to be reinstalled"
    );
    assert!(
        codex_agents_md.is_file(),
        "expected the recorded codex root to be reinstalled"
    );
    assert_absent(&home.join(".claude"));
    assert_absent(&home.join(".codex"));
}

#[test]
fn explicit_flags_do_not_overwrite_the_recorded_roots() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let recorded_claude = temp.path().join("recorded-claude");
    let recorded_codex = temp.path().join("recorded-codex");
    let flagged_claude = temp.path().join("flagged-claude");
    let flagged_codex = temp.path().join("flagged-codex");
    fs::create_dir_all(&home).unwrap();

    let mut command = install_command(&home);
    command
        .env(CLAUDE_OVERRIDE, &recorded_claude)
        .env(CODEX_OVERRIDE, &recorded_codex);
    run(command);

    let mut flagged = install_command(&home);
    flagged
        .args(["--claude-dir", flagged_claude.to_str().unwrap()])
        .args(["--codex-dir", flagged_codex.to_str().unwrap()]);
    run(flagged);
    assert_core_trees(&flagged_claude, &flagged_codex);

    run(install_command(&home));
    assert_core_trees(&recorded_claude, &recorded_codex);
    assert_absent(&home.join(".claude"));
    assert_absent(&home.join(".codex"));
}

#[test]
fn an_env_override_wins_over_a_previously_recorded_root() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let first_claude = temp.path().join("first-claude");
    let first_codex = temp.path().join("first-codex");
    let second_claude = temp.path().join("second-claude");
    let second_codex = temp.path().join("second-codex");
    fs::create_dir_all(&home).unwrap();

    let mut first = install_command(&home);
    first
        .env(CLAUDE_OVERRIDE, &first_claude)
        .env(CODEX_OVERRIDE, &first_codex);
    run(first);

    let mut second = install_command(&home);
    second
        .env(CLAUDE_OVERRIDE, &second_claude)
        .env(CODEX_OVERRIDE, &second_codex);
    run(second);

    assert_core_trees(&second_claude, &second_codex);
    assert_absent(&home.join(".claude"));
    assert_absent(&home.join(".codex"));
}
