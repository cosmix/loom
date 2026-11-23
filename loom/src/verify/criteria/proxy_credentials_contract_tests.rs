//! Contract `credentialed-proxy-reaches-a-confined-command`: a proxy URL
//! carrying userinfo, in any of the variables a client reads it from and with
//! or without a scheme, never reaches a confined criterion command; a
//! credential-free proxy setting and `NO_PROXY` still do.

use std::ffi::OsStr;
use std::process::Command;

use super::confine::apply_confined_environment;

const VARIABLES: [(&str, &str); 5] = [
    ("HTTPS_PROXY", "http://u:p@proxy.example:8080"),
    ("https_proxy", "u:p@proxy.example:8080"),
    ("ALL_PROXY", "socks5://u:p@proxy.example:1080"),
    ("HTTP_PROXY", "http://proxy.example:8080"),
    ("NO_PROXY", "localhost"),
];

/// The value `command` sets `name` to; `None` when absent or removed.
fn value<'a>(command: &'a Command, name: &str) -> Option<&'a OsStr> {
    command
        .get_envs()
        .find(|(key, _)| *key == name)
        .and_then(|(_, value)| value)
}

#[test]
#[serial_test::serial]
fn a_proxy_url_with_userinfo_never_reaches_a_confined_command() {
    let saved: Vec<_> = VARIABLES
        .iter()
        .map(|(name, _)| (*name, std::env::var_os(name)))
        .collect();
    for (name, setting) in VARIABLES {
        std::env::set_var(name, setting);
    }
    let mut command = Command::new("/bin/sh");
    let applied = apply_confined_environment(&mut command);
    for (name, previous) in saved {
        match previous {
            Some(previous) => std::env::set_var(name, previous),
            None => std::env::remove_var(name),
        }
    }
    applied.expect("apply the confined environment");

    for name in ["HTTPS_PROXY", "https_proxy", "ALL_PROXY"] {
        assert_eq!(
            value(&command, name),
            None,
            "{name} carried proxy credentials into the confined command"
        );
    }
    assert_eq!(
        value(&command, "HTTP_PROXY"),
        Some(OsStr::new("http://proxy.example:8080"))
    );
    assert_eq!(value(&command, "NO_PROXY"), Some(OsStr::new("localhost")));
}
