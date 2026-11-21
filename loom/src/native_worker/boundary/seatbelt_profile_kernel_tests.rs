//! The production macOS launch against the kernel's Seatbelt, with an offline
//! stand-in for the proxy and `/bin/sh` for Codex. A Seatbelt profile cannot
//! start inside another one, so inside a stage session these skip (or fail
//! under `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`); the operator runs them from a
//! plain shell after `cargo build`, which puts the `loom` the boundary starts
//! as its bridge beside the test binary. Fixture credentials only.
use std::ffi::OsString;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpListener;
use std::os::fd::AsFd;
use std::os::unix::fs::symlink;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use serial_test::serial;

use crate::native_worker::boundary::{spawn_with_args, spawn_with_host, BoundarySpec, Quiescence};
use crate::native_worker::protocol::wire::SandboxMode;
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};
use crate::stage_cache::StageCache;

const FIXTURE_AUTH: &str = "{\"fixture\":true}\n";
const PAYLOAD_TIMEOUT: Duration = Duration::from_secs(60);

/// Each probe prints `<name>=yes` when its access succeeded, else `=no`; the
/// tunnel probe prints what came back through the bridge and the proxy.
const PAYLOAD: &str = r#"w="$1"; h="$2"; s="$3"; p="$4"
probe() { if (eval "$2") >/dev/null 2>&1; then echo "$1=yes"; else echo "$1=no"; fi; }
probe write_root ': > "$w/x"'
probe write_src ': > "$w/src/x"'
probe write_tmp ': > "$TMPDIR/x"'
probe write_sessions ': > "$CODEX_HOME/sessions/x"'
probe write_git ': > "$w/.git/x"'
probe write_loom ': > "$w/.loom/x"'
probe write_claude 'mkdir "$w/.claude"'
probe write_codex 'mkdir "$w/.codex"'
probe write_auth ': >> "$CODEX_HOME/auth.json"'
probe write_private_root ': > "$TMPDIR/../x"'
probe write_home ': > "$h/x"'
probe read_git 'cat "$w/.git/HEAD"'
probe read_map 'ls "$w/.loom/work/context"'
probe read_state 'cat "$w/.loom/work/secrets/token"'
probe read_home 'cat "$h/secret"'
probe read_real_codex 'cat "$h/.codex/auth.json"'
probe socket_other '/usr/bin/nc -U "$s" </dev/null'
probe tcp_other '/usr/bin/nc -z -G 2 127.0.0.1 "$p"'
echo "tunnel=$(printf ping | /usr/bin/nc -w 5 127.0.0.1 "${HTTPS_PROXY##*:}")"
"#;

/// `target/<profile>/loom`, beside the `deps/` directory of this test binary.
fn built_loom(test: &str) -> Option<PathBuf> {
    if !confined_commands_runnable() {
        return None;
    }
    let loom = crate::native_worker::boundary::loom_program::built_loom();
    let why = "no built loom beside the test binary: run cargo build first";
    if skip_unless(loom.is_some(), test, why) {
        return None;
    }
    loom
}

struct Host {
    _dir: tempfile::TempDir,
    root: PathBuf,
    worktree: PathBuf,
    home: PathBuf,
    proxy: UnixListener,
    other_socket: UnixListener,
    decoy: TcpListener,
    spec: BoundarySpec,
}

/// The worktree (with Git metadata and a state root link), the fake main
/// checkout's state root, the fixture host home and the proxy directory.
fn make_tree(root: &Path) {
    let (worktree, home) = (root.join("worktree"), root.join("home"));
    let main_loom = root.join("main/.loom");
    for path in [
        worktree.join("src"),
        worktree.join(".git"),
        worktree.join(".loom"),
        home.join(".codex/sessions"),
        main_loom.join("work/context"),
        main_loom.join("work/secrets"),
        root.join("proxy"),
    ] {
        fs::create_dir_all(&path).expect("a fixture directory");
    }
    fs::write(worktree.join(".git/HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    fs::write(main_loom.join("work/secrets/token"), "state\n").expect("state");
    fs::write(home.join("secret"), "not for the boundary\n").expect("secret");
    fs::write(home.join(".codex/auth.json"), FIXTURE_AUTH).expect("auth");
    symlink(main_loom.join("work"), worktree.join(".loom/work")).expect("state link");
}

fn host(loom: PathBuf) -> Host {
    let dir = tempfile::tempdir().expect("a temp dir");
    let root = dir.path().canonicalize().expect("canonical");
    let (worktree, home) = (root.join("worktree"), root.join("home"));
    make_tree(&root);
    let proxy_socket = root.join("proxy/p.sock");
    let proxy = UnixListener::bind(&proxy_socket).expect("the proxy stand-in");
    let other_socket = UnixListener::bind(root.join("other.sock")).expect("a decoy socket");
    let decoy = TcpListener::bind(("127.0.0.1", 0)).expect("a decoy port");
    let cache = StageCache::resolve(
        &root.join("repo"),
        "kernel",
        &root,
        Some(&root.join("cache")),
    )
    .expect("a stage cache");
    let spec = BoundarySpec {
        worktree: worktree.clone(),
        codex_home: home.join(".codex"),
        codex_program: PathBuf::from("/bin/sh"),
        proxy_socket,
        loom_program: loom,
        stage_cache: Some(cache),
        invocation: crate::native_worker::ids::mint_invocation_id().expect("an invocation id"),
    };
    Host {
        _dir: dir,
        root,
        worktree,
        home,
        proxy,
        other_socket,
        decoy,
        spec,
    }
}

/// Answers one connection on the proxy stand-in with what it received.
fn echo_once(proxy: UnixListener) -> mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = proxy.accept() {
            let mut buffer = [0u8; 4];
            if stream.read_exact(&mut buffer).is_ok() {
                let _ = stream.write_all(&buffer);
                let _ = sender.send(buffer.to_vec());
            }
        }
    });
    receiver
}

