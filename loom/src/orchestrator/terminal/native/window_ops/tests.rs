use super::*;

#[cfg(target_os = "macos")]
use crate::orchestrator::terminal::emulator::TerminalEmulator;

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn reply(code: i32, stdout: &str) -> Option<Output> {
    use std::os::unix::process::ExitStatusExt;
    Some(Output {
        status: std::process::ExitStatus::from_raw(code << 8),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
    })
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
#[should_panic(expected = "real window commands are forbidden in unit tests")]
fn real_window_runner_rejects_unit_tests() {
    run_window_command(&mut Command::new("osascript"), "guard check");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn window_commands_without_a_fixture_are_unavailable() {
    assert!(bounded_window_command(&mut Command::new("osascript"), "no fixture").is_none());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn window_runner_returns_scripted_output_and_records_arguments() {
    let (output, calls) = test_commands::capture([reply(0, "true\n")], || {
        let mut command = Command::new("osascript");
        command.args(["-e", "return true"]);
        bounded_window_command(&mut command, "scripted command").unwrap()
    });
    assert!(output.status.success());
    assert_eq!(output.stdout, b"true\n");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program, "osascript");
    assert_eq!(
        calls[0].args,
        ["-e", "return true"].map(std::ffi::OsString::from)
    );
}

#[cfg(target_os = "macos")]
#[test]
fn applescript_boolean_requires_success_and_exact_true() {
    for (code, stdout, expected) in [
        (0, "true\n", true),
        (0, "  true  ", true),
        (0, "false", false),
        (0, "true but no window", false),
        (1, "true", false),
    ] {
        let (result, calls) = test_commands::capture([reply(code, stdout)], || {
            close_window_by_title_for_terminal("loom-test", &TerminalEmulator::TerminalApp)
        });
        assert_eq!(result, expected, "exit={code}, stdout={stdout:?}");
        assert_eq!(calls.len(), 1);
    }
    let (result, _) = test_commands::capture([None], || {
        close_window_by_title_for_terminal("loom-test", &TerminalEmulator::TerminalApp)
    });
    assert!(!result);
}

#[cfg(target_os = "macos")]
#[test]
fn terminal_scripts_keep_titles_literal_for_each_application() {
    let title = "loom-a\"\\\nb";
    let escaped_title = r#"loom-a\"\\\nb"#;
    for (terminal, app) in [
        (TerminalEmulator::TerminalApp, "Terminal"),
        (TerminalEmulator::ITerm2, "iTerm2"),
        (TerminalEmulator::Ghostty, "Ghostty"),
        (TerminalEmulator::Kitty, "kitty"),
        (TerminalEmulator::Alacritty, "Alacritty"),
        (TerminalEmulator::Wezterm, "WezTerm"),
    ] {
        let ((closed, exists), calls) =
            test_commands::capture([reply(0, "true"), reply(0, "true")], || {
                (
                    close_window_by_title_for_terminal(title, &terminal),
                    window_exists_by_title_for_terminal(title, &terminal),
                )
            });
        assert!(closed && exists);
        assert_eq!(calls.len(), 2);
        for call in calls {
            assert_eq!(call.program, "osascript");
            assert_eq!(call.args.len(), 2);
            assert_eq!(call.args[0], "-e");
            let script = call.args[1].to_str().unwrap();
            assert!(script.starts_with(&format!("tell application \"{app}\"\n")));
            assert!(script.contains(&format!("every window whose name is \"{escaped_title}\"")));
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn terminal_fallbacks_query_each_application_and_return_false() {
    for operation in [
        macos::close_window_by_title_with_preference,
        macos::window_exists_by_title_with_preference,
    ] {
        let (result, calls) = test_commands::capture((0..6).map(|_| reply(0, "false")), || {
            operation("loom-absent", None)
        });
        assert!(!result);
        assert_eq!(calls.len(), 6);
        for (call, app) in calls.iter().zip([
            "Terminal",
            "iTerm2",
            "Ghostty",
            "kitty",
            "Alacritty",
            "WezTerm",
        ]) {
            assert!(call.args[1]
                .to_str()
                .unwrap()
                .starts_with(&format!("tell application \"{app}\"\n")));
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn preferred_terminal_falls_back_on_failure_and_stops_on_success() {
    for operation in [
        macos::close_window_by_title_with_preference,
        macos::window_exists_by_title_with_preference,
    ] {
        let (found, calls) = test_commands::capture([reply(0, "false"), reply(0, "true")], || {
            operation("loom-test", Some(TerminalEmulator::Kitty))
        });
        assert!(found);
        assert_eq!(calls.len(), 2);
        assert!(calls[0].args[1]
            .to_str()
            .unwrap()
            .starts_with("tell application \"kitty\"\n"));
        assert!(calls[1].args[1]
            .to_str()
            .unwrap()
            .starts_with("tell application \"Terminal\"\n"));
        let (found, calls) = test_commands::capture([reply(0, "true")], || {
            operation("loom-test", Some(TerminalEmulator::Kitty))
        });
        assert!(found);
        assert_eq!(calls.len(), 1);
    }
}

// O-5: prefix-sharing stage IDs must NOT collide. The substring matching
// these helpers replaced would have matched `loom-auth` against
// `loom-auth-tests`, killing a healthy sibling / suppressing crash
// detection. The exact-match logic below proves the two never collide.

#[cfg(target_os = "linux")]
#[test]
fn test_wmctrl_list_title_exact_no_prefix_collision() {
    // wmctrl -l line: <id> <desktop> <host> <title>
    let auth = "0x01 0 host loom-auth";
    let auth_tests = "0x02 0 host loom-auth-tests";

    assert_eq!(wmctrl_list_title(auth), Some("loom-auth"));
    assert_eq!(wmctrl_list_title(auth_tests), Some("loom-auth-tests"));

    // Exact comparison: searching for `loom-auth` must NOT match
    // `loom-auth-tests` (the old substring `.contains` bug).
    assert_ne!(wmctrl_list_title(auth_tests), Some("loom-auth"));
    assert_eq!(wmctrl_list_title(auth), Some("loom-auth"));
}

#[cfg(target_os = "linux")]
#[test]
fn test_wmctrl_list_title_handles_spaces_and_short_lines() {
    // Titles may contain spaces; everything after column 3 is the title.
    assert_eq!(
        wmctrl_list_title("0x03 0 host loom-auth — bash"),
        Some("loom-auth — bash")
    );
    // Fewer than 4 columns → None (no title field present).
    assert_eq!(wmctrl_list_title("0x04 0 host"), None);
    assert_eq!(wmctrl_list_title("0x05 0"), None);
}

#[cfg(target_os = "linux")]
#[test]
fn test_regex_escape_anchors_prevent_prefix_match() {
    // The anchored regex for `loom-auth` is `^loom-auth$`; it must not
    // be a prefix of the anchored regex for `loom-auth-tests`.
    let auth = format!("^{}$", regex_escape("loom-auth"));
    let auth_tests = format!("^{}$", regex_escape("loom-auth-tests"));
    assert_eq!(auth, "^loom-auth$");
    assert_eq!(auth_tests, "^loom-auth-tests$");
    assert_ne!(auth, auth_tests);

    // Metacharacters in a title are escaped so anchoring stays literal.
    assert_eq!(regex_escape("loom-a.b+c"), "loom-a\\.b\\+c");
}
