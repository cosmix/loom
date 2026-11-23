//! The boundary's temporary directories: size-bounded private `/tmp` and
//! `/var/tmp` tmpfs under every bind, the 0700 `TMPDIR` on `/tmp` with nothing
//! mounted below it, a per-boundary Cargo home, and the proxy socket directory
//! bound on the read-only root, off `/tmp`.
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::codex_home_tests::RealFixture;
use super::linux::{self, Plan, TMPFS_BYTES};
use super::tests::{bind_sources, canonical_tempdir, finish, host, no_boundary, no_bwrap};
use super::tests::{position, sh};
use super::{spawn_payload, tmpdir, BOUNDARY_TMPDIR};
use crate::process::sandbox_probe::{path_writable, skip_unless};
use crate::verify::criteria::{bubblewrap, NetworkAccess};

#[test]
fn argv_private_tmp_holds_tmpdir_under_every_bind() {
    if no_bwrap("argv_private_tmp_holds_tmpdir_under_every_bind") {
        return;
    }
    let args = RealFixture::new().argv();
    let end = args.iter().position(|arg| arg == "--").unwrap();
    let tmpfs = position(&args, &["--tmpfs".as_ref(), "/tmp".as_ref()]).expect("private /tmp");
    let dir = position(&args, &["--dir".as_ref(), BOUNDARY_TMPDIR.as_ref()]).expect("TMPDIR");
    assert!(tmpfs < dir, "{args:?}");
    assert_eq!(args[dir - 2..dir], ["--perms", "0700"], "{args:?}");
    let first_bind = args[..end]
        .iter()
        .position(|arg| arg == "--bind" || arg == "--ro-bind")
        .unwrap();
    assert!(dir < first_bind, "{args:?}");
}

#[test]
fn argv_proxy_dir_binds_off_tmp() {
    if no_bwrap("argv_proxy_dir_binds_off_tmp") {
        return;
    }
    let fixture = RealFixture::new();
    let args = fixture.argv();
    let proxy: &OsStr = fixture.proxy_dir().as_ref();
    let bind = ["--ro-bind".as_ref(), proxy, "/loom-proxy".as_ref()];
    assert!(position(&args, &bind).is_some(), "{args:?}");
    let socket = ["--proxy-socket".as_ref(), "/loom-proxy/proxy.sock".as_ref()];
    assert!(position(&args, &socket).is_some(), "{args:?}");
    let host_target = ["--ro-bind".as_ref(), proxy, proxy];
    assert!(position(&args, &host_target).is_none(), "{args:?}");
}

/// Every bind source under the host `/tmp` is one of the fixture's own
/// directories or the per-boundary `.loom-command-*` Cargo scratch; none is
/// `/tmp`, the host `TMPDIR` or an ancestor of them.
#[test]
fn argv_binds_nothing_else_from_host_tmp() {
    if no_bwrap("argv_binds_nothing_else_from_host_tmp") {
        return;
    }
    let fixture = RealFixture::new();
    let args = fixture.argv();
    let host_tmpdir = std::env::temp_dir();
    for (flag, source) in bind_sources(&args) {
        assert!(
            !Path::new("/tmp").starts_with(&source) && !host_tmpdir.starts_with(&source),
            "{flag} {} exposes a host temporary directory",
            source.display()
        );
        if !source.starts_with("/tmp") {
            continue;
        }
        let own = fixture
            .host_dirs()
            .iter()
            .any(|dir| source.starts_with(dir));
        let scratch = source.components().any(|part| {
            part.as_os_str()
                .to_string_lossy()
                .starts_with(".loom-command-")
        });
        assert!(own || scratch, "{flag} {} is unexpected", source.display());
    }
}

#[test]
#[serial_test::serial]
fn payload_writes_under_tmpdir_privately() {
    if no_boundary("payload_writes_under_tmpdir_privately") {
        return;
    }
    let (_tmp, w) = canonical_tempdir();
    let name = format!("loom-boundary-tmp-{}", std::process::id());
    let planted = tempfile::Builder::new()
        .prefix("loom-boundary-host-")
        .tempfile_in("/tmp")
        .unwrap();
    let script = format!(
        "[ \"$TMPDIR\" = {BOUNDARY_TMPDIR} ] && [ \"$(stat -c %a \"$TMPDIR\")\" = 700 ] \
         && [ ! -e {planted} ] && echo private > \"$TMPDIR/{name}\" && cat \"$TMPDIR/{name}\"",
        planted = planted.path().display()
    );
    let boundary = spawn_payload(&w, Path::new("/bin/sh"), &sh(&script)).unwrap();
    let finished = finish(boundary, Duration::from_secs(10));
    assert!(finished.status.success(), "{}", finished.stderr);
    assert_eq!(finished.stdout.trim(), "private");
    assert!(!Path::new(BOUNDARY_TMPDIR).join(&name).exists());
    assert!(!Path::new("/tmp").join(&name).exists());
}

