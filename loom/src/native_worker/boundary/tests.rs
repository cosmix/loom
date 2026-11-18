use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::Duration;

use tempfile::TempDir;

use super::environment::environment;
use super::linux::{self, Plan};
use super::mounts::{codex_package, map_store_roots};
use super::{identity_for, spawn_payload, verify_quiescent, Quiescence, SpawnedBoundary};
use crate::native_worker::model::BoundaryIdentity;
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};

pub(super) const NEEDS_BWRAP: &str = "needs a working bubblewrap pid namespace";

pub(super) fn canonical_tempdir() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().canonicalize().unwrap();
    (tmp, path)
}

pub(super) fn sh(script: &str) -> Vec<OsString> {
    vec!["-c".into(), script.into()]
}

/// Skip a test that starts a boundary.
pub(super) fn no_boundary(name: &str) -> bool {
    skip_unless(confined_commands_runnable(), name, NEEDS_BWRAP)
}

/// Skip a test that only builds the argument list: it needs `bwrap` on PATH.
pub(super) fn no_bwrap(name: &str) -> bool {
    skip_unless(which::which("bwrap").is_ok(), name, "bwrap is not on PATH")
}

/// A host environment whose HOME is `home`.
pub(super) fn host(home: &Path) -> Vec<(OsString, OsString)> {
    vec![
        ("HOME".into(), home.into()),
        ("PATH".into(), "/usr/bin:/bin".into()),
    ]
}

/// The bubblewrap arguments for `plan` under `host`.
pub(super) fn argv(plan: &Plan, host: &[(OsString, OsString)]) -> Vec<OsString> {
    let (command, _scratch) = linux::command(plan, host, 3).unwrap();
    command.get_args().map(OsStr::to_os_string).collect()
}

/// `(flag, source)` of every `--bind` and `--ro-bind` before `--`.
pub(super) fn bind_sources(args: &[OsString]) -> Vec<(String, PathBuf)> {
    let end = args
        .iter()
        .position(|arg| arg == "--")
        .unwrap_or(args.len());
    args[..end]
        .windows(2)
        .filter(|pair| pair[0] == "--bind" || pair[0] == "--ro-bind")
        .map(|pair| {
            (
                pair[0].to_string_lossy().into_owned(),
                PathBuf::from(&pair[1]),
            )
        })
        .collect()
}

/// Index of the first occurrence of `items` as a contiguous run in `args`.
pub(super) fn position(args: &[OsString], items: &[&OsStr]) -> Option<usize> {
    args.windows(items.len())
        .position(|window| window.iter().zip(items).all(|(arg, item)| arg == item))
}

pub(super) struct Finished {
    pub(super) status: ExitStatus,
    pub(super) stdout: String,
    pub(super) stderr: String,
}

/// Wait for `boundary` to exit, retire it, require the namespace empty, and
/// return what it printed.
pub(super) fn finish(mut boundary: SpawnedBoundary, timeout: Duration) -> Finished {
    let identity = identity_for(&boundary).expect("boundary identity");
    drop(boundary.stdin);
    let exited = boundary.process.wait(timeout);
    boundary.process.retire();
    let status = match exited {
        Ok(Some(status)) => status,
        other => panic!("payload did not exit: {other:?}"),
    };
    let (mut stdout, mut stderr) = (String::new(), String::new());
    boundary.stdout.read_to_string(&mut stdout).unwrap();
    boundary.stderr.read_to_string(&mut stderr).unwrap();
    assert_eq!(verify_quiescent(&identity), Quiescence::Released);
    Finished {
        status,
        stdout,
        stderr,
    }
}

