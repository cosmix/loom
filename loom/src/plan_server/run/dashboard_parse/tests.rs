use super::{parse_dashboard_startup, strip_token};
use crate::commands::status::web::startup_lines;
use std::net::IpAddr;

#[test]
fn parses_loopback_startup() {
    let lines = ["loom dashboard: http://127.0.0.1:7373/  (Ctrl-C to stop)"];

    assert_eq!(
        parse_dashboard_startup(&lines).as_deref(),
        Some("http://127.0.0.1:7373/")
    );
}

#[test]
fn keeps_terminal_bootstrap_token_and_path() {
    let lines = [
        "loom dashboard: http://127.0.0.1:7373/dashboard/?token=abc123  \
         (terminals enabled; Ctrl-C to stop)",
    ];

    assert_eq!(
        parse_dashboard_startup(&lines).as_deref(),
        Some("http://127.0.0.1:7373/dashboard/?token=abc123")
    );
}

#[test]
fn remote_startup_prefers_this_machine_over_other_interfaces() {
    let lines = [
        "loom dashboard listening on 0.0.0.0:7373 (remote access enabled; terminals enabled)",
        "  bootstrap from another machine at: http://10.20.30.40:7373/?token=abc123",
        "  bootstrap from another machine at: http://192.168.1.25:7373/?token=abc123",
        "  bootstrap from this machine at: http://127.0.0.1:7373/?token=abc123",
        "  warning: plain HTTP; anyone who can reach this address can view the dashboard, and the token above grants settings and terminal access to anyone who has it",
        "  (Ctrl-C to stop)",
    ];

    assert_eq!(
        parse_dashboard_startup(&lines).as_deref(),
        Some("http://127.0.0.1:7373/?token=abc123")
    );
}

#[test]
fn parses_real_loopback_startup_lines() {
    let cases = [
        (false, "http://127.0.0.1:7373/"),
        (true, "http://127.0.0.1:7373/?token=abc123"),
    ];
    for (terminals, expected) in cases {
        let lines = startup_lines(
            "127.0.0.1:7373".parse().unwrap(),
            false,
            terminals,
            Some("abc123"),
            &[],
        );
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();

        assert_eq!(parse_dashboard_startup(&lines).as_deref(), Some(expected));
    }
}

#[test]
fn parses_real_remote_ipv6_startup_lines() {
    let addresses = ["2001:db8::1".parse().unwrap(), "::1".parse().unwrap()];
    let lines = startup_lines(
        "[::]:8123".parse().unwrap(),
        true,
        true,
        Some("secret"),
        &addresses,
    );
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();

    assert_eq!(
        parse_dashboard_startup(&lines).as_deref(),
        Some("http://[::1]:8123/?token=secret")
    );
}

#[test]
fn rejects_garbage_and_unrecognized_urls() {
    let lines = [
        "",
        "http://127.0.0.1:7373/",
        "log: loom dashboard: http://127.0.0.1:7373/  (Ctrl-C to stop)",
        "loom dashboard: ",
        "loom dashboard: garbage  (Ctrl-C to stop)",
        "loom dashboard: http:///  (Ctrl-C to stop)",
        "loom dashboard: http://127.0.0.1:bad/  (Ctrl-C to stop)",
        "loom dashboard listening on 0.0.0.0:7373 (remote access enabled)",
        "  bootstrap from another machine at: http://10.20.30.40:7373/?token=secret",
        "  bootstrap from this machine at: garbage",
        "  (Ctrl-C to stop)",
    ];

    assert_eq!(parse_dashboard_startup(&lines), None);
}

#[test]
fn empty_startup_has_no_url() {
    assert_eq!(parse_dashboard_startup(&[]), None);
}

#[test]
fn first_parseable_line_wins() {
    let lines = [
        "loom dashboard: invalid",
        "  bootstrap from this machine at: http://127.0.0.1:8123/?token=first",
        "loom dashboard: http://127.0.0.1:7373/  (Ctrl-C to stop)",
        "  bootstrap from this machine at: http://[::1]:8123/?token=second",
    ];

    assert_eq!(
        parse_dashboard_startup(&lines).as_deref(),
        Some("http://127.0.0.1:8123/?token=first")
    );
}

#[test]
fn accepts_twentieth_line() {
    let mut lines = ["unrelated output"; 20];
    lines[19] = "loom dashboard: http://127.0.0.1:7373/  (Ctrl-C to stop)";

    assert_eq!(
        parse_dashboard_startup(&lines).as_deref(),
        Some("http://127.0.0.1:7373/")
    );
}

#[test]
fn ignores_lines_after_twenty() {
    let mut lines = ["unrelated output"; 21];
    lines[20] = "loom dashboard: http://127.0.0.1:7373/  (Ctrl-C to stop)";

    assert_eq!(parse_dashboard_startup(&lines), None);
}

#[test]
fn stripping_only_token_removes_question_mark_and_keeps_path() {
    let cases = [
        ("http://127.0.0.1:7373/?token=abc", "http://127.0.0.1:7373/"),
        (
            "http://127.0.0.1:7373/dashboard/?token=abc",
            "http://127.0.0.1:7373/dashboard/",
        ),
        ("http://[::1]:8123/run?token=", "http://[::1]:8123/run"),
        ("http://[::1]:8123/?token", "http://[::1]:8123/"),
        ("http://[::1]:8123/?token=a&token=b", "http://[::1]:8123/"),
        ("http://[::1]:8123/?", "http://[::1]:8123/"),
    ];

    for (url, expected) in cases {
        assert_eq!(strip_token(url), expected, "{url}");
    }
}

