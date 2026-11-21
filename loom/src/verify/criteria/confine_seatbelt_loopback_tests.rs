//! Kernel tests of the Seatbelt loopback grant, run through the real launcher:
//! no IP networking by default, and a stage's `allow_local_binding` opts in.
//! Each probe is a `/usr/bin/perl` one-liner using explicit `socket`, `bind` and
//! `connect` calls (`IO::Socket::INET` skips `bind` for a wildcard port 0).

use std::net::TcpListener;
use std::path::Path;
use std::time::Duration;

use super::{launcher_absent, tree_below_cargo_files};
use crate::models::stage::{AcceptanceCriterion, CommandConfinement, NetworkConfig, Stage};
use crate::verify::criteria::{
    run_acceptance_with_config, run_spec_with_timeout, AcceptanceResult, CommandSpec,
    CriteriaConfig, CriterionResult,
};

const TIMEOUT: Duration = Duration::from_secs(20);

/// Binds and listens on `127.0.0.1` and on `0.0.0.0`, port 0, printing
/// `LOOPBACK_BOUND` and `WILDCARD_BOUND` for each that works.
const BIND_PROBE: &str = r#"/usr/bin/perl -MSocket -e 'for $t (["127.0.0.1","LOOPBACK_BOUND"],["0.0.0.0","WILDCARD_BOUND"]) { socket(S, PF_INET, SOCK_STREAM, 0) or next; bind(S, sockaddr_in(0, inet_aton($t->[0]))) && listen(S, 1) and print "$t->[1]\n"; close(S); }'"#;

/// Listens on `127.0.0.1:0`, connects to its own listener and exchanges a byte.
const ROUNDTRIP_PROBE: &str = r#"/usr/bin/perl -MSocket -e 'socket(L, PF_INET, SOCK_STREAM, 0) or die; bind(L, sockaddr_in(0, inet_aton("127.0.0.1"))) or die "bind: $!\n"; listen(L, 1) or die; ($port) = sockaddr_in(getsockname(L)); socket(C, PF_INET, SOCK_STREAM, 0) or die; connect(C, sockaddr_in($port, inet_aton("127.0.0.1"))) or die "connect: $!\n"; accept(A, L) or die; syswrite(C, "x", 1); sysread(A, $b, 1); print "ROUNDTRIP_OK\n" if $b eq "x";'"#;

/// Connects to the host and port given as arguments, printing `CONNECTED` or
/// `REFUSED` with the error.
const DIAL_PROBE: &str = r#"/usr/bin/perl -MSocket -e 'alarm 8; socket(C, PF_INET, SOCK_STREAM, 0) or die; if (connect(C, sockaddr_in($ARGV[1], inet_aton($ARGV[0])))) { print "CONNECTED\n" } else { print "REFUSED: $!\n" }'"#;

fn run(spec: &str, tree: &Path, confinement: CommandConfinement) -> CriterionResult {
    let spec = CommandSpec::shell(spec);
    run_spec_with_timeout(&spec, Some(tree), TIMEOUT, confinement).unwrap()
}

/// The results of `command` as the only criterion of a stage whose own
/// `network` section has `allow_local_binding` set to `local_binding`.
fn run_in_stage(command: &str, tree: &Path, local_binding: Option<bool>) -> CriterionResult {
    let mut stage = Stage::new("loopback".to_string(), None);
    stage.sandbox.network = local_binding.map(|allow_local_binding| NetworkConfig {
        allow_local_binding,
        ..NetworkConfig::default()
    });
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple(command.to_string()));
    let config = CriteriaConfig::default();
    match run_acceptance_with_config(&stage, Some(tree), &config).unwrap() {
        AcceptanceResult::AllPassed { mut results }
        | AcceptanceResult::Failed { mut results, .. } => results.remove(0),
    }
}

