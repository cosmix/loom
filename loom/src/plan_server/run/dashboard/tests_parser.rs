//! Startup-line parsing and stdout draining for the dashboard child.

use super::super::*;
use super::{STARTUP, URL};

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
