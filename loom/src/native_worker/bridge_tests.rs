use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use tempfile::TempDir;

use crate::process::sandbox_probe::{require_unix_socket_and_loopback, skip_without_loopback};

use super::{listen, proxy_environment, run};

#[test]
fn proxy_environment_lists_every_variable() {
    let vars = proxy_environment(4321);
    let url = "http://127.0.0.1:4321";
    let loopback = "127.0.0.1,localhost,::1";
    let expected = [
        ("HTTPS_PROXY", url),
        ("https_proxy", url),
        ("HTTP_PROXY", url),
        ("http_proxy", url),
        ("ALL_PROXY", url),
        ("all_proxy", url),
        ("NO_PROXY", loopback),
        ("no_proxy", loopback),
    ];
    assert_eq!(vars.len(), expected.len());
    for (name, value) in expected {
        let found = vars
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str());
        assert_eq!(found, Some(value), "{name}");
    }
}

#[test]
fn listener_forwards_both_ways() {
    let dir = TempDir::new().expect("tempdir");
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).expect("chmod");
    let socket = dir.path().join("proxy.sock");
    require_unix_socket_and_loopback!("listener_forwards_both_ways", &socket);
    let upstream = UnixListener::bind(&socket).expect("bind unix echo");
    thread::spawn(move || {
        if let Ok((mut stream, _)) = upstream.accept() {
            let mut buf = [0u8; 256];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 || stream.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
        }
    });
    let port = listen(&socket).expect("listen");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect bridge");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    stream.write_all(b"round trip").expect("send");
    let mut echoed = [0u8; 10];
    stream.read_exact(&mut echoed).expect("echo");
    assert_eq!(&echoed, b"round trip");
}

#[test]
#[serial_test::serial]
fn run_exports_proxy_and_returns_status() {
    if skip_without_loopback("run_exports_proxy_and_returns_status") {
        return;
    }
    let args: Vec<OsString> = [
        "-c",
        r#"case "$HTTPS_PROXY" in http://127.0.0.1:*) exit 7;; *) exit 1;; esac"#,
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    let code = run(
        Path::new("/nonexistent/proxy.sock"),
        Path::new("/bin/sh"),
        &args,
    )
    .expect("run");
    assert_eq!(code, ExitCode::from(7));
}

#[test]
#[serial_test::serial]
fn run_reports_missing_program() {
    if skip_without_loopback("run_reports_missing_program") {
        return;
    }
    let program = Path::new("/nonexistent/loom-test-program");
    let err = run(Path::new("/nonexistent/proxy.sock"), program, &[]).expect_err("missing");
    assert!(
        format!("{err:#}").contains("/nonexistent/loom-test-program"),
        "{err:#}"
    );
}
