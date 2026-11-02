//! Real HTTP/config writes with deterministic pauses at mutation admission.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::web_server::access::AccessPolicy;
use crate::web_server::http::RequestHead;
use crate::web_server::limits::Limits;
use crate::web_server::{self, ConnInfo, Router, ServeConfig, ShutdownPending};

const WAIT: Duration = Duration::from_secs(8);
const BODY: &str = r#"{"scope":"project","name":"terminal.backend","value":"tmux"}"#;

struct PausedConfig {
    base: PathBuf,
    entered: mpsc::Sender<()>,
    admitted: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl Router for PausedConfig {
    fn upgrade(&self, stream: TcpStream, _: &RequestHead, _: &ConnInfo<'_>) -> Option<TcpStream> {
        Some(stream)
    }

    fn handle(
        &self,
        stream: &mut TcpStream,
        head: &RequestHead,
        prefix: Vec<u8>,
        conn: &ConnInfo<'_>,
    ) -> io::Result<()> {
        self.entered.send(()).unwrap();
        super::handle_post_with_update(stream, head, prefix, &self.base, conn, |base, body| {
            self.admitted.send(()).unwrap();
            self.release.lock().unwrap().recv_timeout(WAIT).unwrap();
            super::super::update(base, body)
        });
        Ok(())
    }
}

struct Server {
    _temp: tempfile::TempDir,
    base: PathBuf,
    local: SocketAddr,
    running: Arc<AtomicBool>,
    limits: Arc<Limits>,
    entered: mpsc::Receiver<()>,
    admitted: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
    done: mpsc::Receiver<anyhow::Result<()>>,
}

impl Server {
    fn start() -> Self {
        let (temp, base) = crate::commands::status::web::tests::workspace();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let local = listener.local_addr().unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let limits = Limits::new(1024);
        let (entered_tx, entered) = mpsc::channel();
        let (admitted_tx, admitted) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let router = Arc::new(PausedConfig {
            base: base.clone(),
            entered: entered_tx,
            admitted: admitted_tx,
            release: Mutex::new(release_rx),
        });
        let config = ServeConfig {
            policy: AccessPolicy::new(local, None),
            limits: Arc::clone(&limits),
            thread_name: "config-lifecycle-test",
        };
        let (done_tx, done) = mpsc::channel();
        let serving = Arc::clone(&running);
        thread::spawn(move || {
            done_tx
                .send(web_server::serve(listener, router, serving, config))
                .unwrap();
        });
        Self {
            _temp: temp,
            base,
            local,
            running,
            limits,
            entered,
            admitted,
            release,
            done,
        }
    }

    fn request(&self, prefix: &str) -> TcpStream {
        let mut client = TcpStream::connect(self.local).unwrap();
        client.set_read_timeout(Some(WAIT)).unwrap();
        write!(
            client,
            "POST /api/config HTTP/1.1\r\nHost: {}\r\nOrigin: http://{}\r\nContent-Type: application/json\r\nX-Loom-Csrf: {}\r\nContent-Length: {}\r\n\r\n{}",
            self.local, self.local, super::super::test_token(), BODY.len(), prefix
        ).unwrap();
        self.entered.recv_timeout(WAIT).unwrap();
        client
    }

    fn stop(&self) -> anyhow::Result<()> {
        self.running.store(false, Ordering::SeqCst);
        self.done.recv_timeout(WAIT).expect("bounded server stop")
    }

    fn bytes(&self) -> Vec<u8> {
        std::fs::read(self.base.join(".loom/work/config.toml")).unwrap_or_default()
    }
}

fn loopback_unavailable(name: &str) -> bool {
    crate::commands::status::web::tests::skip_without_loopback(name)
}

fn successful_response(client: &mut TcpStream) {
    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
}

#[test]
fn admitted_update_retains_ownership_past_bounded_stop() {
    if loopback_unavailable("admitted_update_retains_ownership_past_bounded_stop") {
        return;
    }
    let server = Server::start();
    let before = server.bytes();
    let mut client = server.request(BODY);
    server.admitted.recv_timeout(WAIT).unwrap();
    let error = server.stop().unwrap_err();
    let pending = error.downcast_ref::<ShutdownPending>().unwrap();
    assert!(!pending.wait(Duration::from_millis(20)));
    assert_eq!(server.limits.connection_count(), 1);
    assert_eq!(server.bytes(), before);
    assert!(TcpListener::bind(server.local).is_err());
    server.release.send(()).unwrap();
    successful_response(&mut client);
    assert!(pending.wait(WAIT));
    assert_ne!(server.bytes(), before);
    assert_eq!(server.limits.connection_count(), 0);
    TcpListener::bind(server.local).expect("restart after shutdown completion");
}

#[test]
fn dropping_pending_error_does_not_release_an_admitted_writer() {
    if loopback_unavailable("dropping_pending_error_does_not_release_an_admitted_writer") {
        return;
    }
    let server = Server::start();
    let mut client = server.request(BODY);
    server.admitted.recv_timeout(WAIT).unwrap();
    let error = server.stop().unwrap_err();
    assert!(error.is::<ShutdownPending>());
    drop(error);
    assert!(TcpListener::bind(server.local).is_err());
    assert_eq!(server.limits.connection_count(), 1);
    server.release.send(()).unwrap();
    successful_response(&mut client);
    let expected = b"backend = \"tmux\"";
    assert!(server
        .bytes()
        .windows(expected.len())
        .any(|part| part == expected));
}

#[test]
fn incomplete_body_cannot_publish_after_completed_shutdown() {
    if loopback_unavailable("incomplete_body_cannot_publish_after_completed_shutdown") {
        return;
    }
    let server = Server::start();
    let before = server.bytes();
    let mut client = server.request(&BODY[..1]);
    server.stop().unwrap();
    // The body arrives only after the server has reported a completed drain.
    let _ = client.write_all(&BODY.as_bytes()[1..]);
    assert!(server.admitted.try_recv().is_err());
    assert_eq!(server.limits.connection_count(), 0);
    assert_eq!(server.bytes(), before);
}

#[test]
fn completed_update_is_preserved_by_successful_shutdown() {
    if loopback_unavailable("completed_update_is_preserved_by_successful_shutdown") {
        return;
    }
    let server = Server::start();
    let mut client = server.request(BODY);
    server.admitted.recv_timeout(WAIT).unwrap();
    server.release.send(()).unwrap();
    successful_response(&mut client);
    let committed = server.bytes();
    server.stop().unwrap();
    assert_eq!(server.bytes(), committed);
    assert_eq!(server.limits.connection_count(), 0);
    TcpListener::bind(server.local).expect("completed stop releases the address");
}
