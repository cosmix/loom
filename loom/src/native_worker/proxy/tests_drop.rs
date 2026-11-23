//! Shutdown of open tunnels when the proxy handle is dropped.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::net::UnixStream;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use crate::process::sandbox_probe::require_unix_socket_and_loopback;

use super::{socket_dir, start_with, Connector, Limits, Registration, SOCKET_NAME};

const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Accepts one connection, reads it to end of stream, then holds it until the channel fires or
/// is dropped.
fn silent_server() -> (SocketAddr, mpsc::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind upstream");
    let addr = listener.local_addr().expect("upstream address");
    let (release, released) = mpsc::channel::<()>();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut sink = Vec::new();
            let _ = stream.read_to_end(&mut sink);
            let _ = released.recv();
        }
    });
    (addr, release)
}

fn connector_to(addr: SocketAddr) -> Connector {
    Arc::new(move |host, _port| {
        if host == "api.openai.com" {
            TcpStream::connect(addr)
        } else {
            Err(std::io::Error::other("unmapped host"))
        }
    })
}

#[test]
fn dropping_the_handle_returns_while_the_upstream_stays_open() {
    let dir = socket_dir().expect("socket dir");
    let path = dir.path().join(SOCKET_NAME);
    require_unix_socket_and_loopback!(
        "dropping_the_handle_returns_while_the_upstream_stays_open",
        &path
    );
    let (addr, release) = silent_server();
    let limits = Limits {
        idle_timeout: Duration::from_secs(30),
        ..Limits::default()
    };
    let handle = start_with(&path, &["api.openai.com"], limits, connector_to(addr)).expect("start");
    let mut client = UnixStream::connect(&path).expect("connect proxy");
    client.set_read_timeout(Some(IO_TIMEOUT)).expect("timeout");
    client
        .write_all(b"CONNECT api.openai.com:443 HTTP/1.1\r\nHost: x\r\n\r\n")
        .expect("send head");
    let mut head = [0u8; 39];
    client.read_exact(&mut head).expect("read the 200 head");
    assert!(head.starts_with(b"HTTP/1.1 200"));

    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        drop(handle);
        let _ = done.send(());
    });
    let returned = finished.recv_timeout(IO_TIMEOUT);
    drop(release);
    assert!(returned.is_ok(), "drop blocked on the open upstream");
}

#[test]
fn an_upstream_is_refused_once_the_proxy_is_stopping() {
    let dir = socket_dir().expect("socket dir");
    let path = dir.path().join(SOCKET_NAME);
    require_unix_socket_and_loopback!("an_upstream_is_refused_once_the_proxy_is_stopping", &path);
    let (addr, release) = silent_server();
    let handle = start_with(
        &path,
        &["api.openai.com"],
        Limits::default(),
        connector_to(addr),
    )
    .expect("start");
    let (client, _peer) = UnixStream::pair().expect("pair");
    let registration = Registration::new(&handle.shared, u64::MAX, client);
    let upstream = TcpStream::connect(addr).expect("connect upstream");
    assert!(registration.attach_upstream(&upstream));
    handle.shared.stop.store(true, Ordering::SeqCst);
    assert!(!registration.attach_upstream(&upstream));
    drop(release);
}
