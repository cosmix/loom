//! Settings snapshot, file and grant tests of the planning sandbox builder.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};
use tempfile::TempDir;

use super::tests_kinds_fixture::{
    expected_common_for, expected_filesystem, expected_permissions, kinds, scope_cases,
};
use super::{build_settings, ensure_runtime_grants, write_session_sandbox, SandboxInputs};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::scope;
use crate::planning::paths::LockedSet;
use crate::planning::runtime::{runtime_paths_under, RuntimePaths};

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    runtime: RuntimePaths,
    session: SessionDir,
    _temp: TempDir,
}

impl Fixture {
    fn new() -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let base = temp.path().canonicalize()?;
        let root = base.join("project");
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
            "doc/index.md",
            "doc/plans/PLAN-a.md",
            "doc/plans/IN_PROGRESS-PLAN-b.md",
            "doc/plans/REVIEW-PLAN-b.md",
            "doc/plans/archive/DONE-PLAN-c.md",
            "doc/plans/briefs/b/x.md",
            "doc/plans/briefs/a/y.md",
        ] {
            fs::write(root.join(file), "# Fixture\n")?;
        }
        let home = base.join("home");
        fs::create_dir(&home)?;
        let runtime = runtime_paths_under(&base.join("loomhome"), &root)?;
        let session = SessionDir::new(runtime.sessions.join("authoring"));
        fs::create_dir_all(session.path())?;
        Ok(Self {
            root,
            home,
            runtime,
            session,
            _temp: temp,
        })
    }

    fn inputs(&self) -> SandboxInputs<'_> {
        SandboxInputs {
            project_root: &self.root,
            runtime: &self.runtime,
            session_dir: &self.session,
            kind: &SessionKind::Authoring,
            loom_bin: Path::new("/opt/loom"),
            home: &self.home,
        }
    }

    fn settings(&self) -> Result<Value> {
        build_settings(&self.inputs(), &LockedSet::scan(&self.root)?, &[])
    }
}

#[test]
fn authoring_settings_match_whole_json_snapshot() -> Result<()> {
    let f = Fixture::new()?;
    ensure_runtime_grants(&f.runtime)?;
    let expected = json!({
        "sandbox": {
            "enabled": true, "failIfUnavailable": true,
            "autoAllowBashIfSandboxed": false, "allowUnsandboxedCommands": false,
            "network": {"strictAllowlist": true},
            "filesystem": expected_filesystem(&f.root, &f.home, &f.runtime),
        },
        "permissions": expected_permissions(&f.root, &f.runtime),
        "worktree": {"bgIsolation": "none"},
        "hooks": {"PreToolUse": [{
            "matcher": "Read|Edit|Write|MultiEdit|NotebookEdit|Grep|Glob",
            "hooks": [{
                "type": "command",
                "command": format!(
                    "/opt/loom plan hook PreToolUse --session-dir {}", f.session.path().display()
                ),
                "timeout": 10,
            }],
        }]},
    });

    assert_eq!(f.settings()?, expected);
    Ok(())
}

#[test]
fn permission_mode_matches_each_session_kind() -> Result<()> {
    let f = super::tests_kinds::Fixture::new();
    for kind in kinds() {
        let settings = f.build(&kind, &[])?;
        let expected = expected_common_for(&f, &kind);
        assert_eq!(
            settings["permissions"]["defaultMode"],
            expected["permissions"]["defaultMode"]
        );
        super::assert::assert_planning_settings_for(&settings, &kind)?;
    }
    Ok(())
}

#[test]
fn authoring_ask_rules_match_each_scope() -> Result<()> {
    let f = Fixture::new()?;
    for (session_scope, expected_ask) in scope_cases()? {
        scope::write(&f.session, &session_scope)?;
        let settings = f.settings()?;
        assert_eq!(settings["permissions"]["defaultMode"], json!("auto"));
        assert_eq!(
            settings["permissions"]["ask"], expected_ask,
            "{session_scope:?}"
        );
        super::assert::assert_planning_settings_for(&settings, &SessionKind::Authoring)?;
    }
    Ok(())
}

#[test]
fn runtime_grants_create_private_directories_and_empty_lock() -> Result<()> {
    let f = Fixture::new()?;

    ensure_runtime_grants(&f.runtime)?;

    for dir in [&f.runtime.drafts, &f.runtime.claims] {
        let metadata = fs::symlink_metadata(dir)?;
        assert!(metadata.is_dir());
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
    }
    let metadata = fs::symlink_metadata(&f.runtime.summaries_lock)?;
    assert!(metadata.is_file());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    assert_eq!(fs::read(&f.runtime.summaries_lock)?, b"");
    Ok(())
}

#[test]
fn runtime_grants_twice_preserve_existing_lock_bytes() -> Result<()> {
    let f = Fixture::new()?;
    let contents = b"existing lock\0\xff\n";
    fs::write(&f.runtime.summaries_lock, contents)?;

    ensure_runtime_grants(&f.runtime)?;
    assert_eq!(fs::read(&f.runtime.summaries_lock)?, contents);
    ensure_runtime_grants(&f.runtime)?;

    assert_eq!(fs::read(&f.runtime.summaries_lock)?, contents);
    Ok(())
}

