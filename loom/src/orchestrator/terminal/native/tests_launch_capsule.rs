//! End-to-end launch tests for the settings capsule, the scratch directory,
//! the wrapper's host exports and the spawn-time sandbox preflight, driven
//! through `prepare_session_launch_with` against an injected [`LaunchHost`],
//! so no test touches `PATH`, `HOME`, `LOOM_HOOKS_DIR` or the operator's
//! runtime directory. Split out of `tests_launch.rs` to keep that file under
//! the 400-line ceiling.

use super::host::{hook_path_entries, LaunchHost};
use super::*;
use crate::fs::permissions::install_loom_hooks_to;
use crate::fs::work_dir::write_remote_control_config;
use crate::orchestrator::terminal::native::capsule::CapsuleSupport;
use crate::orchestrator::terminal::native::session_settings_path;
use crate::remote_control::{RemoteControlConfig, RemoteControlMode};
use crate::sandbox::preflight::{HostFacts, SandboxPreflightRefusal};
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

const ALL_KINDS: [SessionType; 5] = [
    SessionType::Stage,
    SessionType::Knowledge,
    SessionType::Merge,
    SessionType::BaseConflict,
    SessionType::Adjudication,
];

fn current_uid() -> u32 {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// A repository with a state root and one stage worktree, and a host whose
/// every fact is fixed and passes the preflight: all capsule flags
/// supported, this build's hooks and the loom binary in the temp dir outside
/// the repository and scratch roots, and a known hook PATH.
struct Fixture {
    temp: TempDir,
    repo: PathBuf,
    work_dir: PathBuf,
    host: LaunchHost,
}

fn fixture() -> Fixture {
    let temp = TempDir::new().unwrap();
    let repo = temp.path().join("repo");
    let work_dir = repo.join(".loom").join("work");
    let hooks_dir = temp.path().join("hooks");
    std::fs::create_dir_all(repo.join(".worktrees").join("stage-1")).unwrap();
    std::fs::create_dir_all(&work_dir).unwrap();
    install_loom_hooks_to(&hooks_dir).unwrap();
    // In a directory of its own: the capsule denies writing dirname(LOOM_BIN).
    let loom_bin = temp.path().join("install").join("loom");
    std::fs::create_dir_all(loom_bin.parent().unwrap()).unwrap();
    std::fs::write(&loom_bin, "").unwrap();
    // Remote Control off, so no launch runs a `claude --version` preflight.
    let remote_control = RemoteControlConfig {
        mode: RemoteControlMode::Off,
    };
    write_remote_control_config(&work_dir, &remote_control).unwrap();
    let scratch_root = temp.path().join("scratch");
    let host = LaunchHost {
        claude_path: PathBuf::from("/usr/bin/claude"),
        capsule_support: CapsuleSupport {
            settings: true,
            setting_sources: true,
            strict_mcp_config: true,
            append_system_prompt_file: true,
        },
        repo_root: repo.clone(),
        facts: HostFacts {
            writable_roots: vec![repo.clone(), scratch_root.clone()],
            hooks_dir: Some(hooks_dir),
            loom_bin,
            hook_path: vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")],
        },
        scratch_root,
        uid: current_uid(),
        home: None,
    };
    Fixture {
        temp,
        repo,
        work_dir,
        host,
    }
}

/// Launch `kind` for `stage-1` from where that kind runs: the stage worktree
/// for a Stage session, the repository root for every other kind.
fn try_launch(fixture: &Fixture, kind: SessionType) -> Result<(Session, String)> {
    let stage = Stage {
        id: "stage-1".to_string(),
        name: "Stage One".to_string(),
        ..Stage::default()
    };
    let cwd = if kind == SessionType::Stage {
        fixture.repo.join(".worktrees").join("stage-1")
    } else {
        fixture.repo.clone()
    };
    let signal = fixture.work_dir.join("signals").join("sig.md");
    std::fs::create_dir_all(signal.parent().unwrap()).unwrap();
    std::fs::write(&signal, "# Assignment\n").unwrap();
    let session = match kind {
        SessionType::Knowledge => Session::new_knowledge(&stage.id),
        SessionType::Adjudication => Session::new_adjudication(&stage.id),
        _ => Session::new(),
    };
    let (session, _, _, wrapper) = prepare_session_launch_with(
        &fixture.host,
        &fixture.work_dir,
        kind,
        &stage,
        session,
        &signal,
        &cwd,
    )?;
    Ok((session, std::fs::read_to_string(wrapper).unwrap()))
}

fn launch(fixture: &Fixture, kind: SessionType) -> (Session, String) {
    try_launch(fixture, kind)
        .unwrap_or_else(|error| panic!("a {kind} launch must succeed: {error:#}"))
}

fn capsule_of(fixture: &Fixture, session: &Session) -> Value {
    let path = session_settings_path(&fixture.work_dir, &session.id);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn every_kind_launches_from_an_absolute_private_capsule_with_pinned_sources() {
    let fixture = fixture();
    for kind in ALL_KINDS {
        let (session, script) = launch(&fixture, kind);
        let capsule = session_settings_path(&fixture.work_dir, &session.id)
            .canonicalize()
            .unwrap();
        let settings_arg = escape(Cow::Owned(capsule.display().to_string()));
        let flags =
            format!("--settings {settings_arg} --setting-sources user,project --strict-mcp-config");
        assert!(script.contains(&flags), "{kind}: {script}");
        assert_eq!(mode(&capsule), 0o600, "{kind}");
        assert_eq!(mode(capsule.parent().unwrap()), 0o700, "{kind}");
    }
}

#[test]
fn every_kind_exports_its_scratch_dir_the_loom_binary_and_the_hook_path() {
    let fixture = fixture();
    for kind in ALL_KINDS {
        let (session, script) = launch(&fixture, kind);
        let scratch = fixture.host.scratch_root.join(&session.id);
        for export in [
            format!("LOOM_SCRATCH_DIR={}", scratch.display()),
            format!("LOOM_BIN={}", fixture.host.facts.loom_bin.display()),
            "LOOM_HOOK_PATH=/usr/bin:/bin".to_string(),
        ] {
            assert!(
                script.contains(&export),
                "{kind} must export {export}: {script}"
            );
        }
    }
}

#[test]
fn each_capsule_on_disk_carries_its_kinds_hooks_and_no_env() {
    let fixture = fixture();
    for (kind, session_hooks) in [
        (SessionType::Stage, true),
        (SessionType::Knowledge, true),
        (SessionType::Adjudication, false),
    ] {
        let (session, _) = launch(&fixture, kind);
        let capsule = capsule_of(&fixture, &session);
        let hooks = capsule["hooks"].to_string();
        assert!(hooks.contains("post-tool-use.sh"), "{kind}: {hooks}");
        assert!(hooks.contains("loom-relay.sh"), "{kind}: {hooks}");
        assert_eq!(
            hooks.contains("session-start.sh"),
            session_hooks,
            "{kind}: {hooks}"
        );
        assert!(capsule.get("env").is_none(), "{kind}: {capsule}");
    }
}

#[test]
fn the_scratch_root_and_session_directory_are_created_0700() {
    let fixture = fixture();
    let (session, _) = launch(&fixture, SessionType::Merge);
    assert_eq!(mode(&fixture.host.scratch_root), 0o700);
    assert_eq!(mode(&fixture.host.scratch_root.join(&session.id)), 0o700);
}

#[test]
fn an_unusable_scratch_root_fails_the_launch() {
    let mut fixture = fixture();
    let blocker = fixture.temp.path().join("not-a-directory");
    std::fs::write(&blocker, "").unwrap();
    fixture.host.scratch_root = blocker;

    let error = try_launch(&fixture, SessionType::Stage)
        .expect_err("a file where the scratch root belongs must fail the spawn");

    assert!(format!("{error:#}").contains("scratch root"), "{error:#}");
}

#[test]
fn a_launch_the_preflight_refuses_fails_as_a_sandbox_refusal_and_writes_nothing() {
    let mut fixture = fixture();
    // A loom binary a session could replace: inside the repository.
    let planted = fixture.repo.join("loom");
    std::fs::write(&planted, "").unwrap();
    fixture.host.facts.loom_bin = planted;

    for kind in ALL_KINDS {
        let error = try_launch(&fixture, kind)
            .expect_err("a LOOM_BIN under a writable root must refuse the spawn");
        assert!(error.is::<SandboxPreflightRefusal>(), "{kind}: {error:#}");
        assert!(
            format!("{error:#}").starts_with("LOOM_BIN"),
            "{kind}: {error:#}"
        );
    }
    let capsules = session_settings_path(&fixture.work_dir, "none");
    assert!(
        !capsules.parent().unwrap().exists(),
        "a refused spawn writes no capsule"
    );
    assert!(
        !fixture.host.scratch_root.exists(),
        "a refused spawn makes no scratch"
    );
}

#[test]
fn a_drifted_hook_script_refuses_the_launch() {
    let fixture = fixture();
    let hooks_dir = fixture.host.facts.hooks_dir.clone().unwrap();
    std::fs::write(hooks_dir.join("loom-relay.sh"), "#!/bin/bash\n").unwrap();

    let error = try_launch(&fixture, SessionType::Stage)
        .expect_err("a hook script that differs from this build must refuse the spawn");

    assert!(error.is::<SandboxPreflightRefusal>(), "{error:#}");
    assert!(format!("{error:#}").contains("loom-relay.sh"), "{error:#}");
}

#[test]
fn hook_path_drops_entries_inside_a_writable_root_and_keeps_the_rest() {
    let writable = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let inside = writable.path().join("bin");
    std::fs::create_dir_all(&inside).unwrap();
    let path_var = std::env::join_paths([
        inside.as_path(),
        outside.path(),
        Path::new("relative/bin"),
        outside.path(),
        Path::new("/does/not/exist"),
    ])
    .unwrap();

    let kept = hook_path_entries(&path_var, &[writable.path().to_path_buf()]);

    assert_eq!(kept, vec![outside.path().canonicalize().unwrap()]);
}
