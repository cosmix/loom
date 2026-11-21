//! A confined command's IP networking under Seatbelt (`NETWORK_RULES`):
//! loopback TCP and UDP work, Gradle's wildcard UDP lock socket binds, and
//! nothing reaches another host. The probe is this test binary, copied into
//! the confined tree and run as [`network_probe`]. The refusals assert
//! `EPERM` for the exact step, which only the profile produces: unconfined, a
//! connect to a documentation address times out and a datagram to one is sent.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::time::Duration;

use tempfile::TempDir;

use crate::models::stage::CommandConfinement;
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};
use crate::verify::criteria::{run_spec_with_timeout, CommandSpec, CriterionResult};

/// The file in the probe's working directory naming its cases, one per line.
const CASES_FILE: &str = "network-probe-cases";

const PROBE_TEST: &str = "verify::criteria::confine::host::seatbelt::network_tests::network_probe";

/// The confined child: runs each case of [`CASES_FILE`] and prints one
/// `<case> <step>: <outcome>` line per socket operation, stopping a case at
/// its first failure. A case is `tcp A` (listen on A, connect, accept,
/// exchange bytes), `udp A P` (bind A, send to P on the bound port, receive),
/// `connect A [PORT]` (TCP to A, port 9 by default) or `send A B` (bind B, send to A port 9).
#[test]
#[ignore = "the confined child of the network tests"]
fn network_probe() {
    let Ok(cases) = std::fs::read_to_string(CASES_FILE) else {
        return;
    };
    for case in cases.lines() {
        let words: Vec<&str> = case.split_whitespace().collect();
        let address = |at: usize| words[at].parse::<IpAddr>().expect("a numeric address");
        let mut report = |step: &str, outcome: std::io::Result<()>| {
            let text = match &outcome {
                Ok(()) => "ok".to_string(),
                Err(error) if error.raw_os_error() == Some(libc::EPERM) => "EPERM".to_string(),
                Err(error) => format!("{:?} {error}", error.kind()),
            };
            println!("{case} {step}: {text}");
            outcome.is_ok()
        };
        match words[0] {
            "tcp" => tcp_case(address(1), &mut report),
            "udp" => udp_case(address(1), address(2), &mut report),
            "connect" => {
                let port = words.get(2).map_or(9, |port| port.parse().unwrap());
                let target = SocketAddr::new(address(1), port);
                let timeout = Duration::from_secs(2);
                report(
                    "connect",
                    TcpStream::connect_timeout(&target, timeout).map(drop),
                );
            }
            "send" => send_case(address(1), address(2), &mut report),
            other => panic!("unknown probe case {other}"),
        }
    }
}

type Report<'a> = dyn FnMut(&str, std::io::Result<()>) -> bool + 'a;

fn tcp_case(address: IpAddr, report: &mut Report<'_>) {
    let listener = TcpListener::bind((address, 0));
    let Some(listener) = step(report, "listen", listener) else {
        return;
    };
    let target = listener.local_addr().unwrap();
    let Some(mut client) = step(report, "connect", TcpStream::connect(target)) else {
        return;
    };
    let Some((mut server, _)) = step(report, "accept", listener.accept()) else {
        return;
    };
    let mut got = [0u8; 2];
    let exchanged = client
        .write_all(b"hi")
        .and_then(|()| server.read_exact(&mut got));
    report("exchange", exchanged.and_then(|()| expect_hi(&got)));
}

fn udp_case(bind: IpAddr, peer: IpAddr, report: &mut Report<'_>) {
    let Some(socket) = step(report, "bind", UdpSocket::bind((bind, 0))) else {
        return;
    };
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let target = SocketAddr::new(peer, socket.local_addr().unwrap().port());
    if !report("send", socket.send_to(b"hi", target).map(drop)) {
        return;
    }
    let mut got = [0u8; 2];
    let received = socket.recv_from(&mut got).map(drop);
    report("recv", received.and_then(|()| expect_hi(&got)));
}

fn send_case(target: IpAddr, bind: IpAddr, report: &mut Report<'_>) {
    let Some(socket) = step(report, "bind", UdpSocket::bind((bind, 0))) else {
        return;
    };
    report("send", socket.send_to(b"hi", (target, 9)).map(drop));
}

/// Reports `outcome` as `name`, keeping its value on success.
fn step<T>(report: &mut Report<'_>, name: &str, outcome: std::io::Result<T>) -> Option<T> {
    match outcome {
        Ok(value) => report(name, Ok(())).then_some(value),
        Err(error) => {
            report(name, Err(error));
            None
        }
    }
}

fn expect_hi(got: &[u8; 2]) -> std::io::Result<()> {
    if got == b"hi" {
        return Ok(());
    }
    Err(std::io::Error::other(format!("received {got:?}")))
}