#[test]
fn argv_has_no_network_and_orders_mounts() {
    if no_bwrap("argv_has_no_network_and_orders_mounts") {
        return;
    }
    let (_w_tmp, w) = canonical_tempdir();
    let (_home_tmp, home) = canonical_tempdir();
    std::fs::create_dir(w.join(".loom")).unwrap();
    let plan = Plan::payload(&w, None, Path::new("/bin/true"), &[]).unwrap();
    let args = argv(&plan, &host(&home));
    assert!(!args.iter().any(|arg| arg == "--share-net"), "{args:?}");
    for flag in ["--unshare-all", "--die-with-parent", "--new-session"] {
        assert!(args.iter().any(|arg| arg == flag), "{flag}: {args:?}");
    }
    let loom = w.join(".loom");
    let bind_w = position(&args, &["--bind".as_ref(), w.as_ref(), w.as_ref()]).unwrap();
    let ro_loom = position(&args, &["--ro-bind".as_ref(), loom.as_ref(), loom.as_ref()]);
    assert!(bind_w < ro_loom.expect("W/.loom read-only"), "{args:?}");
    let remount = position(&args, &["--remount-ro".as_ref(), "/".as_ref()]).unwrap();
    let end = args.iter().position(|arg| arg == "--").unwrap();
    assert!(remount < end, "{args:?}");
    assert!(position(&args, &["--info-fd".as_ref(), "3".as_ref()]).is_some());
}

#[test]
fn argv_exposes_no_secrets_or_state_root() {
    if no_bwrap("argv_exposes_no_secrets_or_state_root") {
        return;
    }
    let fixture = super::codex_home_tests::RealFixture::new();
    std::fs::create_dir_all(fixture.home.join(".ssh")).unwrap();
    std::fs::write(fixture.home.join(".ssh/k"), "key").unwrap();
    let (_main_tmp, main) = canonical_tempdir();
    let state = main.join(".loom/work");
    std::fs::create_dir_all(state.join("context")).unwrap();
    std::fs::create_dir_all(fixture.w.join(".loom")).unwrap();
    std::os::unix::fs::symlink(&state, fixture.w.join(".loom/work")).unwrap();
    let args = fixture.argv();
    let codex = fixture.home.join(".codex");
    for (flag, source) in bind_sources(&args) {
        assert!(
            !source.components().any(|part| part.as_os_str() == ".ssh"),
            "{flag} {}",
            source.display()
        );
        assert!(source != state && source != fixture.w.join(".loom/work"));
        assert!(!source.starts_with("/run"), "{flag} {}", source.display());
        if source.starts_with(&fixture.home) {
            assert!(source.starts_with(&codex), "{flag} {}", source.display());
        }
    }
    assert!(position(&args, &["--tmpfs".as_ref(), codex.as_ref()]).is_some());
}

#[test]
fn environment_is_an_allowlist() {
    let host: Vec<(OsString, OsString)> = "HOME=/home/u PATH=/secret/bin LOOM_SESSION_ID=s \
         LOOM_STAGE_ID=t LOOM_WORK_DIR=/w SSH_AUTH_SOCK=/run/ssh API_TOKEN=x \
         XDG_RUNTIME_DIR=/run/user/1 RUSTUP_HOME=/home/u/.rustup CARGO_HOME=/home/u/.cargo"
        .split_whitespace()
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.into(), value.into()))
        .collect();
    let keys = |env: &[(OsString, OsString)]| {
        let mut keys: Vec<String> = env
            .iter()
            .map(|(key, _)| key.to_string_lossy().into())
            .collect();
        keys.sort();
        keys
    };
    let real = environment(&host, true, Some(Path::new("/pkg/codex-path")));
    let mut expected: Vec<String> = "PATH HOME LANG TERM TMPDIR TMP TEMP GIT_CONFIG_GLOBAL \
         LOOM_CODEX_FORWARD LOOM_BIN RUSTUP_HOME CARGO_HOME"
        .split_whitespace()
        .map(String::from)
        .collect();
    expected.sort();
    assert_eq!(keys(&real), expected);
    let path = |env: &[(OsString, OsString)]| env[0].1.to_string_lossy().into_owned();
    assert!(path(&real).starts_with("/loom-bin:/pkg/codex-path:"));
    assert!(!path(&real).contains("/secret"));
    let payload = environment(&host, false, None);
    expected.retain(|key| key != "LOOM_BIN");
    assert_eq!(keys(&payload), expected);
    assert!(!path(&payload).contains("/loom-bin"));
}

