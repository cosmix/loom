use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;

pub(super) struct MockServer {
    address: SocketAddr,
    body: Arc<Mutex<Option<String>>>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MockServer {
    pub(super) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let body = Arc::new(Mutex::new(None));
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_body = Arc::clone(&body);
        let thread_shutdown = Arc::clone(&shutdown);
        let thread = thread::spawn(move || serve(listener, thread_body, thread_shutdown));
        Self {
            address,
            body,
            shutdown,
            thread: Some(thread),
        }
    }

    pub(super) fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    pub(super) fn respond_with(&self, body: String) {
        *self.body.lock().unwrap() = Some(body);
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(handle) = self.thread.take() {
            // A panicked server thread must not double-panic during test
            // unwind (that aborts the whole binary before it can report);
            // the test's own assertions surface the failure.
            let _ = handle.join();
        }
    }
}

fn serve(listener: TcpListener, body: Arc<Mutex<Option<String>>>, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Acquire) {
        match listener.accept() {
            // A slow or aborted client (e.g. under machine load its request
            // exceeds the 2s read timeout) must not kill the accept loop —
            // the client retries, and later requests still need serving.
            Ok((stream, _)) => {
                if let Err(error) = serve_response(stream, &body) {
                    eprintln!("adjudication mock server dropped a connection: {error}");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("test HTTP listener failed: {error}"),
        }
    }
}

fn serve_response(mut stream: TcpStream, body: &Mutex<Option<String>>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let request = read_request(&mut stream)?;
    let request = String::from_utf8_lossy(&request);
    let valid = request.starts_with("POST /v1/messages HTTP/");
    let configured = body.lock().unwrap().clone();
    let (status, response_body) = match (valid, configured) {
        (true, Some(value)) => ("200 OK", value),
        (false, _) => ("404 Not Found", "{}".to_string()),
        (true, None) => ("503 Service Unavailable", "{}".to_string()),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{response_body}",
        response_body.len()
    );
    stream.write_all(response.as_bytes())?;
    Ok(())
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut request = Vec::new();
    let mut expected_len = None;
    loop {
        let mut chunk = [0_u8; 8 * 1024];
        let bytes = stream.read(&mut chunk)?;
        if bytes == 0 {
            break;
        }
        if request.len() + bytes > MAX_REQUEST_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "test HTTP request exceeded limit",
            ));
        }
        request.extend_from_slice(&chunk[..bytes]);
        if expected_len.is_none() {
            expected_len = expected_request_len(&request);
        }
        if expected_len.is_some_and(|len| request.len() >= len) {
            break;
        }
    }
    Ok(request)
}

fn expected_request_len(request: &[u8]) -> Option<usize> {
    let header_end = request.windows(4).position(|bytes| bytes == b"\r\n\r\n")? + 4;
    let headers = std::str::from_utf8(&request[..header_end]).ok()?;
    let body_len = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    })?;
    header_end.checked_add(body_len)
}
