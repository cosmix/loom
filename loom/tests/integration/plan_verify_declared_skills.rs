//! `loom plan verify` resolves a stage's declared `skills:` against the
//! installed Claude skills dir, `~/.claude/skills`.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

use super::helpers::loom_cmd;

const SKILL: &str = "loom-home-root-skill";

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

/// Creates `<home>/.claude/skills/<SKILL>/SKILL.md`.
fn install_skill(home: &Path) {
    let skill_dir = home.join(".claude/skills").join(SKILL);
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {SKILL}\ndescription: home root skill\n---\n"),
    )
    .unwrap();
}

/// `plan verify --json` with an isolated HOME and no ambient Claude root.
fn verify(home: &Path, plan: &Path) -> Output {
    let mut command: Command = loom_cmd();
    command
        .env("HOME", home)
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .args(["plan", "verify", "--json"])
        .arg(plan);
    command.output().expect("run loom plan verify")
}

#[test]
fn a_skill_under_the_home_root_resolves() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    install_skill(&home);
    let plan = temp.path().join("PLAN-skills.md");
    fs::write(&plan, plan_declaring(SKILL)).unwrap();

    let out = verify(&home, &plan);
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
fn an_unknown_skill_is_a_hard_error() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    install_skill(&home);
    let plan = temp.path().join("PLAN-skills.md");
    fs::write(&plan, plan_declaring("loom-no-such-skill")).unwrap();

    let out = verify(&home, &plan);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "stdout: {stdout}");
    assert!(stdout.contains("loom-no-such-skill"), "stdout: {stdout}");
}
