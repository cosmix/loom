//! CLI parse coverage for `loom status --web [PORT] --host HOST`.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use clap::Parser;

use super::types::{Cli, Commands};

/// `(web, terminals, host)`, the three fields `parse_status` extracts.
type StatusWebFields = (Option<Option<u16>>, bool, Option<IpAddr>);

/// Parse `loom status <extra...>` and return its `Commands::Status` fields.
fn parse_status(extra: &[&str]) -> Result<StatusWebFields, String> {
    let mut args = vec!["loom", "status"];
    args.extend_from_slice(extra);
    let cli = Cli::try_parse_from(args).map_err(|error| error.to_string())?;
    match cli.command {
        Commands::Status { web_args, .. } => Ok((web_args.web, web_args.terminals, web_args.host)),
        _ => panic!("expected Commands::Status"),
    }
}

#[test]
fn ordinary_status_has_no_web_mode() {
    let (web, terminals, host) = parse_status(&[]).expect("plain status parses");
    assert_eq!(web, None);
    assert!(!terminals);
    assert_eq!(host, None);
}

#[test]
fn bare_web_flag_defaults_to_no_port() {
    let (web, _, host) = parse_status(&["--web"]).expect("bare --web parses");
    assert_eq!(web, Some(None));
    assert_eq!(host, None);
}

#[test]
fn explicit_port_and_zero_parse() {
    for port in ["0", "7373", "65535"] {
        let (web, ..) = parse_status(&["--web", port]).unwrap_or_else(|error| {
            panic!("--web {port} should parse: {error}");
        });
        assert_eq!(web, Some(Some(port.parse().unwrap())));
    }
}

#[test]
fn host_without_web_is_rejected() {
    parse_status(&["--host", "127.0.0.1"]).expect_err("--host requires --web");
}

#[test]
fn terminals_without_web_is_rejected() {
    parse_status(&["--terminals"]).expect_err("--terminals requires --web");
}

#[test]
fn host_accepts_localhost_ipv4_and_ipv6_literals() {
    let cases: [(&str, IpAddr); 5] = [
        ("localhost", IpAddr::V4(Ipv4Addr::LOCALHOST)),
        ("127.0.0.1", IpAddr::V4(Ipv4Addr::LOCALHOST)),
        ("0.0.0.0", IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
        ("::", IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
        ("::1", IpAddr::V6(Ipv6Addr::LOCALHOST)),
    ];
    for (raw, expected) in cases {
        let (_, _, host) = parse_status(&["--web", "--host", raw])
            .unwrap_or_else(|error| panic!("--host {raw} should parse: {error}"));
        assert_eq!(host, Some(expected), "{raw}");
    }
}

#[test]
fn host_rejects_dns_names_urls_ports_and_bracketed_forms() {
    for raw in [
        "loom.example",
        "http://1.2.3.4",
        "1.2.3.4:80",
        "[::1]:80",
        "[::1]",
        " 1.2.3.4",
        "1.2.3.4 ",
        "",
    ] {
        parse_status(&["--web", "--host", raw])
            .expect_err(&format!("--host {raw:?} should be rejected"));
    }
}
