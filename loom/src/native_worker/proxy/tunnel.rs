//! Request head parsing, fixed responses, upstream connection and byte splicing for the proxy.

use std::io::{self, ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, TcpStream, ToSocketAddrs};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use super::allow::allowed_host;
use super::{Connector, Limits};

const MAX_HEAD_BYTES: usize = 8192;
const HEAD_CHUNK_BYTES: usize = 2048;
const COPY_BUFFER_BYTES: usize = 16 * 1024;
const MAX_UPSTREAM_ATTEMPTS: usize = 4;
const MIN_READ_TIMEOUT: Duration = Duration::from_millis(1);
const UPSTREAM_PORT: u16 = 443;
const ESTABLISHED: &[u8] = b"HTTP/1.1 200 Connection Established\r\n\r\n";

/// Why a request does not become a tunnel; each maps to one fixed response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refusal {
    BadRequest,
    HeadTooLarge,
    MethodNotAllowed,
    Forbidden,
    BadGateway,
    Unavailable,
}

impl Refusal {
    pub(super) fn response(self) -> &'static [u8] {
        match self {
            Self::BadRequest => {
                b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
            }
            Self::HeadTooLarge => b"HTTP/1.1 431 Request Header Fields Too Large\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
            Self::MethodNotAllowed => b"HTTP/1.1 405 Method Not Allowed\r\nAllow: CONNECT\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
            Self::Forbidden => {
                b"HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
            }
            Self::BadGateway => {
                b"HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
            }
            Self::Unavailable => b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
        }
    }
}

/// Answers `refusal` on `client` within `timeout`; the caller closes the stream.
pub(super) fn refuse(client: &mut UnixStream, refusal: Refusal, timeout: Duration) {
    let _ = client.set_write_timeout(Some(timeout.max(MIN_READ_TIMEOUT)));
    let _ = client.write_all(refusal.response());
}

/// Serves one client connection to completion: head, allowlist, upstream, tunnel. `attach` hands
/// the opened upstream to the proxy's shutdown sweep; when it returns false the proxy is
/// stopping and the connection is dropped before the tunnel starts.
pub(super) fn serve(
    mut client: UnixStream,
    allowed: &[&str],
    limits: &Limits,
    connector: &Connector,
    attach: &dyn Fn(&TcpStream) -> bool,
) {
    match open_upstream(&mut client, allowed, limits, connector) {
        Ok((mut upstream, early)) => {
            if !attach(&upstream) {
                let _ = upstream.shutdown(Shutdown::Both);
                return;
            }
            let _ = upstream.set_write_timeout(Some(limits.idle_timeout));
            if client.write_all(ESTABLISHED).is_ok() && upstream.write_all(&early).is_ok() {
                splice(client, upstream, Some(limits.idle_timeout));
            } else {
                let _ = upstream.shutdown(Shutdown::Both);
            }
        }
        Err(refusal) => refuse(&mut client, refusal, limits.head_timeout),
    }
}

/// The allowlist check runs before any resolution or connection attempt.
fn open_upstream(
    client: &mut UnixStream,
    allowed: &[&str],
    limits: &Limits,
    connector: &Connector,
) -> Result<(TcpStream, Vec<u8>), Refusal> {
    let head = read_head(client, limits.head_timeout)?;
    let host = allowed_host(&head.target, allowed).ok_or(Refusal::Forbidden)?;
    let upstream = connector(&host, UPSTREAM_PORT).map_err(|_| Refusal::BadGateway)?;
    Ok((upstream, head.early))
}

/// A parsed request head: the CONNECT target and any bytes the client sent past the blank line.
struct Head {
    target: String,
    early: Vec<u8>,
}

fn read_head(stream: &mut UnixStream, timeout: Duration) -> Result<Head, Refusal> {
    let deadline = Instant::now() + timeout;
    let mut buf = Vec::with_capacity(HEAD_CHUNK_BYTES);
    let mut chunk = [0u8; HEAD_CHUNK_BYTES];
    let mut scanned = 0;
    loop {
        if let Some(end) = find_blank_line(&buf, scanned) {
            if end > MAX_HEAD_BYTES {
                return Err(Refusal::HeadTooLarge);
            }
            let early = buf.split_off(end);
            return parse_request_line(&buf).map(|target| Head { target, early });
        }
        if buf.len() >= MAX_HEAD_BYTES {
            return Err(Refusal::HeadTooLarge);
        }
        scanned = buf.len().saturating_sub(3);
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Refusal::BadRequest);
        }
        stream
            .set_read_timeout(Some(remaining.max(MIN_READ_TIMEOUT)))
            .map_err(|_| Refusal::BadRequest)?;
        match stream.read(&mut chunk) {
            Ok(0) => return Err(Refusal::BadRequest),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => return Err(Refusal::BadRequest),
        }
    }
}