/// Runs [`network_probe`] confined, in a fresh tree holding a copy of this
/// test binary (the confined command reads nothing outside its tree and
/// toolchain roots) and the `cases`, or `None` when confined commands cannot
/// start here.
fn run_confined(test: &str, cases: &[&str]) -> Option<CriterionResult> {
    let runnable = confined_commands_runnable();
    if skip_unless(runnable, test, "confined commands cannot start") {
        return None;
    }
    let root = TempDir::new().unwrap();
    let tree = root.path().canonicalize().unwrap();
    let probe = tree.join("probe");
    std::fs::copy(std::env::current_exe().unwrap(), &probe).unwrap();
    std::fs::write(tree.join(CASES_FILE), cases.join("\n")).unwrap();
    let args = ["--ignored", "--exact", PROBE_TEST, "--nocapture"];
    let spec = CommandSpec::program(probe.display().to_string(), args);
    let timeout = Duration::from_secs(60);
    let result = run_spec_with_timeout(&spec, Some(&tree), timeout, CommandConfinement::Confined);
    Some(result.unwrap())
}

/// Asserts each `expected` line is in the probe's output.
fn assert_lines(result: &CriterionResult, expected: &[&str]) {
    for line in expected {
        let found = result.stdout.lines().any(|out| out == *line);
        assert!(found, "missing probe line {line:?}: {result:?}");
    }
}

#[test]
fn loopback_tcp_listen_and_connect_work() {
    let test = "loopback_tcp_listen_and_connect_work";
    let Some(result) = run_confined(test, &["tcp 127.0.0.1", "tcp ::1"]) else {
        return;
    };
    assert_lines(
        &result,
        &["tcp 127.0.0.1 exchange: ok", "tcp ::1 exchange: ok"],
    );
}

#[test]
fn loopback_udp_bind_send_and_receive_work() {
    let test = "loopback_udp_bind_send_and_receive_work";
    let cases = ["udp 127.0.0.1 127.0.0.1", "udp ::1 ::1"];
    let Some(result) = run_confined(test, &cases) else {
        return;
    };
    let expected = ["udp 127.0.0.1 127.0.0.1 recv: ok", "udp ::1 ::1 recv: ok"];
    assert_lines(&result, &expected);
}

/// Gradle's `FileLockCommunicator` binds `new DatagramSocket(0, wildcard)`
/// and pings the lock owner on loopback.
#[test]
fn a_wildcard_udp_bind_works_and_reaches_loopback() {
    let test = "a_wildcard_udp_bind_works_and_reaches_loopback";
    let cases = ["udp 0.0.0.0 127.0.0.1", "udp :: ::1"];
    let Some(result) = run_confined(test, &cases) else {
        return;
    };
    let expected = ["udp 0.0.0.0 127.0.0.1 recv: ok", "udp :: ::1 recv: ok"];
    assert_lines(&result, &expected);
}

#[test]
fn a_tcp_connect_to_another_host_is_refused() {
    let test = "a_tcp_connect_to_another_host_is_refused";
    let Some(result) = run_confined(test, &["connect 192.0.2.1", "connect 2001:db8::1"]) else {
        return;
    };
    let refused = [
        "connect 192.0.2.1 connect: EPERM",
        "connect 2001:db8::1 connect: EPERM",
    ];
    assert_lines(&result, &refused);
}

/// The socket binds on the wildcard, as Gradle's does, so only the send is
/// refused.
#[test]
fn a_udp_send_to_another_host_is_refused() {
    let test = "a_udp_send_to_another_host_is_refused";
    let cases = ["send 192.0.2.1 0.0.0.0", "send 2001:db8::1 ::"];
    let Some(result) = run_confined(test, &cases) else {
        return;
    };
    let expected = [
        "send 192.0.2.1 0.0.0.0 bind: ok",
        "send 192.0.2.1 0.0.0.0 send: EPERM",
        "send 2001:db8::1 :: bind: ok",
        "send 2001:db8::1 :: send: EPERM",
    ];
    assert_lines(&result, &expected);
}

/// sccache's default TCP port stays closed on loopback: a host server there
/// runs compilers outside the boundary. Another loopback port connects (or is
/// refused by the kernel, nothing listening), so only the profile says EPERM.
#[test]
fn the_sccache_tcp_port_on_loopback_is_refused() {
    let test = "the_sccache_tcp_port_on_loopback_is_refused";
    let cases = [
        "connect 127.0.0.1 4226",
        "connect ::1 4226",
        "connect 127.0.0.1 4227",
    ];
    let Some(result) = run_confined(test, &cases) else {
        return;
    };
    let refused = [
        "connect 127.0.0.1 4226 connect: EPERM",
        "connect ::1 4226 connect: EPERM",
    ];
    assert_lines(&result, &refused);
    let other = "connect 127.0.0.1 4227 connect: EPERM";
    assert!(
        !result.stdout.lines().any(|line| line == other),
        "{result:?}"
    );
}
