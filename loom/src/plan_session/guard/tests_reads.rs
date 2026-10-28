//! Read-deny tests of the planning PreToolUse guard.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};
use tempfile::TempDir;

use super::run_pre_tool_use;
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::sandbox::{write_session_sandbox, SandboxInputs};
use crate::planning::runtime::{runtime_paths_under, RuntimePaths};

const CREDENTIALS: &[&str] = &[
    ".ssh/id_rsa",
    ".aws/credentials",
    ".config/gcloud/credentials.db",
    ".gnupg/private.key",
    ".claude/.credentials.json",
    ".config/gh/hosts.yml",
    ".netrc",
    ".npmrc",
    ".git-credentials",
    ".docker/config.json",
    ".kube/config",
    ".pypirc",
    ".cargo/credentials.toml",
    ".cargo/credentials",
    ".codex/auth.json",
];

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    runtime: RuntimePaths,
    session: SessionDir,
    temp: TempDir,
}

impl Fixture {
    fn new(kind: SessionKind) -> Result<Self> {
        Self::with_home(kind, "home/.loom")
    }

    fn with_home(kind: SessionKind, loom_home: &str) -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let base = temp.path().canonicalize()?;
        let root = base.join("real/proj");
        let home = base.join("home");
        seed_tree(&root, &home)?;
        let runtime = runtime_paths_under(&base.join(loom_home), &root)?;
        let session = SessionDir::new(runtime.sessions.join(kind.label()));
        fs::create_dir_all(session.path())?;
        write_session_sandbox(
            &SandboxInputs {
                project_root: &root,
                runtime: &runtime,
                session_dir: &session,
                kind: &kind,
                loom_bin: Path::new("/opt/loom"),
                home: &home,
            },
            &[],
        )?;
        Ok(Self {
            root,
            home,
            runtime,
            session,
            temp,
        })
    }

    fn run(&self, tool: &str, input: Value, cwd: &Path) -> Option<String> {
        let payload = json!({
            "hook_event_name": "PreToolUse", "tool_name": tool,
            "tool_input": input, "cwd": cwd,
        });
        let bytes = serde_json::to_vec(&payload).unwrap();
        let answer = run_pre_tool_use(self.session.path(), bytes.as_slice());
        if let Some(output) = &answer {
            assert!(!output.contains("\"allow\""), "{output}");
        }
        answer
    }

    fn read(&self, path: impl AsRef<Path>) -> Option<String> {
        self.run("Read", json!({"file_path": path.as_ref()}), &self.root)
    }

    fn search(&self, tool: &str, path: &Path) -> Option<String> {
        self.run(tool, json!({"path": path, "pattern": "*"}), &self.root)
    }

    fn aliased_root(&self) -> Result<PathBuf> {
        let base = self.temp.path().canonicalize()?;
        symlink(base.join("real"), base.join("link"))?;
        Ok(base.join("link/proj"))
    }
}

fn put(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, "# Fixture\n")?;
    Ok(())
}

fn seed_tree(root: &Path, home: &Path) -> Result<()> {
    for dir in [
        ".git",
        "loom",
        ".claude",
        ".loom/work",
        "doc/product",
        "doc/notes",
        "doc/plans/archive",
        "doc/plans/briefs/a",
        "doc/plans/briefs/b",
        "doc/loom/knowledge",
        "doc/loom/summaries",
    ] {
        fs::create_dir_all(root.join(dir))?;
    }
    for file in [
        "README.md",
        "doc/plans/PLAN-a.md",
        "doc/plans/IN_PROGRESS-PLAN-b.md",
        "doc/plans/REVIEW-PLAN-b.md",
        "doc/plans/archive/DONE-PLAN-c.md",
        "doc/plans/briefs/b/x.md",
        "doc/plans/briefs/a/y.md",
        "loom/src/lib.rs",
        ".loom/work/admin.token",
        ".loom/work/user.token",
        ".loom/work/completion-attestation.key",
        ".loom/work/terminal-history/log",
    ] {
        put(root.join(file))?;
    }
    for credential in CREDENTIALS {
        put(home.join(credential))?;
    }
    Ok(())
}

fn kinds() -> [SessionKind; 3] {
    [
        SessionKind::Authoring,
        SessionKind::Summary,
        SessionKind::Pressure {
            plan: PathBuf::from("doc/plans/PLAN-a.md"),
        },
    ]
}

