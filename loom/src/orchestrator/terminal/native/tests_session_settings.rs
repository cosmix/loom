//! Unit tests for `native/session_settings.rs`: the capsule file, its
//! lifecycle, and what `write_session_capsule` renders into it. Declared as a
//! sibling module the way `tests_capsule.rs` and `tests_wrapper_env.rs` are
//! (CLAUDE.md Rule 17 keeps test files split out of the module they cover).
//! The fixture below is shared with `tests_capsule.rs`, which checks the
//! capsule's write denies.

use super::*;
use crate::models::stage::{Implementer, Implementers, StageType};
use crate::orchestrator::terminal::native::session_settings::{
    write_capsule_file, write_session_capsule, CapsuleRequest,
};
use crate::sandbox::control_surfaces::ControlSurfaces;
use crate::sandbox::MergedSandboxConfig;
use serde_json::{json, Value};
use serial_test::serial;
use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

pub(super) const ALL_KINDS: [SessionType; 5] = [
    SessionType::Stage,
    SessionType::Knowledge,
    SessionType::Merge,
    SessionType::BaseConflict,
    SessionType::Adjudication,
];

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

pub(super) fn sandbox_with(lanes: Vec<Implementer>) -> MergedSandboxConfig {
    crate::sandbox::merge_config(
        &Default::default(),
        &Stage::default().sandbox,
        StageType::Standard,
        &Implementers::new(lanes),
    )
}

