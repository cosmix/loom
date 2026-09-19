//! Shared fixtures for the skill-trigger hook integration tests: the
//! HookOutput/FakeHome scaffolding and the hook-invocation harness. Split out
//! of hooks_skill_trigger.rs purely for size (CLAUDE.md rule 17's 400-line
//! file cap and 50-line function cap), the same way that file already splits
//! out its --codex and evidence-rule tests.

use loom::fs::permissions::constants::{
    HOOK_READ_DISCIPLINE, HOOK_READ_LEDGER, HOOK_SKILL_TRIGGER,
};
use loom::process::sandbox_probe::skip_unless;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use tempfile::TempDir;

pub struct HookOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub struct FakeHome {
    home: TempDir,
    cwd: TempDir,
    tmp: TempDir,
}

fn write_exec(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("hook path has a parent")).expect("create hook dir");
    fs::write(path, content).expect("write file");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
}

fn write_skill_md(path: &Path, name: &str) {
    fs::create_dir_all(path.parent().expect("SKILL.md path has a parent"))
        .expect("create skill dir");
    fs::write(
        path,
        format!("---\nname: {name}\ndescription: {name} desc.\n---\n"),
    )
    .expect("write SKILL.md");
}

/// Build the fake `~/.claude/hooks/loom/skill-keywords.json` index this
/// fixture's tests match against.
fn write_skill_index(home: &Path) {
    let mut index = json!({
        "react": ["loom-react", "loom-typescript"],
        "typescript": ["loom-typescript"],
        "docker": ["loom-docker"],
        "context": ["loom-golang"],
        "pointer": ["loom-golang"],
        "refactor": ["loom-refactoring"],
        "golang": ["loom-golang"],
        "skill": ["loom-skills"],
        "skills": ["loom-skills"],
        "plan": ["loom-plan-writer"],
        "writer": ["loom-plan-writer"],
        "stage": ["loom-ci-cd"],
        "model selection": ["loom-model-evaluation"],
    });
    let index_obj = index.as_object_mut().expect("index is an object");
    for i in 0..10 {
        index_obj.insert(format!("alpha{i}"), json!([format!("loom-alpha{i}")]));
    }
    let index_path = home.join(".claude/hooks/loom/skill-keywords.json");
    fs::create_dir_all(index_path.parent().expect("index path has a parent"))
        .expect("create index dir");
    fs::write(&index_path, index.to_string()).expect("write index");
}

/// Write the core-skill and catalogued-skill SKILL.md fixtures the index
/// above refers to.
fn write_fixture_skills(home: &Path) {
    let core_dir = home.join(".claude/skills");
    for name in ["loom-skills", "loom-plan-writer"] {
        write_skill_md(&core_dir.join(name).join("SKILL.md"), name);
    }
    let mut catalogued: Vec<String> = vec![
        "loom-react".into(),
        "loom-typescript".into(),
        "loom-docker".into(),
        "loom-golang".into(),
        "loom-refactoring".into(),
        "loom-ci-cd".into(),
        "loom-model-evaluation".into(),
    ];
    catalogued.extend((0..10).map(|i| format!("loom-alpha{i}")));
    for name in &catalogued {
        write_skill_md(
            &home
                .join(".claude/loom-skill-catalog")
                .join(name)
                .join("SKILL.md"),
            name,
        );
    }
}

impl FakeHome {
    pub fn new() -> Self {
        let home = TempDir::new().expect("create fake HOME");
        let cwd = TempDir::new().expect("create fake cwd");
        let tmp = TempDir::new().expect("create fake TMPDIR");
        write_skill_index(home.path());
        write_fixture_skills(home.path());
        FakeHome { home, cwd, tmp }
    }

    fn home_path(&self) -> &Path {
        self.home.path()
    }

    fn cwd_path(&self) -> &Path {
        self.cwd.path()
    }

    fn tmp_path(&self) -> &Path {
        self.tmp.path()
    }

    pub fn add_go_project(&self) {
        fs::write(self.cwd.path().join("go.mod"), "module example.com/test\n")
            .expect("write Go module");
    }

