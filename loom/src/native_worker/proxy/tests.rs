use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

use super::allow::connect_target_allowed;
use super::tunnel::public_address;
use super::{start, start_with, Connector, Limits, ProxyHandle};

const ALLOWED: [&str; 3] = ["chatgpt.com", "*.chatgpt.com", "api.openai.com"];
const IO_TIMEOUT: Duration = Duration::from_secs(5);

fn allowed(target: &str) -> bool {
    connect_target_allowed(target, &ALLOWED)
}

#[test]
fn allows_exact_host() {
    assert!(allowed("api.openai.com:443"));
    assert!(allowed("chatgpt.com:443"));
}

#[test]
fn allows_wildcard_subdomain() {
    assert!(allowed("ab.chatgpt.com:443"));
    assert!(allowed("a.b.chatgpt.com:443"));
}

#[test]
fn normalizes_case_and_trailing_dot() {
    assert!(allowed("API.OpenAI.com.:443"));
    assert!(!allowed("api.openai.com..:443"));
    assert!(connect_target_allowed(
        "x.chatgpt.com:443",
        &["*.ChatGPT.com"]
    ));
}

#[test]
fn refuses_suffix_lookalike() {
    assert!(!allowed("chatgpt.com.evil.net:443"));
    assert!(!allowed("api.openai.com.evil.net:443"));
}

#[test]
fn refuses_prefix_lookalike() {
    assert!(!allowed("evilchatgpt.com:443"));
    assert!(!allowed("x.evilchatgpt.com:443"));
    assert!(!allowed("xapi.openai.com:443"));
}

#[test]
fn wildcard_does_not_match_apex() {
    assert!(!connect_target_allowed(
        "chatgpt.com:443",
        &["*.chatgpt.com"]
    ));
}

#[test]
fn wildcard_needs_a_label() {
    assert!(!allowed(".chatgpt.com:443"));
    assert!(!connect_target_allowed("x.com:443", &["*."]));
}

#[test]
fn refuses_empty_label() {
    assert!(!allowed("a..chatgpt.com:443"));
}

#[test]
fn refuses_other_ports() {
    for port in ["22", "8443", "0443", "+443", "443 ", "", "4430"] {
        assert!(!allowed(&format!("api.openai.com:{port}")), "port {port:?}");
    }
}

#[test]
fn refuses_missing_port() {
    assert!(!allowed("api.openai.com"));
}

#[test]
fn refuses_ip_literals() {
    assert!(!allowed("127.0.0.1:443"));
    assert!(!allowed("[::1]:443"));
    assert!(!connect_target_allowed("1.2.3.4:443", &["1.2.3.4"]));
    assert!(!connect_target_allowed("[::1]:443", &["[::1]"]));
}

#[test]
fn refuses_userinfo_trick() {
    assert!(!allowed("api.openai.com:443@evil.net:443"));
    assert!(!allowed("evil.net@api.openai.com:443"));
}

#[test]
fn refuses_whitespace_and_control_chars() {
    for host in [
        "api.openai.com\t",
        " api.openai.com",
        "api.openai.com\0",
        "api.openai.com\r\n",
        "api.openai.\u{212A}om",
        "api_openai.com",
        "api.openai.com/x",
    ] {
        assert!(!allowed(&format!("{host}:443")), "host {host:?}");
    }
}

#[test]
fn public_address_table() {
    for ip in [
        "127.0.0.1",
        "10.1.1.1",
        "192.168.0.1",
        "172.16.0.1",
        "169.254.169.254",
        "100.64.0.1",
        "0.0.0.0",
        "224.0.0.1",
        "255.255.255.255",
        "::1",
        "::",
        "fc00::1",
        "fe80::1",
        "::ffff:127.0.0.1",
        "64:ff9b::a00:1",
        "2002:7f00:1::",
        "192.88.99.1",
        "::ffff:0:808:808",
        "2001:0:4136:e378:8000:63bf:3fff:fdd2",
        "100::1",
    ] {
        let parsed: IpAddr = ip.parse().expect("test address");
        assert!(!public_address(parsed), "{ip} must not be public");
    }
    for ip in ["8.8.8.8", "2606:4700::1111", "::ffff:8.8.8.8"] {
        let parsed: IpAddr = ip.parse().expect("test address");
        assert!(public_address(parsed), "{ip} must be public");
    }
}