fn default_sandbox() -> MergedSandboxConfig {
    sandbox_with(vec![Implementer::Claude])
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

/// A repository with a state root and a stage worktree, a hooks directory, an
/// operator home, where its scratch directories go, and the `/tmp` its
/// host-socket scan reads; every path canonical.
pub(super) struct Checkout {
    _temp: TempDir,
    pub(super) repo: PathBuf,
    pub(super) worktree: PathBuf,
    work_dir: PathBuf,
    pub(super) hooks_dir: PathBuf,
    pub(super) home: PathBuf,
    scratch_root: PathBuf,
    tmp_root: PathBuf,
}

pub(super) fn checkout() -> Checkout {
    let temp = TempDir::new().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let repo = root.join("repo");
    let worktree = repo.join(".worktrees").join("s1");
    let work_dir = repo.join(".loom").join("work");
    let hooks_dir = root.join("hooks");
    let home = root.join("home");
    for dir in [&worktree, &work_dir, &hooks_dir, &home] {
        std::fs::create_dir_all(dir).unwrap();
    }
    Checkout {
        _temp: temp,
        repo,
        worktree,
        work_dir,
        hooks_dir,
        home,
        scratch_root: root.join("scratch"),
        tmp_root: root.join("tmp"),
    }
}

fn surfaces_for(checkout: &Checkout) -> ControlSurfaces {
    let scratch = Some(checkout.scratch_root.as_path());
    let home = Some(checkout.home.as_path());
    ControlSurfaces::new(&checkout.work_dir, scratch, &[], home)
}

/// Write `kind`'s capsule for a session running in `cwd`.
pub(super) fn write_capsule(
    checkout: &Checkout,
    session_id: &str,
    kind: SessionType,
    cwd: &Path,
    sandbox: &MergedSandboxConfig,
    hooks_dir: Option<&Path>,
) -> Result<String> {
    let scratch_dir = checkout.scratch_root.join(session_id);
    write_session_capsule(&CapsuleRequest {
        kind,
        session_id,
        sandbox,
        cwd,
        work_dir: &checkout.work_dir,
        repo_root: &checkout.repo,
        hooks_dir,
        scratch_dir: &scratch_dir,
        surfaces: &surfaces_for(checkout),
        writable_roots: &[],
        python3: None,
        python_hooks: &[],
        socket_scan_root: &checkout.tmp_root,
    })
}

fn write_merge_capsule(checkout: &Checkout, session_id: &str) -> Result<String> {
    write_capsule(
        checkout,
        session_id,
        SessionType::Merge,
        &checkout.repo,
        &default_sandbox(),
        Some(&checkout.hooks_dir),
    )
}

pub(super) fn read_capsule(path: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

pub(super) fn strings(capsule: &Value, pointer: &str) -> BTreeSet<String> {
    let array = capsule.pointer(pointer).and_then(Value::as_array);
    let values = array.into_iter().flatten().filter_map(Value::as_str);
    values.map(str::to_owned).collect()
}

#[test]
fn write_session_capsule_renders_the_approved_list_and_the_scratch_grant() {
    let checkout = checkout();
    let approved = vec!["Bash(cargo test:*)".to_string()];
    crate::fs::permissions::approved::record_approved(
        &checkout.work_dir,
        &approved,
        &surfaces_for(&checkout),
    )
    .unwrap();

    let capsule = read_capsule(&write_merge_capsule(&checkout, "session-app1").unwrap());
    #[cfg(target_os = "macos")]
    assert_eq!(
        strings(&capsule, "/sandbox/network/allowUnixSockets"),
        BTreeSet::from([crate::daemon::socket_path(&checkout.work_dir)
            .display()
            .to_string()]),
    );

    let allow = strings(&capsule, "/permissions/allow");
    let scratch = checkout.scratch_root.join("session-app1");
    for rule in [
        "Bash(cargo test:*)".to_string(),
        format!("Edit(/{}/**)", scratch.display()),
    ] {
        assert!(allow.contains(&rule), "missing {rule}: {capsule}");
    }
}

#[test]
fn write_session_capsule_refuses_an_unparseable_checkout_settings_file() {
    let checkout = checkout();
    let claude_dir = checkout.repo.join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(claude_dir.join("settings.local.json"), "not json").unwrap();

    let error = write_merge_capsule(&checkout, "session-bad1")
        .expect_err("an unparseable checkout settings file must refuse the capsule");

    assert!(
        format!("{error:#}").contains("settings.local.json"),
        "{error:#}"
    );
}

#[test]
fn a_spawn_without_a_verified_hooks_dir_is_refused_for_every_kind() {
    let checkout = checkout();
    let sandbox = default_sandbox();
    for kind in ALL_KINDS {
        for cwd in [&checkout.repo, &checkout.worktree] {
            let error = write_capsule(&checkout, "session-nh1", kind, cwd, &sandbox, None)
                .expect_err("a spawn with no verified hooks directory must be refused");
            assert!(
                format!("{error:#}").contains("hooks directory"),
                "{kind}: {error:#}"
            );
        }
    }
    assert!(
        !checkout.work_dir.join("capsules").exists(),
        "a refused spawn writes no capsule"
    );
}

/// Bind an IDE socket under the checkout's `/tmp` and the daemon socket, and
/// plant links at the tmux and X11 candidates' fixed places, pointing at the
/// repository and at a directory inside the worktree; return what a granted
/// stage's capsule hides on Linux.
fn plant_host_sockets(checkout: &Checkout) -> Vec<String> {
    let ide_dir = checkout.tmp_root.join("ide");
    let daemon_socket = checkout.work_dir.join("orchestrator.sock");
    for socket in [&ide_dir.join("ide.sock"), &daemon_socket] {
        drop(std::os::unix::net::UnixListener::bind(socket).unwrap());
    }
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    let tmux_link = checkout.tmp_root.join(format!("tmux-{uid}"));
    std::os::unix::fs::symlink(&checkout.repo, tmux_link).unwrap();
    let src = checkout.worktree.join("src");
    std::fs::create_dir(&src).unwrap();
    std::os::unix::fs::symlink(src, checkout.tmp_root.join(".X11-unix")).unwrap();
    ["/run", "/var/run"]
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .chain([ide_dir, daemon_socket])
        .map(|path| path.display().to_string())
        .collect()
}

#[test]
#[serial]
fn a_stage_granted_every_unix_socket_hides_the_host_sockets_for_every_kind() {
    let checkout = checkout();
    std::fs::create_dir_all(checkout.tmp_root.join("ide")).unwrap();
    if crate::process::sandbox_probe::skip_unless(
        crate::process::sandbox_probe::unix_socket_bindable(&checkout.tmp_root),
        "a_stage_granted_every_unix_socket_hides_the_host_sockets_for_every_kind",
        "this sandbox denies binding an AF_UNIX listener",
    ) {
        return;
    }
    let planted = plant_host_sockets(&checkout);
    let expected = if cfg!(target_os = "linux") {
        planted
    } else {
        Vec::new()
    };
    let mut granted = default_sandbox();
    granted.network.allow_all_unix_sockets = true;

    let deny_read = |kind: SessionType, sandbox: &MergedSandboxConfig| {
        let cwd = &checkout.worktree;
        let hooks = Some(checkout.hooks_dir.as_path());
        let path = write_capsule(&checkout, "session-us1", kind, cwd, sandbox, hooks).unwrap();
        strings(&read_capsule(&path), "/sandbox/filesystem/denyRead")
    };
    for kind in ALL_KINDS {
        let plain = deny_read(kind, &default_sandbox());
        let added: BTreeSet<String> = deny_read(kind, &granted)
            .difference(&plain)
            .cloned()
            .collect();
        for entry in &expected {
            assert!(added.contains(entry), "{kind}: {entry} not in {added:?}");
        }
        assert_eq!(added.is_empty(), expected.is_empty(), "{kind}: {added:?}");
        // Resolved as the sandbox's mount would resolve it: the repository
        // behind the tmux link and the worktree's `src` behind the X11 link
        // stay visible; inside the repository only the daemon socket is hidden.
        let daemon_socket = checkout.work_dir.join("orchestrator.sock");
        let hides_tree = |entry: &String| {
            std::fs::canonicalize(entry).is_ok_and(|entry| {
                checkout.worktree.starts_with(&entry)
                    || (entry.starts_with(&checkout.repo) && entry != daemon_socket)
            })
        };
        assert!(!added.iter().any(hides_tree), "{added:?}");
    }
}