#[test]
#[serial_test::serial]
fn git_roots_refuse_a_forged_gitdir() {
    if no_bwrap("git_roots_refuse_a_forged_gitdir") {
        return;
    }
    use crate::git::target_guard::test_support::{git, repo};
    let repo = repo();
    git(
        &repo.root,
        &["worktree", "add", "-q", "-b", "a", ".worktrees/a"],
    );
    let common = repo.root.canonicalize().unwrap().join(".git");
    let w = common.parent().unwrap().join(".worktrees/a");
    let (_home_tmp, home) = canonical_tempdir();
    let sources = |w: &Path| {
        let plan = Plan::payload(w, None, Path::new("/bin/true"), &[]).unwrap();
        bind_sources(&argv(&plan, &host(&home)))
            .into_iter()
            .map(|(_, source)| source)
            .collect::<Vec<_>>()
    };
    let consistent = sources(&w);
    for expected in [common.join("worktrees/a"), common.join("objects")] {
        assert!(
            consistent.contains(&expected),
            "{expected:?}: {consistent:?}"
        );
    }
    let (_outside_tmp, outside) = canonical_tempdir();
    std::fs::write(w.join(".git"), format!("gitdir: {}\n", outside.display())).unwrap();
    let forged = sources(&w);
    assert!(
        !forged.iter().any(|source| source.starts_with(&common)),
        "{forged:?}"
    );
    assert!(!forged.contains(&outside), "{forged:?}");
}

#[test]
fn map_store_roots_bind_only_context_and_cache() {
    let (_main_tmp, main) = canonical_tempdir();
    let (_w_tmp, w) = canonical_tempdir();
    for dir in ["cache/context-v1", "work/context", "work/native-workers"] {
        std::fs::create_dir_all(main.join(".loom").join(dir)).unwrap();
    }
    std::fs::create_dir(w.join(".loom")).unwrap();
    std::os::unix::fs::symlink(main.join(".loom/work"), w.join(".loom/work")).unwrap();
    assert_eq!(
        map_store_roots(&w),
        vec![
            main.join(".loom/cache/context-v1"),
            main.join(".loom/work/context")
        ]
    );
}

#[test]
fn codex_script_is_refused() {
    let (_tmp, dir) = canonical_tempdir();
    let script = dir.join("codex");
    std::fs::write(&script, "#!/bin/sh\nexec node codex.js\n").unwrap();
    let error = codex_package(&script).unwrap_err();
    assert!(format!("{error:#}").contains("script"), "{error:#}");
}

fn identity(pid_namespace: u64, boot_id: String) -> BoundaryIdentity {
    BoundaryIdentity {
        supervisor_pid: std::process::id() as i32,
        supervisor_start: "0".into(),
        boundary_pid: 0,
        pid_namespace,
        boot_id,
    }
}

fn boot_id() -> String {
    crate::process::boot_id::os_boot_id().unwrap()
}

#[test]
fn quiescence_reports_own_namespace_as_unverified() {
    let link = std::fs::read_link("/proc/self/ns/pid").unwrap();
    let link = link.to_string_lossy();
    let inode = link
        .strip_prefix("pid:[")
        .and_then(|rest| rest.strip_suffix(']'))
        .and_then(|inode| inode.parse().ok())
        .unwrap();
    let quiescence = verify_quiescent(&identity(inode, boot_id()));
    assert!(
        matches!(quiescence, Quiescence::Unverified(_)),
        "{quiescence:?}"
    );
}

#[test]
fn quiescence_other_boot_is_released() {
    let started = std::time::Instant::now();
    let other = identity(1, "not-this-boot".into());
    assert_eq!(verify_quiescent(&other), Quiescence::Released);
    assert!(started.elapsed() < Duration::from_millis(500));
}

#[test]
fn quiescence_unused_namespace_is_released() {
    assert_eq!(
        verify_quiescent(&identity(1, boot_id())),
        Quiescence::Released
    );
}