/// Offset just past the first `\r\n\r\n` at or after `from`.
fn find_blank_line(buf: &[u8], from: usize) -> Option<usize> {
    buf.get(from..)?
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| from + p + 4)
}

fn parse_request_line(head: &[u8]) -> Result<String, Refusal> {
    let line_end = head
        .windows(2)
        .position(|w| w == b"\r\n")
        .unwrap_or(head.len());
    let line = std::str::from_utf8(&head[..line_end]).map_err(|_| Refusal::BadRequest)?;
    let tokens: Vec<&str> = line.split(' ').collect();
    let [method, target, version] = tokens.as_slice() else {
        return Err(Refusal::BadRequest);
    };
    if method.is_empty() || target.is_empty() || !matches!(*version, "HTTP/1.1" | "HTTP/1.0") {
        return Err(Refusal::BadRequest);
    }
    if *method != "CONNECT" {
        return Err(Refusal::MethodNotAllowed);
    }
    Ok((*target).to_string())
}

/// Resolves `host` and connects to the first public address, trying at most four. The address
/// connected to is the one checked, so a name that later resolves elsewhere cannot slip past.
/// Resolution itself is bounded by the system resolver's timeouts, not by `timeout`.
pub(super) fn connect_public(host: &str, port: u16, timeout: Duration) -> io::Result<TcpStream> {
    let mut last = io::Error::new(
        ErrorKind::AddrNotAvailable,
        format!("{host} has no public address"),
    );
    let candidates = (host, port)
        .to_socket_addrs()?
        .filter(|addr| public_address(addr.ip()))
        .take(MAX_UPSTREAM_ATTEMPTS);
    for addr in candidates {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => return Ok(stream),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// False for every address that is not globally routable unicast, so an allowed name that
/// resolves to an internal address (DNS rebinding) is never connected to.
pub(super) fn public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => public_v6(v6),
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    let reserved = a == 0                       // this network
        || (a == 100 && (b & 0xc0) == 64)       // shared address space 100.64.0.0/10
        || (a == 192 && b == 0 && c == 0)       // IETF protocol assignments
        || (a == 192 && b == 88 && c == 99)     // 6to4 relay anycast 192.88.99.0/24
        || (a == 198 && (b & 0xfe) == 18)       // benchmarking 198.18.0.0/15
        || a >= 240; // reserved and broadcast
    !(reserved
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_documentation())
}

fn public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return public_v4(v4);
    }
    let s = ip.segments();
    if s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0; 4] {
        return public_v4(embedded_v4(s[6], s[7])); // NAT64 64:ff9b::/96
    }
    if s[0] == 0x2002 {
        return public_v4(embedded_v4(s[1], s[2])); // 6to4 2002::/16
    }
    !(s[..6] == [0; 6]                          // ::, ::1 and IPv4-compatible ::/96
        || ip.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00            // unique local fc00::/7
        || (s[0] & 0xffc0) == 0xfe80            // link local fe80::/10
        || (s[0] & 0xffc0) == 0xfec0            // site local fec0::/10
        || (s[..4] == [0; 4] && s[4] == 0xffff && s[5] == 0) // SIIT ::ffff:0:0:0/96
        || (s[0] == 0x2001 && s[1] == 0)        // Teredo 2001::/32
        || (s[0] == 0x0100 && s[1..4] == [0; 3]) // discard-only 100::/64
        || (s[0] == 0x2001 && s[1] == 0x0db8)) // documentation 2001:db8::/32
}

fn embedded_v4(high: u16, low: u16) -> Ipv4Addr {
    Ipv4Addr::from((u32::from(high) << 16) | u32::from(low))
}