#[test]
fn argv_bounds_both_private_tmpfs() {
    if no_bwrap("argv_bounds_both_private_tmpfs") {
        return;
    }
    let args = RealFixture::new().argv();
    for (dir, bytes) in ["/tmp", "/var/tmp"].into_iter().zip(TMPFS_BYTES) {
        let bytes = bytes.to_string();
        let sized = [
            "--size".as_ref(),
            bytes.as_ref(),
            "--tmpfs".as_ref(),
            dir.as_ref(),
        ];
        assert!(position(&args, &sized).is_some(), "{dir}: {args:?}");
        let mounts = args
            .windows(2)
            .filter(|pair| pair[0] == "--tmpfs" && pair[1] == dir);
        assert_eq!(mounts.count(), 1, "{dir}: {args:?}");
    }
    assert_eq!(TMPFS_BYTES, [4 * 1024 * 1024 * 1024, 1024 * 1024 * 1024]);
}

#[test]
fn confined_gate_tmpfs_stays_unsized() {
    if no_bwrap("confined_gate_tmpfs_stays_unsized") {
        return;
    }
    let command = bubblewrap(NetworkAccess::Denied, None).unwrap();
    assert!(!command.get_args().any(|arg| arg == "--size"));
    assert!(command.get_args().any(|arg| arg == "--tmpfs"));
}

/// The Cargo home is the boundary's own whether it is mounted (a HOME names
/// one) or Linked (none is named): never the persistent per-tree
/// `.loom-cargo-*` home the confined gate and later boundaries reuse.
#[test]
fn argv_never_binds_a_shared_cargo_home() {
    if no_bwrap("argv_never_binds_a_shared_cargo_home") {
        return;
    }
    let (_w_tmp, w) = canonical_tempdir();
    let (_home_tmp, home) = canonical_tempdir();
    let no_home: Vec<(OsString, OsString)> = vec![("PATH".into(), "/usr/bin:/bin".into())];
    for host in [host(&home), no_home] {
        let plan = Plan::payload(&w, None, Path::new("/bin/true"), &[]).unwrap();
        let (command, scratch) = linux::command(&plan, &host, 3).unwrap();
        let args: Vec<_> = command.get_args().map(OsStr::to_os_string).collect();
        let shared = args
            .iter()
            .any(|arg| arg.to_string_lossy().contains(".loom-cargo-"));
        assert!(!shared, "{args:?}");
        if let Some(linked) = scratch.linked_cargo_home() {
            assert!(linked.starts_with(scratch.path()), "{}", linked.display());
            let bind = ["--bind".as_ref(), linked.as_os_str(), linked.as_os_str()];
            assert!(position(&args, &bind).is_some(), "{args:?}");
        }
        let directory = scratch.path().to_path_buf();
        drop(scratch);
        assert!(
            !directory.exists(),
            "{} outlived the boundary",
            directory.display()
        );
    }
}

/// Skip decision for a test that creates a real directory under `BOUNDARY_TMPDIR`: true
/// when the host's `/tmp` is not writable (a host job or session sandbox), where
/// creating it fails regardless of the code under test.
fn no_host_tmp(test_name: &str) -> bool {
    skip_unless(
        path_writable(Path::new("/tmp")),
        test_name,
        "/tmp is not writable",
    )
}

/// A real directory under the host's `/tmp/loom-tmp`, removed on drop with
/// that parent when it is left empty.
struct UnderTmpdir(Option<tempfile::TempDir>);

impl UnderTmpdir {
    fn new() -> Self {
        std::fs::create_dir_all(BOUNDARY_TMPDIR).unwrap();
        Self(Some(tempfile::tempdir_in(BOUNDARY_TMPDIR).unwrap()))
    }

    fn path(&self) -> PathBuf {
        self.0.as_ref().unwrap().path().to_path_buf()
    }
}

impl Drop for UnderTmpdir {
    fn drop(&mut self) {
        drop(self.0.take());
        let _ = std::fs::remove_dir(BOUNDARY_TMPDIR);
    }
}

fn refused(plan: &Plan, host: &[(OsString, OsString)]) -> String {
    match linux::command(plan, host, 3) {
        Ok(_) => panic!("a plan under {BOUNDARY_TMPDIR} was accepted"),
        Err(error) => format!("{error:#}"),
    }
}

