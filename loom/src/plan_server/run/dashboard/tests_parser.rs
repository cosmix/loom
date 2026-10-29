//! Startup-line parsing and stdout draining for the dashboard child.

use super::super::*;
use super::{fake, STARTUP, URL};
use crate::commands::status::web::startup_lines;
use std::fs;
use tempfile::TempDir;

#[test]
fn parser_matches_exact_startup_strings() {
    assert_eq!(parse_dashboard_startup(&[STARTUP]).as_deref(), Some(URL));
    assert_eq!(
        parse_dashboard_startup(&["loom dashboard: http://127.0.0.1:7373/  (Ctrl-C to stop)",])
            .as_deref(),
        Some("http://127.0.0.1:7373/")
    );
    assert_eq!(
        parse_dashboard_startup(&[
            "loom dashboard listening on 0.0.0.0:7373 (remote access enabled; terminals enabled)",
            "  bootstrap from another machine at: http://10.20.30.40:7373/?token=abc123",
            "  bootstrap from this machine at: http://127.0.0.1:7373/?token=abc123",
        ])
        .as_deref(),
        Some(URL)
    );
}

#[test]
fn parser_matches_real_print_startup_line_generator() {
    for (local, remote, terminals, addresses, expected) in [
        ("127.0.0.1:7373", false, true, vec![], URL),
        (
            "[::]:7373",
            true,
            true,
            vec!["::1".parse().unwrap()],
            "http://[::1]:7373/?token=abc123",
        ),
    ] {
        let lines = crate::commands::status::web::startup_lines(
            local.parse().unwrap(),
            remote,
            terminals,
            Some("abc123"),
            &addresses,
        );
        let borrowed: Vec<_> = lines.iter().map(String::as_str).collect();

        assert_eq!(
            parse_dashboard_startup(&borrowed).as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn stdout_keeps_draining_after_startup() {
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    let (mut writer, stdout) = UnixStream::pair().unwrap();
    let (sender, receiver) = mpsc::sync_channel(STARTUP_LINES);
    let (done, finished) = mpsc::sync_channel(1);
    let reader = thread::spawn(move || drain_stdout(stdout, sender));
    let producer = thread::spawn(move || {
        writeln!(writer, "{STARTUP}").unwrap();
        writer.write_all(&vec![b'x'; 1024 * 1024]).unwrap();
        done.send(()).unwrap();
    });
    assert_eq!(
        startup_url(&receiver, Instant::now() + Duration::from_secs(3)).unwrap(),
        URL
    );
    drop(receiver);

    finished.recv_timeout(Duration::from_secs(3)).unwrap();
    producer.join().unwrap();
    reader.join().unwrap();
}

fn remote_lines() -> Vec<String> {
    startup_lines(
        "192.168.1.5:7373".parse().unwrap(),
        true,
        true,
        Some("abc123"),
        &[],
    )
}

const REMOTE_URL: &str = "http://192.168.1.5:7373/?token=abc123";

#[test]
fn startup_url_reads_a_concrete_remote_bind_across_lines() {
    let (sender, receiver) = mpsc::sync_channel(STARTUP_LINES);
    for line in remote_lines() {
        sender.send(Ok(format!("{line}\n"))).unwrap();
    }

    assert_eq!(
        startup_url(&receiver, Instant::now() + Duration::from_secs(3)).unwrap(),
        REMOTE_URL
    );
}

#[test]
fn child_bound_to_one_remote_address_yields_its_bootstrap_url() {
    let temp = TempDir::new().unwrap();
    let printed: String = remote_lines()
        .iter()
        .map(|line| format!("printf '%s\\n' '{line}'\n"))
        .collect();
    let bin = fake(temp.path(), &format!("{printed}exec sleep 60"));

    let child = start_dashboard(
        temp.path(),
        &bin,
        "192.168.1.5".parse().unwrap(),
        true,
        Duration::from_secs(3),
    )
    .unwrap();

    assert_eq!(child.url, REMOTE_URL);
    assert_eq!(
        fs::read_to_string(temp.path().join("args")).unwrap(),
        "status --web --host 192.168.1.5 --terminals\n"
    );
}