/// Runs the payload through the production launch ([`run_released`]).
fn run_payload(host: Host) -> (String, Option<Vec<u8>>, Host) {
    let proxy = host.proxy.try_clone().expect("clone the proxy stand-in");
    let tunnelled = echo_once(proxy);
    let decoy_port = host.decoy.local_addr().expect("decoy port").port();
    let args: Vec<OsString> = vec![
        "-c".into(),
        PAYLOAD.into(),
        "sh".into(),
        host.worktree.clone().into(),
        host.home.clone().into(),
        host.root.join("other.sock").into(),
        decoy_port.to_string().into(),
    ];
    let host_env: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    let output = run_released(&host.spec, &args, &host_env);
    let echoed = tunnelled.recv_timeout(Duration::from_secs(1)).ok();
    (output, echoed, host)
}

/// Runs `args` through the production launch with `host_env` as the
/// supervisor's environment and returns the output, after checking the
/// boundary retired released with its private root gone.
fn run_released(
    spec: &BoundarySpec,
    args: &[OsString],
    host_env: &[(OsString, OsString)],
) -> String {
    let mut boundary = spawn_with_host(spec, args, host_env).expect("the boundary starts");
    let scratch = boundary
        .mac_identity()
        .expect("a mac identity")
        .scratch_root
        .clone();
    assert_eq!(
        boundary.activate().expect("released"),
        SandboxMode::DangerFullAccess
    );
    let descriptor = boundary.stdout.as_fd().try_clone_to_owned();
    let mut stdout = fs::File::from(descriptor.expect("stdout"));
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        let _ = sender.send(text);
    });
    let output = receiver
        .recv_timeout(PAYLOAD_TIMEOUT)
        .expect("the payload finished");
    assert_eq!(boundary.retire(), Quiescence::Released);
    assert!(!scratch.exists(), "the private root is removed");
    output
}

fn verdict<'a>(output: &'a str, name: &str) -> &'a str {
    let prefix = format!("{name}=");
    output
        .lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
        .unwrap_or_else(|| panic!("{name} missing from the payload output:\n{output}"))
}

/// No connection reached `listener` from inside the boundary.
fn unix_untouched(listener: &UnixListener) -> bool {
    listener.set_nonblocking(true).expect("nonblocking");
    matches!(listener.accept(), Err(error) if error.kind() == ErrorKind::WouldBlock)
}

