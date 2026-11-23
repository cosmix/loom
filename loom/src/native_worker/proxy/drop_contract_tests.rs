//! Contract of stage runtime-and-dashboard-server: dropping the proxy handle
//! closes the upstream side of every tunnel instead of waiting for it.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::process::sandbox_probe::require_unix_socket_and_loopback;

use super::{socket_dir, start_with, Connector, Limits, SOCKET_NAME};

const ESTABLISHED: &[u8] = b"HTTP/1.1 200 Connection Established\r\n\r\n";

/// A loopback server that accepts one connection, reads it to end of stream,
/// then holds it open until `release` is sent or dropped.
fn silent_upstream() -> (Connector, mpsc::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind upstream");
    let target = listener.local_addr().expect("upstream addr");
    let (release, released) = mpsc::channel::<()>();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut sink = Vec::new();
            let _ = stream.read_to_end(&mut sink);
            let _ = released.recv();
            drop(stream);
        }
    });
    let connector: Connector = Arc::new(move |host, _port| {
        if host == "api.openai.com" {
            TcpStream::connect(target)
        } else {
            Err(std::io::Error::new(ErrorKind::NotFound, "unmapped"))
        }
    });
    (connector, release)
}

#[test]
fn dropping_the_handle_returns_while_the_upstream_stays_open() {
    let dir = socket_dir().expect("socket dir");
    let path = dir.path().join(SOCKET_NAME);
    require_unix_socket_and_loopback!(
        "dropping_the_handle_returns_while_the_upstream_stays_open",
        &path
    );
    let (connector, release) = silent_upstream();
    let limits = Limits {
        idle_timeout: Duration::from_secs(30),
        ..Limits::default()
    };
    let handle = start_with(&path, &["api.openai.com"], limits, connector).expect("start");
    let mut tunnel = UnixStream::connect(&path).expect("connect proxy");
    tunnel
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    tunnel
        .write_all(b"CONNECT api.openai.com:443 HTTP/1.1\r\nHost: x\r\n\r\n")
        .expect("send CONNECT");
    let mut reply = vec![0u8; ESTABLISHED.len()];
    tunnel.read_exact(&mut reply).expect("read 200 head");
    assert_eq!(reply, ESTABLISHED);
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        drop(handle);
        let _ = done.send(());
    });
    let outcome = finished.recv_timeout(Duration::from_secs(5));
    drop(release);
    drop(tunnel);
    assert!(
        outcome.is_ok(),
        "dropping the ProxyHandle blocked past 5 s on a silent upstream"
    );
}