/// A stream the tunnel can copy through: duplicable, closable, with read and write timeouts.
pub(crate) trait Socket: Read + Write + Send + Sized + 'static {
    fn duplicate(&self) -> io::Result<Self>;
    fn set_timeouts(&self, timeout: Option<Duration>) -> io::Result<()>;
    fn close(&self);
    /// Ends this side's writes only, so the peer reads end of stream while replies still flow.
    fn close_write(&self);
}

impl Socket for TcpStream {
    fn duplicate(&self) -> io::Result<Self> {
        self.try_clone()
    }
    fn set_timeouts(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.set_read_timeout(timeout)?;
        self.set_write_timeout(timeout)
    }
    fn close(&self) {
        let _ = self.shutdown(Shutdown::Both);
    }
    fn close_write(&self) {
        let _ = self.shutdown(Shutdown::Write);
    }
}

impl Socket for UnixStream {
    fn duplicate(&self) -> io::Result<Self> {
        self.try_clone()
    }
    fn set_timeouts(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.set_read_timeout(timeout)?;
        self.set_write_timeout(timeout)
    }
    fn close(&self) {
        let _ = self.shutdown(Shutdown::Both);
    }
    fn close_write(&self) {
        let _ = self.shutdown(Shutdown::Write);
    }
}

/// Copies `a` to `b` on this thread and `b` to `a` on a helper thread. `a` is the side that
/// opened the connection: when it ends its writes cleanly (a half-close, as `nc` does at the end
/// of its input) the end is passed on to `b` and `b`'s reply keeps flowing to `a`. Any other end
/// of `a`, and any end of `b`, shuts both sockets down so the other direction ends too. A
/// half-closed tunnel waits on `b`, which bounds it: the proxy's client side drops the stream
/// after its head timeout or a refusal, and its upstream side runs with `idle` set. With `idle`
/// set, the tunnel closes once neither direction has moved a byte for that long.
pub(crate) fn splice<A: Socket, B: Socket>(mut a: A, mut b: B, idle: Option<Duration>) {
    let activity = Arc::new(Activity::new());
    let reverse = a
        .set_timeouts(idle)
        .and_then(|()| b.set_timeouts(idle))
        .and_then(|()| Ok((a.duplicate()?, b.duplicate()?)))
        .and_then(|(mut a2, mut b2)| {
            let activity = Arc::clone(&activity);
            thread::Builder::new()
                .name("tunnel-reverse".to_string())
                .spawn(move || {
                    copy_until_closed(&mut b2, &mut a2, idle, &activity);
                    a2.close();
                    b2.close();
                })
        });
    if reverse.is_ok() && copy_until_closed(&mut a, &mut b, idle, &activity) == End::Finished {
        b.close_write();
    } else {
        a.close();
        b.close();
    }
    if let Ok(handle) = reverse {
        let _ = handle.join();
    }
    a.close();
    b.close();
}

/// How one copy direction ended.
#[derive(Debug, PartialEq, Eq)]
enum End {
    /// The source reached end of stream.
    Finished,
    /// A read or write failed, or the tunnel sat idle past its limit.
    Broken,
}

fn copy_until_closed<R: Read, W: Write>(
    from: &mut R,
    to: &mut W,
    idle: Option<Duration>,
    activity: &Activity,
) -> End {
    let mut buf = vec![0u8; COPY_BUFFER_BYTES];
    loop {
        match from.read(&mut buf) {
            Ok(0) => return End::Finished,
            Ok(n) => {
                if to.write_all(&buf[..n]).is_err() {
                    return End::Broken;
                }
                activity.touch();
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e)
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
                    && idle.is_some_and(|limit| activity.idle_for() < limit) => {}
            Err(_) => return End::Broken,
        }
    }
}

/// Time of the last byte moved in either direction, shared by both copy loops.
struct Activity {
    base: Instant,
    last_millis: AtomicU64,
}

impl Activity {
    fn new() -> Self {
        Self {
            base: Instant::now(),
            last_millis: AtomicU64::new(0),
        }
    }
    fn touch(&self) {
        let now = u64::try_from(self.base.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.last_millis.store(now, Ordering::Relaxed);
    }
    fn idle_for(&self) -> Duration {
        let last = Duration::from_millis(self.last_millis.load(Ordering::Relaxed));
        self.base.elapsed().saturating_sub(last)
    }
}
