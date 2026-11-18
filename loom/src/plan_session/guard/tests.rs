//! Write-table tests of the planning PreToolUse guard.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};
use tempfile::TempDir;

use super::run_pre_tool_use;
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::sandbox::{write_session_sandbox, SandboxInputs};
use crate::planning::runtime::{runtime_paths_under, RuntimePaths};

// Fields are `pub(super)` so the sibling `tests_cases` module can read them.
pub(super) struct Fixture {
    pub(super) root: PathBuf,
    pub(super) home: PathBuf,
    pub(super) runtime: RuntimePaths,
    pub(super) session: SessionDir,
    _temp: TempDir,
}

/// Create the project directories and plan/bundle files every row of the table relies on.
fn seed_project_tree(root: &Path) -> Result<()> {
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
        "doc/plans/DONE-PLAN-d.md",
        "doc/plans/RETIRED-PLAN-e.md",
    ] {
        fs::write(root.join(file), "# Fixture\n")?;
    }
    Ok(())
}

impl Fixture {
    pub(super) fn new(kind: SessionKind) -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let base = temp.path().canonicalize()?;
        let root = base.join("project");
        seed_project_tree(&root)?;
        let home = base.join("home");
        fs::create_dir(&home)?;
        let runtime = runtime_paths_under(&base.join("loomhome"), &root)?;
        let session = SessionDir::new(runtime.sessions.join("authoring"));
        fs::create_dir_all(session.path())?;
        let f = Self {
            root,
            home,
            runtime,
            session,
            _temp: temp,
        };
        write_session_sandbox(
            &SandboxInputs {
                project_root: &f.root,
                runtime: &f.runtime,
                session_dir: &f.session,
                kind: &kind,
                loom_bin: Path::new("/opt/loom"),
                home: &f.home,
            },
            &[],
        )?;
        Ok(f)
    }

    pub(super) fn run(&self, bytes: &[u8]) -> Option<String> {
        let output = run_pre_tool_use(self.session.path(), bytes);
        if let Some(line) = &output {
            assert!(!line.contains("\"allow\""), "{line}");
        }
        output
    }

    pub(super) fn check(&self, payload: Value, denied: bool) {
        let output = self.run(&serde_json::to_vec(&payload).unwrap());
        if denied {
            assert_deny(output);
        } else {
            assert_eq!(output, None, "{payload}");
        }
    }

    pub(super) fn write(&self, path: &Path, denied: bool) {
        self.check(payload("Write", path), denied);
    }
}

pub(super) fn pressure() -> SessionKind {
    SessionKind::Pressure {
        plan: "doc/plans/PLAN-a.md".into(),
    }
}

pub(super) fn payload(tool: &str, path: &Path) -> Value {
    let field = if tool == "NotebookEdit" {
        "notebook_path"
    } else {
        "file_path"
    };
    let mut input = json!({});
    input[field] = json!(path);
    json!({"hook_event_name": "PreToolUse", "tool_name": tool, "tool_input": input})
}

pub(super) fn assert_deny(output: Option<String>) -> Value {
    let line = output.expect("expected a deny, got silence");
    assert!(!line.contains('\n'), "{line}");
    let value: Value = serde_json::from_str(&line).expect("deny JSON");
    assert_eq!(value["hookSpecificOutput"]["permissionDecision"], "deny");
    assert!(
        value["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .starts_with("loom plan: "),
        "{line}"
    );
    value
}

// Each row expands into an independently reported test for each session kind.
macro_rules! write_row {
    ($name:ident, $($base:ident).+, $path:literal, $a:expr, $s:expr, $p:expr) => {
        mod $name {
            use super::*;
            #[test]
            fn authoring() -> Result<()> {
                let f = Fixture::new(SessionKind::Authoring)?;
                f.write(&f.$($base).+.join($path), $a);
                Ok(())
            }
            #[test]
            fn summary() -> Result<()> {
                let f = Fixture::new(SessionKind::Summary)?;
                f.write(&f.$($base).+.join($path), $s);
                Ok(())
            }
            #[test]
            fn pressure() -> Result<()> {
                let f = Fixture::new(super::pressure())?;
                f.write(&f.$($base).+.join($path), $p);
                Ok(())
            }
        }
    };
}

write_row!(product, root, "doc/product/x.md", false, true, true);
write_row!(notes, root, "doc/notes/x.md", false, true, true);
write_row!(doc_top_level, root, "doc/x.md", false, true, true);
write_row!(
    selected_plan,
    root,
    "doc/plans/PLAN-a.md",
    false,
    true,
    false
);
write_row!(
    selected_bundle,
    root,
    "doc/plans/briefs/a/y.md",
    false,
    true,
    false
);
write_row!(
    new_bundle_file,
    root,
    "doc/plans/briefs/a/new.md",
    false,
    true,
    false
);
write_row!(
    other_plan,
    root,
    "doc/plans/PLAN-other.md",
    false,
    true,
    true
);
write_row!(
    running_plan,
    root,
    "doc/plans/IN_PROGRESS-PLAN-b.md",
    true,
    true,
    true
);
write_row!(
    done_plan,
    root,
    "doc/plans/DONE-PLAN-d.md",
    true,
    true,
    true
);
write_row!(
    retired_plan,
    root,
    "doc/plans/RETIRED-PLAN-e.md",
    true,
    true,
    true
);
write_row!(
    archive,
    root,
    "doc/plans/archive/DONE-PLAN-c.md",
    true,
    true,
    true
);
write_row!(
    run_review,
    root,
    "doc/plans/REVIEW-PLAN-b.md",
    true,
    true,
    true
);
write_row!(
    locked_bundle,
    root,
    "doc/plans/briefs/b/x.md",
    true,
    true,
    true
);
write_row!(
    new_locked_bundle,
    root,
    "doc/plans/briefs/b/new.md",
    true,
    true,
    true
);
write_row!(knowledge, root, "doc/loom/knowledge/x.md", true, true, true);
write_row!(summaries, root, "doc/loom/summaries/x.md", true, true, true);
write_row!(source, root, "loom/src/x.rs", true, true, true);
write_row!(git_config, root, ".git/config", true, true, true);
write_row!(
    claude_settings,
    root,
    ".claude/settings.json",
    true,
    true,
    true
);
write_row!(state, root, ".loom/work/x", true, true, true);
write_row!(readme, root, "README.md", true, true, true);
write_row!(omissions, root, "planning-omissions.md", true, true, false);
write_row!(runtime_lock, runtime.dir, "lock", true, true, true);
write_row!(
    other_runtime,
    runtime.planning_root,
    "other/lock",
    true,
    true,
    true
);
write_row!(
    answers,
    runtime.dir,
    "sessions/s/answers/a.json",
    true,
    true,
    true
);
write_row!(claims, runtime.dir, "claims/x.json", true, true, true);
write_row!(
    summaries_lock,
    runtime.dir,
    "summaries.lock",
    true,
    true,
    true
);
write_row!(drafts, runtime.dir, "drafts/x.md", false, false, true);
write_row!(
    marker,
    runtime.dir,
    "sessions/authoring/done/complete",
    true,
    true,
    true
);
write_row!(home, home, "x.md", true, true, true);
