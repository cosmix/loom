//! Contracts for the extracted `loom::web_server` layer: the accept loop takes
//! an injected router, checks `Host` before the router sees a request, and the
//! dashboard's startup lines stay byte-identical.

use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use loom::commands::status::web::startup_lines;
use loom::web_server::access::AccessPolicy;
use loom::web_server::http::{write_response, RequestHead};
use loom::web_server::limits::Limits;
use loom::web_server::{serve, ConnInfo, Router, ServeConfig};

/// Answers `200 router-ok` for `GET /probe`, 404 otherwise, counting calls.
struct ProbeRouter {
    calls: Arc<AtomicUsize>,
}

impl Router for ProbeRouter {
    fn upgrade(
        &self,
        stream: TcpStream,
        _peeked: &RequestHead,
        _conn: &ConnInfo<'_>,
    ) -> Option<TcpStream> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Some(stream)
    }

    fn handle(
        &self,
        stream: &mut TcpStream,
        head: &RequestHead,
        _body_prefix: Vec<u8>,
        _conn: &ConnInfo<'_>,
    ) -> io::Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if head.method == "GET" && head.path == "/probe" {
            write_response(stream, 200, "OK", "text/plain", b"router-ok", true)
        } else {
            write_response(stream, 404, "Not Found", "text/plain", b"missing", true)
        }
    }
}

struct Running {
    addr: SocketAddr,
    running: Arc<AtomicBool>,
    done: mpsc::Receiver<anyhow::Result<()>>,
}

fn start(calls: Arc<AtomicUsize>) -> Running {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let running = Arc::new(AtomicBool::new(true));
    let config = ServeConfig {
        policy: AccessPolicy::new(addr, None),
        limits: Limits::new(1024 * 1024),
        thread_name: "contract-web",
    };
    let router: Arc<dyn Router> = Arc::new(ProbeRouter { calls });
    let flag = Arc::clone(&running);
    let (tx, done) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(serve(listener, router, flag, config));
    });
    Running {
        addr,
        running,
        done,
    }
}

fn stop(server: Running) {
    server.running.store(false, Ordering::SeqCst);
    let result = server
        .done
        .recv_timeout(Duration::from_secs(15))
        .expect("serve must return after running is cleared");
    result.expect("serve returns Ok on a clean stop");
}

fn get(addr: SocketAddr, host: &str) -> String {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout");
    let request = format!("GET /probe HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("send request");
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    response
}

#[test]
fn serve_dispatches_to_custom_router() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = start(Arc::clone(&calls));
    let response = get(server.addr, &format!("127.0.0.1:{}", server.addr.port()));
    stop(server);
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "the injected router must answer GET /probe, got: {response}"
    );
    assert!(
        response.ends_with("router-ok"),
        "body must come from the injected router, got: {response}"
    );
    assert!(calls.load(Ordering::SeqCst) >= 1, "router was never called");
}

#[test]
fn bad_host_never_reaches_router() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = start(Arc::clone(&calls));
    let response = get(server.addr, "evil.example:80");
    stop(server);
    assert!(
        response.starts_with("HTTP/1.1 403"),
        "a foreign Host must be refused with 403 by serve, got: {response}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "the router must not see a request whose Host failed the check"
    );
}

#[test]
fn dashboard_startup_lines_are_unchanged() {
    let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7373);
    let token = "0123456789abcdef".repeat(4);
    assert_eq!(token.len(), 64);
    assert_eq!(
        startup_lines(local, false, false, None, &[]),
        vec!["loom dashboard: http://127.0.0.1:7373/  (Ctrl-C to stop)".to_string()]
    );
    assert_eq!(
        startup_lines(local, false, true, Some(&token), &[]),
        vec![format!(
            "loom dashboard: http://127.0.0.1:7373/?token={token}  (terminals enabled; Ctrl-C to stop)"
        )]
    );
}
