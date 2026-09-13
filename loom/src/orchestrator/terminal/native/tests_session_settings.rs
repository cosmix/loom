//! Unit tests for `native/session_settings.rs`: the capsule file, its
//! lifecycle, and what `write_session_capsule` renders into it. Declared as a
//! sibling module the way `tests_capsule.rs` and `tests_wrapper_env.rs` are
//! (CLAUDE.md Rule 17 keeps test files split out of the module they cover).

use super::*;
use crate::models::stage::{Implementers, StageType};
use crate::orchestrator::terminal::native::session_settings::{
    write_capsule_file, write_session_capsule, CapsuleRequest,
};
use crate::sandbox::control_surfaces::ControlSurfaces;
use crate::sandbox::MergedSandboxConfig;
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn default_sandbox() -> MergedSandboxConfig {
    crate::sandbox::merge_config(
        &Default::default(),
        &Stage::default().sandbox,
        StageType::Standard,
        &Implementers::default(),
    )
}

/// `target` spelled relative to the process's current directory, which this
/// reads but never changes.
fn relative_to_cwd(target: &Path) -> PathBuf {
    let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
    let mut relative: PathBuf = cwd.components().skip(1).map(|_| "..").collect();
    relative.push(target.canonicalize().unwrap().strip_prefix("/").unwrap());
    relative
}

#[test]
fn write_capsule_file_is_private_and_overwrites_cleanly() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");

    let path = write_capsule_file(&work_dir, "session-abc123", &json!({"a": 1})).unwrap();

    let expected = work_dir.canonicalize().unwrap().join("capsules");
    assert_eq!(path, expected.join("session-abc123.settings.json"));
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(path.parent().unwrap()), 0o700);

    let again = write_capsule_file(&work_dir, "session-abc123", &json!({"a": 2})).unwrap();
    assert_eq!(again, path);
    let content: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(content, json!({"a": 2}));
    assert_eq!(mode(&path), 0o600);
}

#[test]
fn write_capsule_file_tightens_an_existing_capsules_directory() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    let dir = work_dir.join("capsules");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

    write_capsule_file(&work_dir, "session-t1", &json!({})).unwrap();

    assert_eq!(mode(&dir), 0o700);
}

#[test]
fn write_capsule_file_returns_an_absolute_path_for_a_relative_work_dir() {
    let temp = TempDir::new().unwrap();
    let relative = relative_to_cwd(temp.path());
    assert!(relative.is_relative(), "{}", relative.display());

    let path = write_capsule_file(&relative, "session-rel1", &json!({})).unwrap();

    assert!(path.is_absolute(), "{}", path.display());
    let expected = temp.path().canonicalize().unwrap().join("capsules");
    assert_eq!(path, expected.join("session-rel1.settings.json"));
}

#[test]
fn write_capsule_file_rejects_an_invalid_session_id() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");

    assert!(write_capsule_file(&work_dir, "../etc/passwd", &json!({})).is_err());
    assert!(
        !work_dir.join("capsules").exists(),
        "a rejected session id must not create the capsules directory"
    );
}

#[test]
fn write_capsule_file_refuses_a_symlinked_capsules_directory() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    let attacker_target = temp.path().join("attacker");
    std::fs::create_dir_all(&attacker_target).unwrap();
    std::os::unix::fs::symlink(&attacker_target, work_dir.join("capsules")).unwrap();

    let error = write_capsule_file(&work_dir, "session-sym1", &json!({}))
        .expect_err("a symlinked capsules directory must be refused");

    assert!(format!("{error:#}").contains("capsules"), "{error:#}");
    assert!(
        std::fs::read_dir(&attacker_target)
            .unwrap()
            .next()
            .is_none(),
        "must not write through the symlinked capsules directory"
    );
}

#[test]
fn write_capsule_file_refuses_a_symlinked_capsule_file() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    let dir = work_dir.join("capsules");
    std::fs::create_dir_all(&dir).unwrap();
    let attacker_target = temp.path().join("attacker.json");
    std::fs::write(&attacker_target, "{}").unwrap();
    std::os::unix::fs::symlink(&attacker_target, dir.join("session-sym2.settings.json")).unwrap();

    let error = write_capsule_file(&work_dir, "session-sym2", &json!({"a": 1}))
        .expect_err("a symlinked capsule file must be refused");

    assert!(
        format!("{error:#}").contains("session-sym2.settings.json"),
        "{error:#}"
    );
    assert_eq!(std::fs::read_to_string(&attacker_target).unwrap(), "{}");
}

#[test]
fn cleanup_session_settings_removes_the_file_and_tolerates_a_missing_one() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    let path = write_capsule_file(&work_dir, "session-cleanup1", &json!({})).unwrap();

    cleanup_session_settings(&work_dir, "session-cleanup1");
    assert!(!path.exists());

    // Idempotent: the daemon's own close path is best-effort.
    cleanup_session_settings(&work_dir, "session-cleanup1");
}

/// A repository with a state root, and where its scratch directories go.
struct Checkout {
    _temp: TempDir,
    repo: PathBuf,
    work_dir: PathBuf,
    scratch_root: PathBuf,
}

fn checkout() -> Checkout {
    let temp = TempDir::new().unwrap();
    let repo = temp.path().join("repo");
    let work_dir = repo.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    let scratch_root = temp.path().join("scratch");
    Checkout {
        _temp: temp,
        repo,
        work_dir,
        scratch_root,
    }
}

fn surfaces_for(checkout: &Checkout) -> ControlSurfaces {
    let state_root = checkout.work_dir.canonicalize().unwrap();
    ControlSurfaces::new(&state_root, Some(&checkout.scratch_root), &[], None)
}

fn write_merge_capsule(
    checkout: &Checkout,
    session_id: &str,
    surfaces: &ControlSurfaces,
) -> Result<String> {
    let sandbox = default_sandbox();
    let scratch_dir = checkout.scratch_root.join(session_id);
    write_session_capsule(&CapsuleRequest {
        kind: SessionType::Merge,
        session_id,
        sandbox: &sandbox,
        cwd: &checkout.repo,
        work_dir: &checkout.work_dir,
        repo_root: &checkout.repo,
        hooks_dir: None,
        scratch_dir: &scratch_dir,
        surfaces,
    })
}

#[test]
fn write_session_capsule_renders_the_approved_list_and_the_scratch_grant() {
    let checkout = checkout();
    let surfaces = surfaces_for(&checkout);
    let approved = vec!["Bash(cargo test:*)".to_string()];
    crate::fs::permissions::approved::record_approved(&checkout.work_dir, &approved, &surfaces)
        .unwrap();

    let path = write_merge_capsule(&checkout, "session-app1", &surfaces).unwrap();

    let capsule: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let allow = capsule["permissions"]["allow"].as_array().unwrap();
    let scratch = checkout.scratch_root.join("session-app1");
    for rule in [
        "Bash(cargo test:*)".to_string(),
        format!("Edit(/{}/**)", scratch.display()),
    ] {
        assert!(
            allow.iter().any(|value| value == &json!(rule)),
            "missing {rule}: {capsule}"
        );
    }
}

#[test]
fn write_session_capsule_refuses_an_unparseable_checkout_settings_file() {
    let checkout = checkout();
    let claude_dir = checkout.repo.join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(claude_dir.join("settings.local.json"), "not json").unwrap();

    let error = write_merge_capsule(&checkout, "session-bad1", &surfaces_for(&checkout))
        .expect_err("an unparseable checkout settings file must refuse the capsule");

    assert!(
        format!("{error:#}").contains("settings.local.json"),
        "{error:#}"
    );
}
