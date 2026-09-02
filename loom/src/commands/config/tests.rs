use super::*;
use crate::user_config::{redirect_user_config, UserConfigRedirect};

// Every test here installs a `redirect_user_config` guard over a temp path,
// so a set/read round trip exercises the real read-modify-write behavior
// without ever touching the operator's `~/.loom/config.toml`.
fn redirect() -> (tempfile::TempDir, UserConfigRedirect) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let guard = redirect_user_config(path);
    (temp, guard)
}

#[test]
fn reading_a_key_with_no_file_present_yields_its_default() {
    let (_temp, _guard) = redirect();
    assert_eq!(print_key("update.check_interval_hours").unwrap(), "24\n");
}

#[test]
fn a_set_round_trips_through_a_subsequent_read() {
    let (_temp, _guard) = redirect();
    assert_eq!(
        set_key("update.check_interval_hours", "6").unwrap(),
        "update.check_interval_hours: 24 -> 6\n"
    );
    assert_eq!(print_key("update.check_interval_hours").unwrap(), "6\n");
}

#[test]
fn list_reflects_a_set_key_as_set_and_others_as_default() {
    let (_temp, _guard) = redirect();
    set_key("update.check_interval_hours", "6").unwrap();

    let out = list().unwrap();
    let set_line = out
        .lines()
        .find(|line| line.starts_with("update.check_interval_hours"))
        .unwrap_or_else(|| panic!("missing update.check_interval_hours line in {out:?}"));
    assert!(set_line.contains("set"), "{set_line}");

    let default_line = out
        .lines()
        .find(|line| line.starts_with("terminal.backend"))
        .unwrap_or_else(|| panic!("missing terminal.backend line in {out:?}"));
    assert!(default_line.contains("default"), "{default_line}");
}

#[test]
fn print_resolved_renders_every_key_with_a_set_value_applied() {
    let (_temp, _guard) = redirect();
    set_key("context.ceiling_tokens", "70000").unwrap();

    let out = print_resolved().unwrap();
    assert!(out.contains("ceiling_tokens = 70000"), "{out}");
    assert!(out.contains("[context]"), "{out}");
    assert!(out.contains("[terminal]"), "{out}");
    assert!(out.contains("[update]"), "{out}");
}

fn args(key: Option<&str>, list: bool, print: bool) -> ConfigArgs {
    ConfigArgs {
        key: key.map(str::to_string),
        value: None,
        list,
        print,
    }
}

/// `should_open_tui` is the gate `execute()` uses to decide between the
/// raw-mode TUI and the non-interactive branches: only a bare invocation (no
/// key, `--list`, or `--print`) at a real terminal opens it. Driving the
/// function directly, with the tty state as a parameter, covers every
/// combination without faking a terminal or risking the TUI launching inside
/// the test binary.
#[test]
fn should_open_tui_only_for_a_bare_invocation_at_a_tty() {
    assert!(should_open_tui(&args(None, false, false), true));

    assert!(!should_open_tui(&args(None, false, false), false));
    assert!(!should_open_tui(
        &args(Some("update.check_interval_hours"), false, false),
        true
    ));
    assert!(!should_open_tui(&args(None, true, false), true));
    assert!(!should_open_tui(&args(None, false, true), true));
}

#[test]
fn an_unknown_key_is_an_error_listing_the_valid_keys() {
    let (_temp, _guard) = redirect();
    let err = print_key("no.such.key").unwrap_err().to_string();
    assert!(err.contains("no.such.key"), "{err}");
    for key in keys::KEYS {
        assert!(err.contains(key.name), "missing {} in: {err}", key.name);
    }
}

#[test]
fn a_value_failing_its_key_type_is_an_error_naming_the_key() {
    let (_temp, _guard) = redirect();
    let err = set_key("update.check_interval_hours", "not-a-number")
        .unwrap_err()
        .to_string();
    assert!(err.contains("update.check_interval_hours"), "{err}");
}

/// The stage's headline invariant: a read must never create `~/.loom`. Every
/// other test in this file redirects into a directory that already exists,
/// which would let a directory-creating read pass silently. Redirecting into
/// a config path whose parent (`dotloom`) does not exist yet pins both
/// halves: `--list` and `-k <key>` create nothing, and only a set creates the
/// directory.
#[test]
fn a_read_never_creates_the_user_config_directory_only_a_set_does() {
    let temp = tempfile::tempdir().unwrap();
    let dotloom = temp.path().join("dotloom");
    let _guard = redirect_user_config(dotloom.join("config.toml"));

    list().unwrap();
    print_key("update.check_interval_hours").unwrap();
    assert!(
        !dotloom.exists(),
        "a read must not create the user config directory"
    );

    set_key("update.check_interval_hours", "6").unwrap();
    assert!(
        dotloom.exists(),
        "a set must create the user config directory"
    );
}
