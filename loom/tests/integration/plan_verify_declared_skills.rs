//! `loom plan verify` resolves a stage's declared `skills:` against the
//! installed Claude skills dir (custom root, recorded root, or `~/.claude`).

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

use super::helpers::loom_cmd;

const SKILL: &str = "loom-custom-root-skill";
const CLAUDE_OVERRIDE: &str = "LOOM_CLAUDECODE_INSTALL_DIR";

fn plan_declaring(skill: &str) -> String {
    format!(
        r#"# Declared Skills

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-one
      name: "Stage One"
      stage_type: standard
      working_dir: "."
      skills: [{skill}]
      acceptance:
        - "true"
```

<!-- END loom METADATA -->
"#
    )
}

/// Creates `<root>/skills/<SKILL>/SKILL.md` and returns `<root>`.
fn install_skill(base: &Path) -> std::path::PathBuf {
    let root = base.join("custom-claude");
    let skill_dir = root.join("skills").join(SKILL);
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {SKILL}\ndescription: custom root skill\n---\n"),
    )
    .unwrap();
    root
}

/// `plan verify --json` with an isolated HOME and no ambient overrides.
fn verify(home: &Path, plan: &Path, claude_override: Option<&Path>) -> Output {
    let mut command: Command = loom_cmd();
    command
        .env("HOME", home)
        .env_remove(CLAUDE_OVERRIDE)
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .args(["plan", "verify", "--json"])
        .arg(plan);
    if let Some(root) = claude_override {
        command.env(CLAUDE_OVERRIDE, root);
    }
    command.output().expect("run loom plan verify")
}

fn assert_resolved(out: &Output) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!stdout.contains("No skill index"), "stdout: {stdout}");
    assert!(!stdout.contains("unknown skill"), "stdout: {stdout}");
}

#[test]
fn env_override_root_resolves_declared_skill() {
    let temp = TempDir::new().unwrap();
    let root = install_skill(temp.path());
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let plan = temp.path().join("PLAN-skills.md");
    fs::write(&plan, plan_declaring(SKILL)).unwrap();

    assert_resolved(&verify(&home, &plan, Some(&root)));
}

#[test]
fn recorded_root_resolves_declared_skill() {
    let temp = TempDir::new().unwrap();
    let root = install_skill(temp.path());
    let home = temp.path().join("home");
    let record_dir = home.join(".config/loom");
    fs::create_dir_all(&record_dir).unwrap();
    fs::write(
        record_dir.join("install-roots.toml"),
        format!("claude_dir = {:?}\n", root.to_str().unwrap()),
    )
    .unwrap();
    let plan = temp.path().join("PLAN-skills.md");
    fs::write(&plan, plan_declaring(SKILL)).unwrap();

    assert_resolved(&verify(&home, &plan, None));
}

#[test]
fn unknown_skill_under_custom_root_is_a_hard_error() {
    let temp = TempDir::new().unwrap();
    let root = install_skill(temp.path());
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let plan = temp.path().join("PLAN-skills.md");
    fs::write(&plan, plan_declaring("loom-no-such-skill")).unwrap();

    let out = verify(&home, &plan, Some(&root));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "stdout: {stdout}");
    assert!(stdout.contains("loom-no-such-skill"), "stdout: {stdout}");
}