#[test]
#[serial_test::serial]
fn plans_under_boundary_tmpdir_are_refused() {
    if no_host_tmp("plans_under_boundary_tmpdir_are_refused") {
        return;
    }
    let (_w_tmp, w) = canonical_tempdir();
    let (_home_tmp, home) = canonical_tempdir();
    let under = UnderTmpdir::new();
    let inside = under.path();
    let plan = Plan::payload(&inside, None, Path::new("/bin/true"), &[]).unwrap();
    assert!(refused(&plan, &host(&home)).contains("the worktree"));
    let codex = inside.join(".codex");
    let plan = Plan::payload(&w, Some(&codex), Path::new("/bin/true"), &[]).unwrap();
    assert!(refused(&plan, &host(&home)).contains("the Codex home"));
    let plan = Plan::payload(&w, None, Path::new("/bin/true"), &[]).unwrap();
    for key in ["HOME", "TMPDIR", "CARGO_HOME", "RUSTUP_HOME"] {
        let mut env = host(&home);
        env.retain(|(name, _)| name != key);
        env.push((key.into(), inside.join("x").into()));
        let error = refused(&plan, &env);
        assert!(error.starts_with(&format!("{key} ")), "{error}");
        assert!(error.contains(BOUNDARY_TMPDIR), "{error}");
    }
}

/// A path outside [`BOUNDARY_TMPDIR`] that resolves into it is refused just
/// the same: only the resolved path shows where a bind would land.
#[test]
#[serial_test::serial]
fn symlinks_into_boundary_tmpdir_are_refused() {
    if no_host_tmp("symlinks_into_boundary_tmpdir_are_refused") {
        return;
    }
    let (_w_tmp, w) = canonical_tempdir();
    let (_home_tmp, home) = canonical_tempdir();
    let under = UnderTmpdir::new();
    let inside = under.path();
    std::fs::create_dir(inside.join("x")).unwrap();
    let (_links_tmp, links) = canonical_tempdir();
    let link = links.join("into-tmpdir");
    std::os::unix::fs::symlink(&inside, &link).unwrap();
    assert!(!link.starts_with(BOUNDARY_TMPDIR));
    let plan = Plan::payload(&w, Some(&link), Path::new("/bin/true"), &[]).unwrap();
    let error = refused(&plan, &host(&home));
    assert!(error.starts_with("the Codex home "), "{error}");
    let plan = Plan::payload(&w, None, Path::new("/bin/true"), &[]).unwrap();
    for (key, path) in [("HOME", link.clone()), ("CARGO_HOME", link.join("x"))] {
        let mut env = host(&home);
        env.retain(|(name, _)| name != key);
        env.push((key.into(), path.into()));
        let error = refused(&plan, &env);
        assert!(error.starts_with(&format!("{key} ")), "{error}");
    }
}

/// Every option the real and payload argv carry is known to the mount check;
/// an unknown one, or one cut short, is refused before anything starts.
#[test]
fn mount_check_knows_every_emitted_option_and_refuses_others() {
    if no_bwrap("mount_check_knows_every_emitted_option_and_refuses_others") {
        return;
    }
    let fixture = RealFixture::new();
    let mut real = Command::new("bwrap");
    real.args(fixture.argv());
    tmpdir::refuse_mount_targets(&real).unwrap();
    let codex = fixture.home.join(".codex");
    let plan = Plan::payload(&fixture.w, Some(&codex), Path::new("/bin/true"), &[]).unwrap();
    let (payload, _scratch) = linux::command(&plan, &host(&fixture.home), 3).unwrap();
    tmpdir::refuse_mount_targets(&payload).unwrap();
    for flags in [
        &["--overlay", "/a", "/b", "/c"][..],
        &["--tmp-overlay", "/usr"],
        &["--ro-overlay", "/usr"],
        &["--file", "3", "/etc/x"],
        &["--bind-data", "3", "/etc/x"],
        &["--dev-bind-try", "/dev", "/dev"],
        &["--symlink", "/usr", "/x"],
        &["/usr"],
    ] {
        let mut command = Command::new("bwrap");
        command
            .args(["--unshare-all"])
            .args(flags)
            .args(["--", "true"]);
        let error = tmpdir::refuse_mount_targets(&command).unwrap_err();
        assert!(
            format!("{error}").contains("does not know"),
            "{flags:?}: {error}"
        );
    }
    let mut short = Command::new("bwrap");
    short.args(["--ro-bind", "/usr"]);
    assert!(tmpdir::refuse_mount_targets(&short).is_err());
}

/// Derived mounts no plan names are refused too; arguments after `--` and the
/// `TMPDIR` directory itself are not mounts.
#[test]
fn mount_targets_under_boundary_tmpdir_are_refused() {
    let below = format!("{BOUNDARY_TMPDIR}/x");
    for flags in [
        &["--ro-bind", "/usr", &below][..],
        &["--bind", "/usr", &below],
        &["--tmpfs", &below],
    ] {
        let mut command = Command::new("bwrap");
        command.args(flags).args(["--", "true"]);
        assert!(tmpdir::refuse_mount_targets(&command).is_err(), "{flags:?}");
    }
    let mut command = Command::new("bwrap");
    command.args([
        "--perms",
        "0700",
        "--dir",
        BOUNDARY_TMPDIR,
        "--ro-bind",
        "/usr",
        "/usr",
    ]);
    command.args(["--", "sh", "--bind", "/usr", &below]);
    tmpdir::refuse_mount_targets(&command).unwrap();
}