fn assert_denied(answer: Option<String>) {
    let output = answer.expect("expected a deny, got silent Defer");
    let value: Value = serde_json::from_str(&output).expect("deny must be JSON");
    let reason = value["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .expect("deny must have a string reason");
    assert!(reason.starts_with("loom plan: "), "{output}");
    assert_eq!(
        value,
        json!({"hookSpecificOutput": {
            "hookEventName": "PreToolUse", "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }})
    );
}

#[test]
fn credentials_are_read_denied_for_every_kind() -> Result<()> {
    for kind in kinds() {
        let f = Fixture::new(kind)?;
        for credential in CREDENTIALS {
            assert_denied(f.read(f.home.join(credential)));
        }
    }
    Ok(())
}

#[test]
fn literal_tilde_credential_is_denied() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    assert_denied(f.read("~/.ssh/id_rsa"));
    Ok(())
}

#[test]
fn runtime_locks_in_any_project_are_denied() -> Result<()> {
    for kind in kinds() {
        let f = Fixture::new(kind)?;
        put(&f.runtime.lock)?;
        let other = f.home.join(".loom/plan/other-project/lock");
        put(&other)?;
        assert_denied(f.read(&f.runtime.lock));
        assert_denied(f.read(other));
    }
    Ok(())
}

#[test]
fn project_tokens_and_terminal_history_are_denied() -> Result<()> {
    for kind in kinds() {
        let f = Fixture::new(kind)?;
        for secret in [
            "admin.token",
            "user.token",
            "completion-attestation.key",
            "terminal-history/log",
        ] {
            assert_denied(f.read(f.root.join(".loom/work").join(secret)));
        }
    }
    Ok(())
}

#[test]
fn runtime_read_grants_defer_for_every_kind() -> Result<()> {
    for kind in kinds() {
        let f = Fixture::new(kind)?;
        for path in [
            f.runtime.drafts.join("x.md"),
            f.runtime.claims.join("x.json"),
            f.runtime.summaries_lock.clone(),
        ] {
            put(&path)?;
            assert_eq!(f.read(path), None);
        }
    }
    Ok(())
}

#[test]
fn source_read_defers_for_every_kind() -> Result<()> {
    for kind in kinds() {
        let f = Fixture::new(kind)?;
        assert_eq!(f.read(f.root.join("loom/src/lib.rs")), None);
    }
    Ok(())
}

#[test]
fn home_search_is_denied_for_every_kind() -> Result<()> {
    for kind in kinds() {
        let f = Fixture::new(kind)?;
        for tool in ["Grep", "Glob"] {
            assert_denied(f.search(tool, &f.home));
        }
    }
    Ok(())
}

#[test]
fn project_search_defers_despite_token_descendants() -> Result<()> {
    for kind in kinds() {
        let f = Fixture::new(kind)?;
        for tool in ["Grep", "Glob"] {
            assert_eq!(f.search(tool, &f.root), None);
        }
    }
    Ok(())
}

#[test]
fn absent_search_path_uses_payload_cwd() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    for tool in ["Grep", "Glob"] {
        assert_denied(f.run(tool, json!({"pattern": "*"}), &f.home));
        assert_eq!(f.run(tool, json!({"pattern": "*"}), &f.root), None);
    }
    Ok(())
}

#[test]
fn glob_patterns_cannot_escape_the_checked_root() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    for pattern in ["/x/*", "~/x", "../*"] {
        assert_denied(f.run("Glob", json!({"path": f.root, "pattern": pattern}), &f.root));
    }
    Ok(())
}

#[test]
fn pseudo_paths_are_denied_to_all_read_tools() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    for path in ["/proc/self/cwd/.loom/work/admin.token", "/dev/fd/0"] {
        assert_denied(f.read(path));
        for tool in ["Grep", "Glob"] {
            assert_denied(f.search(tool, Path::new(path)));
        }
    }
    Ok(())
}

#[test]
fn doc_symlink_cannot_read_or_search_credentials() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    let notes = f.root.join("doc/notes");
    fs::remove_dir(&notes)?;
    symlink(f.home.join(".ssh"), &notes)?;
    assert_denied(f.read(notes.join("id_rsa")));
    for tool in ["Grep", "Glob"] {
        assert_denied(f.search(tool, &notes));
    }
    Ok(())
}

#[test]
fn symlinked_project_ancestor_cannot_read_tokens() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    assert_denied(f.read(f.aliased_root()?.join(".loom/work/user.token")));
    Ok(())
}

#[test]
fn symlinked_project_ancestor_can_read_source() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    assert_eq!(f.read(f.aliased_root()?.join("loom/src/lib.rs")), None);
    Ok(())
}

#[test]
fn redirected_runtime_denies_locks_and_searches_above_planning_root() -> Result<()> {
    let f = Fixture::with_home(SessionKind::Authoring, "other-loomhome")?;
    put(&f.runtime.lock)?;
    assert_denied(f.read(&f.runtime.lock));
    assert_denied(f.read(f.home.join(".loom/plan/other/lock")));
    assert_denied(f.search("Grep", f.runtime.planning_root.parent().unwrap()));
    assert_eq!(f.search("Grep", &f.root), None);
    Ok(())
}

#[test]
fn draft_symlink_does_not_grant_access_to_credentials() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    let draft = f.runtime.drafts.join("x.md");
    symlink(f.home.join(".ssh/id_rsa"), &draft)?;
    assert_denied(f.read(draft));
    Ok(())
}

#[test]
fn grant_directory_symlink_does_not_carry_carve_out_to_credentials() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    fs::remove_dir(&f.runtime.drafts)?;
    symlink(f.home.join(".ssh"), &f.runtime.drafts)?;
    assert_denied(f.read(f.runtime.drafts.join("id_rsa")));
    assert_denied(f.read(f.home.join(".ssh/id_rsa")));
    Ok(())
}

#[test]
fn lexical_credential_deny_survives_symlink_to_source() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    let key = f.home.join(".ssh/id_rsa");
    fs::remove_file(&key)?;
    symlink(f.root.join("loom/src/lib.rs"), &key)?;
    assert_denied(f.read(key));
    Ok(())
}

#[test]
fn benign_symlink_search_root_is_denied() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    let link = f.root.join("doc/source");
    symlink(f.root.join("loom/src"), &link)?;
    for tool in ["Grep", "Glob"] {
        assert_denied(f.search(tool, &link));
    }
    assert_eq!(f.read(link.join("lib.rs")), None);
    Ok(())
}

#[test]
fn dangling_link_is_read_denied() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    let link = f.root.join("doc/missing");
    symlink(f.root.join("does-not-exist"), &link)?;
    assert_denied(f.read(link));
    Ok(())
}
