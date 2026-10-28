//! Fixture, settings and guard helpers shared by the plan_session sandbox contracts.
//!
//! Every tree lives in a `tempfile::Builder::new().prefix("loom-plan-session-")` directory under
//! the system temp dir, holding the project root, a fake home and a fake loom home.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use loom::plan_session::guard::run_pre_tool_use;
use loom::plan_session::kind::{SessionDir, SessionKind};
use loom::plan_session::sandbox::{
    build_settings, write_session_sandbox, SandboxFiles, SandboxInputs,
};
use loom::planning::paths::LockedSet;
use loom::planning::runtime::{runtime_paths_under, RuntimePaths};
use serde_json::{json, Value};
use tempfile::TempDir;

/// The built loom binary; spawned only through [`loom_cmd`], which scrubs every `LOOM_*` variable.
const LOOM_BIN: &str = env!("CARGO_BIN_EXE_loom");

/// One project tree with its runtime directory and a session directory `s1` under it.
pub struct Fixture {
    _tmp: TempDir,
    /// The canonical temp directory holding root, home and loom home.
    pub base: PathBuf,
    pub root: PathBuf,
    pub home: PathBuf,
    pub runtime: RuntimePaths,
    pub session: SessionDir,
    loom_bin: PathBuf,
}

impl Fixture {
    /// Root `<base>/proj`, loom home `<home>/.loom`.
    pub fn new() -> Self {
        Self::build("proj", "home/.loom")
    }

    /// Root `<base>/proj`, loom home `<base>/<rel>`.
    pub fn with_loom_home(rel: &str) -> Self {
        Self::build("proj", rel)
    }

    /// Root `<base>/<rel>`, loom home `<home>/.loom`.
    pub fn with_root(rel: &str) -> Self {
        Self::build(rel, "home/.loom")
    }