#[test]
#[serial_test::serial]
fn payload_sees_only_loopback() {
    if no_boundary("payload_sees_only_loopback") {
        return;
    }
    let (_tmp, w) = canonical_tempdir();
    let script = "cut -d: -f1 /proc/net/dev | tail -n +3";
    let boundary = spawn_payload(&w, Path::new("/bin/sh"), &sh(script)).unwrap();
    let out = finish(boundary, Duration::from_secs(10)).stdout;
    let interfaces: Vec<&str> = out.lines().map(str::trim).collect();
    assert_eq!(interfaces, ["lo"]);
}

#[test]
#[serial_test::serial]
fn payload_filesystem_policy() {
    if no_boundary("payload_filesystem_policy") {
        return;
    }
    let home = crate::fs::permissions::scratch_home::ScratchHome::set();
    std::fs::create_dir(home.path().join(".ssh")).unwrap();
    std::fs::write(home.path().join(".ssh/k"), "key").unwrap();
    let (_tmp, w) = canonical_tempdir();
    std::fs::create_dir(w.join(".loom")).unwrap();
    let ws = w.display();
    let script = format!(
        "r=''; [ ! -e \"$HOME/.ssh\" ] && r=\"${{r}}nossh \"; \
         touch '{ws}/x' && r=\"${{r}}w \"; \
         touch '{ws}/.loom/x' 2>/dev/null || r=\"${{r}}noloom \"; \
         touch /etc/x 2>/dev/null || r=\"${{r}}noetc \"; \
         ls /run >/dev/null 2>&1 || r=\"${{r}}norun \"; echo \"$r\""
    );
    let boundary = spawn_payload(&w, Path::new("/bin/sh"), &sh(&script)).unwrap();
    let out = finish(boundary, Duration::from_secs(10)).stdout;
    assert_eq!(out.trim(), "nossh w noloom noetc norun");
    assert!(w.join("x").is_file());
    assert!(!w.join(".loom/x").exists());
}

#[test]
#[serial_test::serial]
fn payload_namespace_matches_info() {
    if no_boundary("payload_namespace_matches_info") {
        return;
    }
    let (_tmp, w) = canonical_tempdir();
    let boundary =
        spawn_payload(&w, Path::new("/bin/sh"), &sh("readlink /proc/self/ns/pid")).unwrap();
    let expected = format!("pid:[{}]", boundary.pid_namespace);
    assert_eq!(
        finish(boundary, Duration::from_secs(10)).stdout.trim(),
        expected
    );
}

#[test]
#[serial_test::serial]
fn spawn_failure_leaves_no_process() {
    if no_boundary("spawn_failure_leaves_no_process") {
        return;
    }
    let (_tmp, w) = canonical_tempdir();
    if let Ok(boundary) = spawn_payload(&w, Path::new("/nonexistent"), &[]) {
        let finished = finish(boundary, Duration::from_secs(10));
        assert!(!finished.status.success());
    }
}

#[test]
#[serial_test::serial]
fn payload_cargo_check_compiles() {
    if no_boundary("payload_cargo_check_compiles") {
        return;
    }
    let (_tmp, w) = canonical_tempdir();
    let manifest =
        "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n";
    std::fs::write(w.join("Cargo.toml"), manifest).unwrap();
    std::fs::create_dir(w.join("src")).unwrap();
    std::fs::write(w.join("src/lib.rs"), "pub fn one() -> u8 {\n    1\n}\n").unwrap();
    let cargo = std::env::var_os("CARGO")
        .map(PathBuf::from)
        .or_else(|| which::which("cargo").ok())
        .expect("cargo executable");
    let args: Vec<OsString> = ["check", "--offline"].map(OsString::from).into();
    let boundary = spawn_payload(&w, &cargo, &args).unwrap();
    let finished = finish(boundary, Duration::from_secs(120));
    assert!(
        finished.status.success(),
        "cargo check: {}",
        finished.stderr
    );
    assert!(w.join("target").is_dir());
}
