use super::*;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

const LOGGED_IN_JSON: &str = r#"{"analyticsDisabled": false, "apiProvider": "firstParty", "authMethod": "claude.ai", "configDirectory": "/home/u/.claude", "email": "someone@example.com", "loggedIn": true, "orgId": "org-1234-secret", "orgName": "Secret Org Inc", "projectsDirectory": "/home/u/.claude/projects", "subscriptionType": "max"}"#;

const LOGGED_OUT_JSON: &str = r#"{"loggedIn": false, "authMethod": "none", "apiProvider": "firstParty", "analyticsDisabled": false, "projectsDirectory": "/home/u/.claude/projects", "configDirectory": "/home/u/.claude"}"#;

#[test]
fn logged_in_status_parses_to_logged_in() {
    assert_eq!(
        parse_auth_status(LOGGED_IN_JSON, true),
        AuthProbe::LoggedIn {
            method: "claude.ai".to_string()
        }
    );
}

#[test]
fn logged_out_status_is_not_logged_in_whatever_the_exit() {
    assert_eq!(
        parse_auth_status(LOGGED_OUT_JSON, true),
        AuthProbe::NotLoggedIn
    );
    assert_eq!(
        parse_auth_status(LOGGED_OUT_JSON, false),
        AuthProbe::NotLoggedIn
    );
}

#[test]
fn unparseable_output_is_unknown() {
    for exit_success in [true, false] {
        assert!(matches!(
            parse_auth_status("Error: something broke", exit_success),
            AuthProbe::Unknown(_)
        ));
        assert!(matches!(
            parse_auth_status("", exit_success),
            AuthProbe::Unknown(_)
        ));
        assert!(matches!(
            parse_auth_status(r#"{"authMethod": "claude.ai"}"#, exit_success),
            AuthProbe::Unknown(_)
        ));
    }
}

#[test]
fn probe_output_never_carries_identity() {
    let probe = parse_auth_status(LOGGED_IN_JSON, true);
    let rendered = format!("{probe:?}");
    for secret in ["someone@example.com", "org-1234-secret", "Secret Org Inc"] {
        assert!(!rendered.contains(secret), "{rendered}");
    }

    let malformed =
        r#"{"email": "someone@example.com", "orgName": "Secret Org Inc", "loggedIn": tru"#;
    let AuthProbe::Unknown(reason) = parse_auth_status(malformed, false) else {
        panic!("malformed JSON must be Unknown");
    };
    for secret in ["someone@example.com", "Secret Org Inc"] {
        assert!(!reason.contains(secret), "{reason}");
    }
}

/// A fake claude that is logged in only when `$USER` is non-empty.
fn write_fake_claude(dir: &TempDir) -> std::path::PathBuf {
    let script = dir.path().join("claude");
    let body = format!(
        "#!/bin/sh\nif [ -n \"$USER\" ]; then\n  echo '{{\"loggedIn\": true, \"authMethod\": \"claude.ai\"}}'\n  exit 0\nfi\necho '{LOGGED_OUT_JSON}'\nexit 1\n"
    );
    std::fs::write(&script, body).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

/// Probe through `source`, retrying past the ETXTBSY fork/exec race on the
/// just-written script (it surfaces as `Unknown`; the fake never says that).
fn probe_retrying(script: &Path, source: &[(&str, &str)]) -> AuthProbe {
    let mut probe = AuthProbe::Unknown(String::new());
    for _ in 0..50 {
        probe = stage_auth_status_from(script, source.iter().copied());
        if !matches!(probe, AuthProbe::Unknown(_)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    probe
}

#[test]
fn stage_probe_sees_user() {
    let dir = TempDir::new().unwrap();
    let script = write_fake_claude(&dir);
    let home = dir.path().to_str().unwrap();

    let with_user = [("HOME", home), ("PATH", "/usr/bin:/bin"), ("USER", "alice")];
    assert_eq!(
        probe_retrying(&script, &with_user),
        AuthProbe::LoggedIn {
            method: "claude.ai".to_string()
        }
    );

    let without_user = [("HOME", home), ("PATH", "/usr/bin:/bin")];
    assert_eq!(
        probe_retrying(&script, &without_user),
        AuthProbe::NotLoggedIn
    );
}

#[test]
fn missing_claude_binary_is_unknown_without_output() {
    let probe = stage_auth_status_from(
        Path::new("/nonexistent/claude-xyz"),
        [("PATH", "/usr/bin:/bin")],
    );
    assert!(matches!(probe, AuthProbe::Unknown(_)), "{probe:?}");
}