#[test]
fn sandbox_files_are_private_json_at_returned_absolute_paths() -> Result<()> {
    let f = Fixture::new()?;

    let files = write_session_sandbox(&f.inputs(), &[])?;

    assert_eq!(files.settings, f.session.settings());
    assert_eq!(files.guard, f.session.guard());
    for path in [&files.settings, &files.guard] {
        assert!(path.is_absolute());
        let metadata = fs::symlink_metadata(path)?;
        assert!(metadata.is_file());
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }
    let settings: Value = serde_json::from_slice(&fs::read(files.settings)?)?;
    assert_eq!(settings, f.settings()?);
    let guard: Value = serde_json::from_slice(&fs::read(files.guard)?)?;
    assert_eq!(
        guard,
        json!({
            "schema": 1, "kind": {"kind": "authoring"}, "project_root": f.root,
            "runtime_dir": f.runtime.dir, "session_dir": f.session.path(), "home": f.home,
        })
    );
    Ok(())
}

#[test]
fn sandbox_writer_refuses_planted_settings_symlink() -> Result<()> {
    let f = Fixture::new()?;
    let target = f.home.join("settings-target.json");
    let contents = b"do not overwrite\n";
    fs::write(&target, contents)?;
    symlink(&target, f.session.settings())?;

    let error = write_session_sandbox(&f.inputs(), &[]).unwrap_err();

    assert!(format!("{error:#}").contains("settings.json"));
    assert!(fs::symlink_metadata(f.session.settings())?
        .file_type()
        .is_symlink());
    assert_eq!(fs::read(target)?, contents);
    Ok(())
}

#[test]
fn top_level_symlink_is_denied_by_its_own_path() -> Result<()> {
    let f = Fixture::new()?;
    let link = f.root.join("linked-home");
    symlink(&f.home, &link)?;

    let settings = f.settings()?;

    let deny = settings["sandbox"]["filesystem"]["denyWrite"]
        .as_array()
        .unwrap();
    assert!(deny.contains(&json!(link)));
    assert!(!deny.contains(&json!(f.home)));
    Ok(())
}

#[test]
fn archived_plan_without_a_bundle_gets_a_bundle_deny_and_unlocked_plans_none() -> Result<()> {
    let f = Fixture::new()?;
    let plans = f.root.join("doc/plans");
    for file in ["archive/PLAN-d.md", "archive/notes.md", "PLAN-e.md"] {
        fs::write(plans.join(file), "# Fixture\n")?;
    }

    let settings = f.settings()?;

    let deny = settings["permissions"]["deny"].as_array().unwrap();
    let bundle = |slug: &str| json!(format!("Edit(/{}/briefs/{slug}/**)", plans.display()));
    assert!(
        deny.contains(&bundle("d")),
        "archived plan d keeps its missing bundle denied"
    );
    for unlocked in ["a", "e", "notes"] {
        assert!(
            !deny.contains(&bundle(unlocked)),
            "{unlocked} is not a locked plan"
        );
    }
    Ok(())
}

#[test]
fn rule_syntax_in_a_rule_path_is_refused_and_a_plain_root_builds() -> Result<()> {
    let f = Fixture::new()?;
    let locked = LockedSet::scan(&f.root)?;
    let odd_root = PathBuf::from("/home/u/src/app[2]");
    let odd_runtime = f.runtime.dir.join("rt(1)");
    let odd_session = SessionDir::new(f.session.path().join("s*"));
    let refused = [
        (
            "project root",
            SandboxInputs {
                project_root: &odd_root,
                ..f.inputs()
            },
        ),
        (
            "runtime directory",
            SandboxInputs {
                runtime: &RuntimePaths {
                    dir: odd_runtime,
                    ..f.runtime.clone()
                },
                ..f.inputs()
            },
        ),
        (
            "session directory",
            SandboxInputs {
                session_dir: &odd_session,
                ..f.inputs()
            },
        ),
    ];

    for (what, inputs) in refused {
        let error = build_settings(&inputs, &locked, &[]).unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains(what) && message.contains("syntax"),
            "{message}"
        );
    }
    assert!(build_settings(&f.inputs(), &locked, &[]).is_ok());
    Ok(())
}

#[test]
fn authoring_write_denies_are_existing_paths_without_nested_mounts() -> Result<()> {
    let f = Fixture::new()?;
    let settings = f.settings()?;
    let deny = settings["sandbox"]["filesystem"]["denyWrite"]
        .as_array()
        .unwrap();

    for (index, entry) in deny.iter().enumerate() {
        let path = Path::new(entry.as_str().unwrap());
        assert!(path.is_absolute());
        fs::symlink_metadata(path)?;
        for (other_index, other) in deny.iter().enumerate() {
            if index != other_index {
                let ancestor = Path::new(other.as_str().unwrap());
                assert!(
                    !path.starts_with(ancestor),
                    "{} under {}",
                    path.display(),
                    ancestor.display()
                );
            }
        }
    }
    Ok(())
}