/// The production socket directory, holding the socket path.
fn socket_dir() -> (TempDir, PathBuf) {
    let dir = super::socket_dir().expect("socket dir");
    let path = dir.path().join("proxy.sock");
    (dir, path)
}

#[test]
fn socket_dir_is_private_under_any_umask() {
    // Fails on any umask other than 077 if the mode is left to tempfile's default.
    let (dir, _) = socket_dir();
    let mode = fs::metadata(dir.path()).expect("stat").permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "{:o}", mode);
}

/// A one-connection echo server on loopback.
fn echo_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind echo");
    let addr = listener.local_addr().expect("echo addr");
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 || stream.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
        }
    });
    addr
}

/// Maps `api.openai.com` to `target` and counts every call.
fn connector(target: SocketAddr, calls: &Arc<AtomicUsize>) -> Connector {
    let calls = Arc::clone(calls);
    Arc::new(move |host, port| {
        calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(port, 443);
        if host == "api.openai.com" {
            TcpStream::connect(target)
        } else {
            Err(std::io::Error::new(ErrorKind::NotFound, "unmapped"))
        }
    })
}

fn proxy(path: &Path, limits: Limits) -> (ProxyHandle, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let handle =
        start_with(path, &ALLOWED, limits, connector(echo_server(), &calls)).expect("start");
    (handle, calls)
}

fn client(path: &Path) -> UnixStream {
    let stream = UnixStream::connect(path).expect("connect proxy");
    stream.set_read_timeout(Some(IO_TIMEOUT)).expect("timeout");
    stream
}

/// Sends `request` and reads the whole reply until the proxy closes.
fn exchange(path: &Path, request: &[u8]) -> String {
    let mut stream = client(path);
    stream.write_all(request).expect("send");
    let mut reply = Vec::new();
    let _ = stream.read_to_end(&mut reply);
    String::from_utf8_lossy(&reply).into_owned()
}

/// Reads exactly the 200 reply, then proves the tunnel echoes.
fn open_tunnel(path: &Path) -> UnixStream {
    let mut stream = client(path);
    stream
        .write_all(b"CONNECT api.openai.com:443 HTTP/1.1\r\nHost: x\r\n\r\nhello")
        .expect("send");
    let expected = b"HTTP/1.1 200 Connection Established\r\n\r\nhello";
    let mut reply = vec![0u8; expected.len()];
    stream.read_exact(&mut reply).expect("reply");
    assert_eq!(reply, expected);
    stream
}