#[test]
fn stripping_token_preserves_other_query_parameters() {
    let cases = [
        (
            "http://127.0.0.1:7373/?token=abc&view=run",
            "http://127.0.0.1:7373/?view=run",
        ),
        (
            "http://127.0.0.1:7373/?view=run&token=abc&stage=a%2Fb",
            "http://127.0.0.1:7373/?view=run&stage=a%2Fb",
        ),
        (
            "http://127.0.0.1:7373/?view=run&token=abc",
            "http://127.0.0.1:7373/?view=run",
        ),
        (
            "http://127.0.0.1:7373/?token=a&view=run&token=b&view=all",
            "http://127.0.0.1:7373/?view=run&view=all",
        ),
    ];

    for (url, expected) in cases {
        assert_eq!(strip_token(url), expected, "{url}");
    }
}

#[test]
fn stripping_token_preserves_similar_keys_and_values() {
    let url = "http://127.0.0.1:7373/path/?access_token=keep&tokenized=yes&value=token=keep&flag";

    assert_eq!(strip_token(url), url);
}

#[test]
fn stripping_token_preserves_fragment() {
    let cases = [
        (
            "http://127.0.0.1:7373/path/?token=abc#stage",
            "http://127.0.0.1:7373/path/#stage",
        ),
        (
            "http://127.0.0.1:7373/?token=abc&view=run#stage?token=anchor",
            "http://127.0.0.1:7373/?view=run#stage?token=anchor",
        ),
        (
            "http://127.0.0.1:7373/#stage?token=anchor",
            "http://127.0.0.1:7373/#stage?token=anchor",
        ),
    ];

    for (url, expected) in cases {
        assert_eq!(strip_token(url), expected, "{url}");
    }
}

#[test]
fn stripping_url_without_query_preserves_it() {
    let url = "http://127.0.0.1:7373/dashboard/";

    assert_eq!(strip_token(url), url);
}

fn real_lines(bind: &str, remote: bool, addresses: &[&str]) -> Vec<String> {
    let addresses: Vec<IpAddr> = addresses.iter().map(|ip| ip.parse().unwrap()).collect();
    startup_lines(
        bind.parse().unwrap(),
        remote,
        true,
        Some("secret"),
        &addresses,
    )
}

fn parse(lines: &[String]) -> Option<String> {
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    parse_dashboard_startup(&lines)
}

#[test]
fn parses_real_remote_startup_lines_for_one_concrete_address() {
    let cases = [
        ("192.168.1.5:7373", "http://192.168.1.5:7373/?token=secret"),
        (
            "[2001:db8::5]:8123",
            "http://[2001:db8::5]:8123/?token=secret",
        ),
    ];
    for (bind, expected) in cases {
        let lines = real_lines(bind, true, &[]);
        // This bind prints no `this machine` line: the bootstrap line is the only URL.
        assert!(lines.iter().all(|line| !line.contains("this machine")));

        assert_eq!(parse(&lines).as_deref(), Some(expected), "{bind}");
    }
}

#[test]
fn a_concrete_remote_bind_to_loopback_parses_too() {
    let lines = real_lines("127.0.0.1:7373", true, &[]);

    assert_eq!(
        parse(&lines).as_deref(),
        Some("http://127.0.0.1:7373/?token=secret")
    );
}

#[test]
fn a_wildcard_bind_without_a_loopback_line_has_no_url() {
    let lines = real_lines("0.0.0.0:7373", true, &["192.168.1.5", "10.0.0.7"]);

    assert_eq!(parse(&lines), None);
}

#[test]
fn another_machine_line_needs_its_header_and_the_bound_address() {
    let bootstrap = "  bootstrap from another machine at: http://192.168.1.5:7373/?token=secret";
    let cases: [&[&str]; 3] = [
        &[bootstrap],
        &[
            "loom dashboard listening on 192.168.1.6:7373 (remote access enabled)",
            bootstrap,
        ],
        &[
            "loom dashboard listening on not-an-address (remote access enabled)",
            bootstrap,
        ],
    ];
    for lines in cases {
        assert_eq!(parse_dashboard_startup(lines), None, "{lines:?}");
    }
}

#[test]
fn this_machine_line_still_wins_over_a_concrete_bind_line() {
    let lines = [
        "loom dashboard listening on 192.168.1.5:7373 (remote access enabled)",
        "  bootstrap from another machine at: http://192.168.1.5:7373/?token=remote",
        "  bootstrap from this machine at: http://127.0.0.1:7373/?token=local",
    ];

    assert_eq!(
        parse_dashboard_startup(&lines).as_deref(),
        Some("http://127.0.0.1:7373/?token=local")
    );
}

#[test]
fn concrete_bind_url_redacts_to_a_tokenless_link() {
    let lines = real_lines("192.168.1.5:7373", true, &[]);

    assert_eq!(
        strip_token(&parse(&lines).unwrap()),
        "http://192.168.1.5:7373/"
    );
}
