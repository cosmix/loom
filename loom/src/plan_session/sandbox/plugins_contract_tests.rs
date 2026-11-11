//! Contract: every planner session's settings disable the `loom-ui` plugin at the top level,
//! because the planner shows background work itself.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::{build_settings, ensure_runtime_grants, SandboxInputs};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::planning::paths::LockedSet;
use crate::planning::runtime::runtime_paths_under;

/// The settings `build_settings` writes for `kind`, over a project laid out as the sandbox
/// tests lay theirs out.
fn settings_for(kind: &SessionKind) -> Value {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let root = base.join("repo");
    for dir in [
        ".git",
        "loom",
        ".claude",
        ".loom/work",
        "doc/plans/briefs/a",
    ] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in [
        "README.md",
        "doc/plans/PLAN-a.md",
        "doc/plans/briefs/a/y.md",
    ] {
        fs::write(root.join(file), "# Fixture\n").unwrap();
    }
    let home = base.join("home");
    fs::create_dir_all(home.join(".loom/plan")).unwrap();
    let runtime = runtime_paths_under(&base.join("loomhome"), &root).unwrap();
    ensure_runtime_grants(&runtime).unwrap();
    let session = SessionDir::new(runtime.sessions.join("session-a"));
    fs::create_dir_all(session.done()).unwrap();
    let inputs = SandboxInputs {
        project_root: &root,
        runtime: &runtime,
        session_dir: &session,
        kind,
        loom_bin: Path::new("/usr/bin/loom"),
        home: &home,
    };
    build_settings(&inputs, &LockedSet::scan(&root).unwrap(), &[]).unwrap()
}

#[test]
fn every_planner_session_disables_loom_ui() {
    let kinds = [
        SessionKind::Authoring,
        SessionKind::Summary,
        SessionKind::Pressure {
            plan: PathBuf::from("doc/plans/PLAN-a.md"),
        },
    ];
    for kind in kinds {
        let settings = settings_for(&kind);
        assert_eq!(
            settings.get("enabledPlugins"),
            Some(&json!({"loom-ui@skills-dir": false})),
            "{} settings lack the top-level loom-ui entry: {settings:#}",
            kind.label()
        );
        let permissions = &settings["permissions"];
        assert!(
            permissions.get("enabledPlugins").is_none(),
            "{} settings nest enabledPlugins under permissions",
            kind.label()
        );
    }
}