/// No connection reached `listener` from inside the boundary.
fn tcp_untouched(listener: &TcpListener) -> bool {
    listener.set_nonblocking(true).expect("nonblocking");
    matches!(listener.accept(), Err(error) if error.kind() == ErrorKind::WouldBlock)
}

#[test]
#[serial]
fn native_macos_writes_reach_only_the_worktree_tmp_and_sessions() {
    let Some(loom) = built_loom("native_macos_writes_reach_only_the_worktree_tmp_and_sessions")
    else {
        return;
    };
    let (output, _, host) = run_payload(host(loom));
    for allowed in ["write_root", "write_src", "write_tmp", "write_sessions"] {
        assert_eq!(verdict(&output, allowed), "yes", "{allowed}:\n{output}");
    }
    let denied = [
        "write_git",
        "write_loom",
        "write_claude",
        "write_codex",
        "write_auth",
        "write_private_root",
        "write_home",
    ];
    for name in denied {
        assert_eq!(verdict(&output, name), "no", "{name}:\n{output}");
    }
    assert!(!host.home.join("x").exists());
    assert_eq!(
        fs::read_to_string(host.home.join(".codex/auth.json")).expect("auth"),
        FIXTURE_AUTH
    );
}

#[test]
#[serial]
fn native_macos_git_and_map_reads_stay_read_only_and_state_stays_hidden() {
    let Some(loom) =
        built_loom("native_macos_git_and_map_reads_stay_read_only_and_state_stays_hidden")
    else {
        return;
    };
    let (output, _, _host) = run_payload(host(loom));
    for allowed in ["read_git", "read_map"] {
        assert_eq!(verdict(&output, allowed), "yes", "{allowed}:\n{output}");
    }
    for hidden in ["read_state", "read_home", "read_real_codex"] {
        assert_eq!(verdict(&output, hidden), "no", "{hidden}:\n{output}");
    }
}

#[test]
#[serial]
fn native_macos_the_proxy_tunnel_round_trips_and_other_sockets_are_denied() {
    let Some(loom) =
        built_loom("native_macos_the_proxy_tunnel_round_trips_and_other_sockets_are_denied")
    else {
        return;
    };
    let (output, echoed, host) = run_payload(host(loom));
    assert_eq!(verdict(&output, "tunnel"), "ping", "{output}");
    assert_eq!(
        echoed.as_deref(),
        Some(&b"ping"[..]),
        "the stand-in saw the bytes"
    );
    assert_eq!(verdict(&output, "socket_other"), "no", "{output}");
    assert_eq!(verdict(&output, "tcp_other"), "no", "{output}");
    assert!(
        unix_untouched(&host.other_socket),
        "a stray Unix socket was reached"
    );
    assert!(
        tcp_untouched(&host.decoy),
        "another loopback port was reached"
    );
}

/// The private root is created where the identity says, under the stage
/// cache, and a payload that never reads its input still retires cleanly.
#[test]
#[serial]
fn native_macos_the_private_root_lives_under_the_stage_cache() {
    let Some(loom) = built_loom("native_macos_the_private_root_lives_under_the_stage_cache") else {
        return;
    };
    let host = host(loom);
    let cache_root = host
        .spec
        .stage_cache
        .as_ref()
        .expect("cache")
        .ensure()
        .expect("root");
    let args: Vec<OsString> = ["-c", "sleep 30", "sh"].map(OsString::from).into();
    let mut boundary = spawn_with_args(&host.spec, &args).expect("the boundary starts");
    let identity = boundary.mac_identity().expect("a mac identity").clone();
    let expected = cache_root.join("native").join(&host.spec.invocation);
    assert_eq!(identity.scratch_root, expected);
    assert_eq!(identity.session as i32, boundary.boundary_pid);
    assert!(Path::new(&identity.scratch_root)
        .join("codex-home/auth.json")
        .is_file());
    boundary.activate().expect("released");
    assert_eq!(boundary.retire(), Quiescence::Released);
    assert!(!expected.exists());
}

#[path = "seatbelt_profile_kernel_cargo_tests.rs"]
mod cargo;