/// Issue #40 in one denied-network command: neither a loopback nor a wildcard
/// listener starts, while the parent directory lists and the Cargo files two
/// levels up are refused.
#[test]
fn open_issues_macos_completion_kernel_default_binds_nothing_and_keeps_ancestor_cargo_files_hidden()
{
    let name = "open_issues_macos_completion_kernel_default_binds_nothing_and_keeps_ancestor_cargo_files_hidden";
    if launcher_absent(name) {
        return;
    }
    let (_root, beside) = tree_below_cargo_files("[workspace]\n", "[build]\n");
    let tree = beside.parent().unwrap().join("mid/worktree");
    std::fs::create_dir_all(&tree).unwrap();
    let spec = format!(
        "{BIND_PROBE}; ls .. >/dev/null && echo LISTED; \
         cat ../../Cargo.toml >/dev/null 2>&1 && echo MANIFEST_READ; \
         cat ../../.cargo/config.toml >/dev/null 2>&1 && echo CONFIG_READ; true"
    );
    let control = run(&spec, &tree, CommandConfinement::Inherit);
    for marker in [
        "LOOPBACK_BOUND",
        "WILDCARD_BOUND",
        "LISTED",
        "MANIFEST_READ",
        "CONFIG_READ",
    ] {
        assert!(
            control.stdout.contains(marker),
            "unconfined, the fixture shows {marker}: {control:?}"
        );
    }
    let result = run(&spec, &tree, CommandConfinement::Confined);
    assert!(
        result.success && result.stdout.contains("LISTED"),
        "a confined command lists its parent: {result:?}"
    );
    for marker in [
        "LOOPBACK_BOUND",
        "WILDCARD_BOUND",
        "MANIFEST_READ",
        "CONFIG_READ",
    ] {
        assert!(
            !result.stdout.contains(marker),
            "a confined command must not reach {marker}: {result:?}"
        );
    }
}

/// A stage that sets `allow_local_binding` serves and dials on loopback, and
/// still cannot connect to an address off this host.
#[test]
fn the_local_binding_opt_in_serves_and_dials_loopback_but_not_other_hosts() {
    let name = "the_local_binding_opt_in_serves_and_dials_loopback_but_not_other_hosts";
    if launcher_absent(name) {
        return;
    }
    let (_root, tree) = tree_below_cargo_files("[package]\n", "");
    let control = run(ROUNDTRIP_PROBE, &tree, CommandConfinement::Inherit);
    assert!(control.stdout.contains("ROUNDTRIP_OK"), "{control:?}");
    let command =
        format!("{ROUNDTRIP_PROBE}; {DIAL_PROBE} 192.0.2.1 443; {DIAL_PROBE} 1.1.1.1 443");
    let granted = run_in_stage(&command, &tree, Some(true));
    assert!(
        granted.stdout.contains("ROUNDTRIP_OK"),
        "an opted-in stage listens, dials its own listener and exchanges a byte: {granted:?}"
    );
    assert!(
        !granted.stdout.contains("CONNECTED") && granted.stdout.matches("REFUSED").count() == 2,
        "a connect off this host stays refused: {granted:?}"
    );
    for local_binding in [None, Some(false)] {
        let denied = run_in_stage(ROUNDTRIP_PROBE, &tree, local_binding);
        assert!(
            !denied.stdout.contains("ROUNDTRIP_OK") && denied.stderr.contains("bind"),
            "without the opt-in ({local_binding:?}) the bind is refused: {denied:?}"
        );
    }
}

/// The grant reaches every host loopback listener except sccache's default
/// port, where a host server would run compilers outside the boundary.
#[test]
fn the_loopback_grant_reaches_host_listeners_except_the_sccache_port() {
    let name = "the_loopback_grant_reaches_host_listeners_except_the_sccache_port";
    if launcher_absent(name) {
        return;
    }
    let (_root, tree) = tree_below_cargo_files("[package]\n", "");
    let other = TcpListener::bind("127.0.0.1:0").unwrap();
    let other_port = other.local_addr().unwrap().port();
    // A host sccache may already own 4226; either way something listens there.
    let _sccache = TcpListener::bind("127.0.0.1:4226");
    let dial_control = |port: u16| {
        let command = format!("{DIAL_PROBE} 127.0.0.1 {port}");
        run(&command, &tree, CommandConfinement::Inherit)
            .stdout
            .contains("CONNECTED")
    };
    if !dial_control(other_port) || !dial_control(4226) {
        eprintln!("SKIP {name}: nothing listens on 4226 and it cannot be bound");
        return;
    }
    let command = format!("{DIAL_PROBE} 127.0.0.1 {other_port}; {DIAL_PROBE} 127.0.0.1 4226");
    let granted = run_in_stage(&command, &tree, Some(true));
    let lines: Vec<&str> = granted.stdout.lines().collect();
    assert_eq!(lines.first(), Some(&"CONNECTED"), "{granted:?}");
    assert!(
        lines.get(1).is_some_and(|line| line.starts_with("REFUSED")),
        "the sccache default port stays unreachable: {granted:?}"
    );
}