#[test]
fn allowed_connect_tunnels_bytes() {
    let (_dir, path) = socket_dir();
    let (_handle, calls) = proxy(&path, Limits::default());
    let mut stream = open_tunnel(&path);
    stream.write_all(b" world").expect("send");
    let mut echoed = [0u8; 6];
    stream.read_exact(&mut echoed).expect("echo");
    assert_eq!(&echoed, b" world");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn refused_host_gets_403_and_never_connects() {
    let (_dir, path) = socket_dir();
    let (_handle, calls) = proxy(&path, Limits::default());
    for target in ["evil.net:443", "api.openai.com:80", "127.0.0.1:443"] {
        let reply = exchange(
            &path,
            format!("CONNECT {target} HTTP/1.1\r\n\r\n").as_bytes(),
        );
        assert!(reply.starts_with("HTTP/1.1 403 Forbidden\r\n"), "{reply}");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn get_request_gets_405() {
    let (_dir, path) = socket_dir();
    let (_handle, calls) = proxy(&path, Limits::default());
    let reply = exchange(&path, b"GET http://api.openai.com/ HTTP/1.1\r\n\r\n");
    assert!(
        reply.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"),
        "{reply}"
    );
    assert!(reply.contains("Allow: CONNECT\r\n"), "{reply}");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn oversized_head_gets_431() {
    let (_dir, path) = socket_dir();
    let (_handle, _calls) = proxy(&path, Limits::default());
    let reply = exchange(&path, &[b'a'; 9000]);
    assert!(reply.starts_with("HTTP/1.1 431 "), "{reply}");
}

#[test]
fn malformed_request_line_gets_400() {
    let (_dir, path) = socket_dir();
    let (_handle, calls) = proxy(&path, Limits::default());
    for request in [
        "CONNECT api.openai.com:443\r\n\r\n",
        "CONNECT  api.openai.com:443 HTTP/1.1\r\n\r\n",
        "CONNECT api.openai.com:443 HTTP/2\r\n\r\n",
        "\r\n\r\n",
    ] {
        let reply = exchange(&path, request.as_bytes());
        assert!(
            reply.starts_with("HTTP/1.1 400 Bad Request\r\n"),
            "{request:?}: {reply}"
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn stalled_head_gets_400() {
    let (_dir, path) = socket_dir();
    let limits = Limits {
        head_timeout: Duration::from_millis(200),
        ..Limits::default()
    };
    let (_handle, _calls) = proxy(&path, limits);
    let started = Instant::now();
    let reply = exchange(&path, b"CONNECT api.openai.com:443 HTTP/1.1\r\n");
    assert!(reply.starts_with("HTTP/1.1 400 Bad Request\r\n"), "{reply}");
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn cap_gets_503() {
    let (_dir, path) = socket_dir();
    let limits = Limits {
        max_connections: 1,
        ..Limits::default()
    };
    let (handle, _calls) = proxy(&path, limits);
    let _held = client(&path);
    let deadline = Instant::now() + IO_TIMEOUT;
    while handle.shared.active.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < deadline, "held connection never admitted");
        thread::sleep(Duration::from_millis(10));
    }
    let reply = exchange(&path, b"CONNECT api.openai.com:443 HTTP/1.1\r\n\r\n");
    assert!(
        reply.starts_with("HTTP/1.1 503 Service Unavailable\r\n"),
        "{reply}"
    );
}

#[test]
fn localhost_resolves_to_502() {
    let (_dir, path) = socket_dir();
    let _handle = start(&path, &["localhost"]).expect("start");
    let reply = exchange(&path, b"CONNECT localhost:443 HTTP/1.1\r\n\r\n");
    assert!(reply.starts_with("HTTP/1.1 502 Bad Gateway\r\n"), "{reply}");
}

#[test]
fn drop_removes_socket_and_joins_threads() {
    let (_dir, path) = socket_dir();
    let (handle, _calls) = proxy(&path, Limits::default());
    let mut tunnel = open_tunnel(&path);
    drop(handle);
    assert!(!path.exists());
    assert!(UnixStream::connect(&path).is_err());
    let mut buf = [0u8; 8];
    match tunnel.read(&mut buf) {
        Ok(n) => assert_eq!(n, 0, "tunnel still open"),
        Err(e) => assert!(
            !matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut),
            "tunnel still open: {e}"
        ),
    }
}

#[test]
fn start_refuses_open_directory() {
    let (dir, path) = socket_dir();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).expect("chmod");
    let err = start(&path, &ALLOWED).err().expect("refused");
    assert!(err.to_string().contains("mode 755"), "{err:#}");
    assert!(!path.exists());
}

#[test]
fn start_refuses_long_path() {
    let (dir, _) = socket_dir();
    let path = dir.path().join("s".repeat(100));
    let err = start(&path, &ALLOWED).err().expect("refused");
    assert!(err.to_string().contains("107"), "{err:#}");
}

#[test]
fn start_refuses_existing_path() {
    let (_dir, path) = socket_dir();
    fs::write(&path, b"").expect("create");
    let err = start(&path, &ALLOWED).err().expect("refused");
    assert!(err.to_string().contains("already exists"), "{err:#}");
}