    fn build(root_rel: &str, loom_home_rel: &str) -> Self {
        let tmp = tempfile::Builder::new()
            .prefix("loom-plan-session-")
            .tempdir_in(std::env::temp_dir())
            .unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let (root, home) = (base.join(root_rel), base.join("home"));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(home.join(".loom/plan")).unwrap();
        let runtime = runtime_paths_under(&base.join(loom_home_rel), &root).unwrap();
        let session = SessionDir::new(runtime.sessions.join("s1"));
        fs::create_dir_all(session.path()).unwrap();
        fs::set_permissions(session.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let fixture = Fixture {
            _tmp: tmp,
            base,
            root,
            home,
            runtime,
            session,
            loom_bin: PathBuf::from(LOOM_BIN),
        };
        fixture.populate();
        fixture
    }

    fn populate(&self) {
        for dir in [".claude", "loom/src", "doc/product", "doc/plans"] {
            fs::create_dir_all(self.root.join(dir)).unwrap();
        }
        let files = [
            ".git/HEAD",
            "README.md",
            ".hidden",
            "loom/src/lib.rs",
            ".loom/work/user.token",
            ".loom/work/admin.token",
            "doc/product/spec.md",
            "doc/loom/knowledge/INDEX.md",
            "doc/loom/summaries/s.md",
        ];
        for rel in files {
            self.write(rel, "x\n");
        }
        let rt = &self.runtime;
        for path in [&rt.lock, &rt.summaries_lock, &rt.drafts.join("x.md")] {
            write_abs(path, "x\n");
        }
        write_abs(&rt.claims.join("c.json"), "{}\n");
        write_abs(&self.session.path().join("answers/a.json"), "{}\n");
        write_abs(&self.home.join(".ssh/id_rsa"), "secret\n");
    }

    /// Create `rel` under the project root with `body`, making its parents.
    pub fn write(&self, rel: &str, body: &str) {
        write_abs(&self.root.join(rel), body);
    }

    /// `rel` under the project root.
    pub fn at(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    pub fn inputs<'a>(&'a self, kind: &'a SessionKind) -> SandboxInputs<'a> {
        SandboxInputs {
            project_root: &self.root,
            runtime: &self.runtime,
            session_dir: &self.session,
            kind,
            loom_bin: &self.loom_bin,
            home: &self.home,
        }
    }

    /// `build_settings` over a fresh scan of the tree, without extra hooks.
    pub fn settings(&self, kind: &SessionKind) -> Value {
        let locked = LockedSet::scan(&self.root).unwrap();
        build_settings(&self.inputs(kind), &locked, &[]).unwrap()
    }

    /// `write_session_sandbox`: settings.json and guard.json into the session directory.
    pub fn sandbox(&self, kind: &SessionKind) -> SandboxFiles {
        write_session_sandbox(&self.inputs(kind), &[]).unwrap()
    }

    /// Run the guard for this fixture's session on one tool call, `cwd` the project root.
    pub fn guard(&self, tool: &str, input: Value) -> Option<String> {
        let body = payload(tool, input, &self.root);
        run_pre_tool_use(self.session.path(), body.as_slice())
    }
}

fn write_abs(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

/// A symlink at `link` pointing at `target`, making the parents of `link`.
pub fn link(target: &Path, link: &Path) {
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    symlink(target, link).unwrap();
}

pub fn payload(tool: &str, input: Value, cwd: &Path) -> Vec<u8> {
    let body = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": tool,
        "tool_input": input,
        "cwd": cwd,
        "session_id": "s1",
    });
    serde_json::to_vec(&body).unwrap()
}

/// A tool input naming `path` the way Edit, Write and Read do.
pub fn file_input(path: &Path) -> Value {
    json!({ "file_path": path, "content": "x", "old_string": "x", "new_string": "y" })
}

/// The guard's answer must be exactly one JSON deny line.
pub fn assert_denied(out: &Option<String>, what: &str) {
    let line = out
        .as_deref()
        .unwrap_or_else(|| panic!("{what}: the guard printed nothing, expected a deny"));
    assert_eq!(
        line.trim().lines().count(),
        1,
        "{what}: not one line: {line}"
    );
    let answer: Value = serde_json::from_str(line).expect("deny line is JSON");
    let output = &answer["hookSpecificOutput"];
    assert_eq!(output["hookEventName"], "PreToolUse", "{what}: {line}");
    assert_eq!(output["permissionDecision"], "deny", "{what}: {line}");
}

/// The strings at `pointer` in `settings`; a missing or non-array value is an empty list.
pub fn list(settings: &Value, pointer: &str) -> Vec<String> {
    let items = settings.pointer(pointer).and_then(Value::as_array);
    let strings = items.into_iter().flatten().filter_map(Value::as_str);
    strings.map(str::to_owned).collect()
}

pub fn contains(list: &[String], item: impl AsRef<str>) -> bool {
    list.iter().any(|entry| entry == item.as_ref())
}

pub fn path_str(path: &Path) -> String {
    path.to_str().expect("utf-8 path").to_owned()
}

/// A permission rule `<tool>(/` + the absolute `path` + `suffix` + `)`: `Edit(//abs/...)`.
pub fn rule(tool: &str, path: &Path, suffix: &str) -> String {
    format!("{tool}(/{}{suffix})", path.display())
}

/// Replace (or insert) the value at `pointer`, whose parent object must exist.
pub fn set_pointer(settings: &mut Value, pointer: &str, value: Value) {
    let (parent, key) = pointer.rsplit_once('/').expect("pointer has a key");
    let object = settings.pointer_mut(parent).and_then(Value::as_object_mut);
    object
        .expect("parent object exists")
        .insert(key.to_owned(), value);
}

/// Every way an allow or deny rule breaks the permission-rule syntax contract: a `Write(` rule,
/// a `Read(` deny, a rule path that is not the absolute `//` form under `base`, or an Edit deny
/// on the knowledge or summaries directories (it would also be an OS write deny).
pub fn rule_problems(settings: &Value, base: &Path) -> Vec<String> {
    let absolute = format!("(/{}", base.display());
    let mut bad = Vec::new();
    for section in ["allow", "deny"] {
        for entry in list(settings, &format!("/permissions/{section}")) {
            let path_rule = entry.starts_with("Edit(") || entry.starts_with("Read(");
            let wrong_form = path_rule && !entry[4..].starts_with(&absolute);
            let knowledge =
                entry.contains("doc/loom/knowledge") || entry.contains("doc/loom/summaries");
            if entry.starts_with("Write(")
                || wrong_form
                || (section == "deny" && entry.starts_with("Read("))
                || (section == "deny" && entry.starts_with("Edit(") && knowledge)
            {
                bad.push(format!("{section}: {entry}"));
            }
        }
    }
    bad
}

/// A `loom` command with every `LOOM_*` variable removed and `LOOM_HOME` set to `home`.
fn loom_cmd(home: &Path) -> Command {
    let mut command = Command::new(LOOM_BIN);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(key);
        }
    }
    command.env("LOOM_HOME", home);
    command
}

/// Run `loom plan hook <event> --session-dir <dir>` with `{}` on stdin.
pub fn run_hook(f: &Fixture, event: &str, dir: &Path) -> Output {
    let home = f.base.join("cli-home");
    fs::create_dir_all(&home).unwrap();
    let mut child = loom_cmd(&home)
        .args(["plan", "hook", event, "--session-dir"])
        .arg(dir)
        .current_dir(&f.base)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn loom");
    // An unwired command exits before reading; its failure shows in the output, not here.
    let _ = child.stdin.take().unwrap().write_all(b"{}");
    child.wait_with_output().unwrap()
}
