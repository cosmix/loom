//! The boundary's temporary directories: a private empty `/tmp` tmpfs under
//! every bind holding the 0700 `TMPDIR`, and the proxy socket directory bound
//! on the read-only root, off `/tmp`.
use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;

use super::codex_home_tests::RealFixture;
use super::tests::{bind_sources, canonical_tempdir, finish, no_boundary, no_bwrap};
use super::tests::{position, sh};
use super::{spawn_payload, BOUNDARY_TMPDIR};

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
/// directories or the Cargo scratch (the per-boundary `.loom-command-*` or
/// the Linked `.loom-cargo-*` home); none is `/tmp`, the host `TMPDIR` or an
/// ancestor of them.
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
            let part = part.as_os_str().to_string_lossy();
            part.starts_with(".loom-cargo-") || part.starts_with(".loom-command-")
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