    pub fn add_ci_cd_project(&self) {
        let workflows = self.cwd.path().join(".github/workflows");
        fs::create_dir_all(&workflows).expect("create workflows dir");
        fs::write(workflows.join("ci.yml"), "name: ci\non: [push]\n").expect("write ci workflow");
    }

    /// Add a raw keyword -> skills mapping to the index after construction,
    /// to simulate an index entry an indexer defect could have produced
    /// (this fixture's own index is hand-written, not built by the real
    /// indexer, so a review defect there needs the entry injected here).
    pub fn add_index_entry(&self, key: &str, skills: &[&str]) {
        let index_path = self
            .home
            .path()
            .join(".claude/hooks/loom/skill-keywords.json");
        let raw = fs::read_to_string(&index_path).expect("read index");
        let mut index: Value = serde_json::from_str(&raw).expect("parse index");
        index[key] = json!(skills);
        fs::write(&index_path, index.to_string()).expect("rewrite index");
    }
}

fn python3_available() -> bool {
    static RESULT: OnceLock<bool> = OnceLock::new();
    *RESULT.get_or_init(|| {
        Command::new("python3")
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

pub fn install_hook() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("create hook dir");
    let path = dir.path().join("skill-trigger.sh");
    write_exec(&path, HOOK_SKILL_TRIGGER);
    // The once-per-session ledger sources these siblings at runtime
    // (_loom_ledger_file, _read_discipline.sh:135-154); install them beside
    // the hook so the dedup path under test is real, not a silent no-op.
    write_exec(
        &dir.path().join("_read_discipline.sh"),
        HOOK_READ_DISCIPLINE,
    );
    write_exec(&dir.path().join("_read_ledger.sh"), HOOK_READ_LEDGER);
    (dir, path)
}

/// Build the `python3 <hook>` command with the isolated env this harness
/// needs, without spawning it.
fn build_hook_command(
    hook: &Path,
    home: &FakeHome,
    hash_seed: Option<&str>,
    codex: bool,
) -> Command {
    let mut cmd = Command::new("python3");
    cmd.arg(hook)
        .env("HOME", home.home_path())
        .env("LOOM_BIN", env!("CARGO_BIN_EXE_loom"))
        // Isolate the once-per-session ledger per FakeHome instance instead
        // of the ambient TMPDIR, which every test in this process shares -
        // and strip any real stage identity so the hook can never resolve
        // _loom_ledger_file's in-stage branch and touch the live
        // .loom/work of the session running this test.
        .env("TMPDIR", home.tmp_path())
        .env_remove("LOOM_WORK_DIR")
        .env_remove("LOOM_SESSION_ID")
        .env_remove("LOOM_STAGE_ID")
        .env_remove("LOOM_SKILL_DEBUG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if codex {
        cmd.arg("--codex")
            .env("CODEX_HOME", home.home_path().join(".claude"));
    }
    match hash_seed {
        Some(seed) => {
            cmd.env("PYTHONHASHSEED", seed);
        }
        None => {
            cmd.env_remove("PYTHONHASHSEED");
        }
    }
    cmd
}

pub fn run_hook(
    hook: &Path,
    home: &FakeHome,
    prompt: &str,
    hash_seed: Option<&str>,
    codex: bool,
) -> HookOutput {
    let payload = json!({
        "session_id": "t",
        "cwd": home.cwd_path().display().to_string(),
        "prompt": prompt,
    });
    let mut cmd = build_hook_command(hook, home, hash_seed, codex);
    let mut child = cmd.spawn().expect("spawn python3 skill-trigger.sh");
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(payload.to_string().as_bytes()).ok();
    }
    let output = child.wait_with_output().expect("wait for hook");
    HookOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

pub fn additional_context(stdout: &str) -> String {
    let v: Value = serde_json::from_str(stdout.trim()).expect("parse stdout json");
    v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext present")
        .to_string()
}

pub fn skip_unless_python3(test_name: &str) -> bool {
    skip_unless(
        python3_available(),
        test_name,
        "python3 must be spawnable to run the skill-trigger hook",
    )
}
