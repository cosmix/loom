use super::*;

#[test]
fn test_binary_roundtrip() {
    // Linux terminals have unique binary names that roundtrip
    let terminals = [
        TerminalEmulator::XdgTerminalExec,
        TerminalEmulator::Kitty,
        TerminalEmulator::Alacritty,
        TerminalEmulator::Foot,
        TerminalEmulator::Wezterm,
        TerminalEmulator::GnomeTerminal,
        TerminalEmulator::Konsole,
        TerminalEmulator::Xfce4Terminal,
        TerminalEmulator::MateTerminal,
        TerminalEmulator::XTerm,
        TerminalEmulator::Urxvt,
    ];

    for terminal in terminals {
        let binary = terminal.binary();
        let parsed = TerminalEmulator::from_binary(binary);
        assert_eq!(parsed, Some(terminal));
    }
}

#[test]
fn test_from_binary_unknown() {
    assert_eq!(TerminalEmulator::from_binary("unknown-terminal"), None);
}

#[test]
fn test_from_name_macos() {
    // Test macOS Terminal.app variants
    assert_eq!(
        TerminalEmulator::from_name("Terminal"),
        Some(TerminalEmulator::TerminalApp)
    );
    assert_eq!(
        TerminalEmulator::from_name("Terminal.app"),
        Some(TerminalEmulator::TerminalApp)
    );

    // Test iTerm2 variants
    assert_eq!(
        TerminalEmulator::from_name("iTerm"),
        Some(TerminalEmulator::ITerm2)
    );
    assert_eq!(
        TerminalEmulator::from_name("iTerm2"),
        Some(TerminalEmulator::ITerm2)
    );
    assert_eq!(
        TerminalEmulator::from_name("iTerm.app"),
        Some(TerminalEmulator::ITerm2)
    );
    assert_eq!(
        TerminalEmulator::from_name("iTerm2.app"),
        Some(TerminalEmulator::ITerm2)
    );

    // Test TERM_PROGRAM values
    assert_eq!(
        TerminalEmulator::from_name("Apple_Terminal"),
        Some(TerminalEmulator::TerminalApp)
    );

    // from_name falls back to from_binary for Linux terminals
    assert_eq!(
        TerminalEmulator::from_name("kitty"),
        Some(TerminalEmulator::Kitty)
    );
}

#[test]
fn test_display() {
    assert_eq!(TerminalEmulator::Kitty.to_string(), "kitty");
    assert_eq!(
        TerminalEmulator::GnomeTerminal.to_string(),
        "gnome-terminal"
    );
    // macOS terminals display their app names, not "osascript"
    assert_eq!(TerminalEmulator::TerminalApp.to_string(), "Terminal.app");
    assert_eq!(TerminalEmulator::ITerm2.to_string(), "iTerm2");
}

#[test]
fn test_macos_binary() {
    // Both macOS terminals use osascript
    assert_eq!(TerminalEmulator::TerminalApp.binary(), "osascript");
    assert_eq!(TerminalEmulator::ITerm2.binary(), "osascript");
}

#[test]
fn test_terminal_app_build_command() {
    let emulator = TerminalEmulator::TerminalApp;
    let workdir = Path::new("/tmp/test");
    let cmd = emulator.build_command("Test Title", workdir, Path::new("/tmp/wrapper.sh"));

    // Verify osascript is used
    assert_eq!(cmd.get_program(), "osascript");

    // Verify args contain the AppleScript
    let args: Vec<_> = cmd.get_args().collect();
    assert_eq!(args.len(), 2);
    assert_eq!(args[0], "-e");

    let script = args[1].to_str().unwrap();
    assert!(script.contains("tell application \"Terminal\""));
    // Note: workdir is no longer in the AppleScript - it's handled by the wrapper script
    assert!(script.contains("/tmp/wrapper.sh"));
    assert!(script.contains("Test Title"));
}

#[test]
fn test_iterm2_build_command() {
    let emulator = TerminalEmulator::ITerm2;
    let workdir = Path::new("/tmp/test");
    let cmd = emulator.build_command("Test Title", workdir, Path::new("/tmp/wrapper.sh"));

    // Verify osascript is used
    assert_eq!(cmd.get_program(), "osascript");

    // Verify args contain the AppleScript
    let args: Vec<_> = cmd.get_args().collect();
    assert_eq!(args.len(), 2);
    assert_eq!(args[0], "-e");

    let script = args[1].to_str().unwrap();
    assert!(script.contains("tell application \"iTerm\""));
    assert!(script.contains("set loomWindow to (create window with default profile)"));
    assert!(script.contains("set name of loomWindow to \"Test Title\""));
    assert!(script.contains("tell current session of loomWindow"));
    // Note: workdir is no longer in the AppleScript - it's handled by the wrapper script
    assert!(script.contains("/tmp/wrapper.sh"));
}

#[test]
fn iterm_window_title_is_applescript_escaped() {
    let cmd = TerminalEmulator::ITerm2.build_command(
        "stage \"quoted\" \\ path",
        Path::new("/tmp/test"),
        Path::new("/tmp/wrapper.sh"),
    );
    let script = cmd.get_args().nth(1).unwrap().to_str().unwrap().to_string();
    assert!(script.contains(r#"set name of loomWindow to "stage \"quoted\" \\ path""#));
}

#[test]
fn test_escape_applescript_string() {
    use super::escape_applescript_string;

    // Test basic escaping
    assert_eq!(escape_applescript_string("hello"), "hello");

    // Test quote escaping
    assert_eq!(escape_applescript_string(r#"say "hi""#), r#"say \"hi\""#);

    // Test backslash escaping
    assert_eq!(
        escape_applescript_string(r#"path\to\file"#),
        r#"path\\to\\file"#
    );

    // Test combined escaping
    assert_eq!(
        escape_applescript_string(r#"echo "test\path""#),
        r#"echo \"test\\path\""#
    );

    // Test potential injection attempt
    let malicious = r#""; do shell script "rm -rf /" --"#;
    let escaped = escape_applescript_string(malicious);
    // The quotes should be escaped, preventing breakout
    assert!(escaped.contains(r#"\""#));
    assert!(!escaped.contains(r#"" do"#)); // No unescaped quote followed by space
    assert_eq!(escape_applescript_string("one\ntwo"), "one\\ntwo");
}

#[test]
fn test_terminal_app_escapes_special_chars() {
    let emulator = TerminalEmulator::TerminalApp;
    let workdir = Path::new("/tmp");

    // Test with command containing quotes
    let cmd = emulator.build_command("Test", workdir, Path::new(r#"/tmp/echo "hello""#));
    let args: Vec<_> = cmd.get_args().collect();
    let script = args[1].to_str().unwrap();

    // The quotes in the command should be escaped
    assert!(script.contains(r#"/tmp/echo \"hello\""#));
}

#[test]
fn test_terminal_app_escapes_title() {
    let emulator = TerminalEmulator::TerminalApp;
    let workdir = Path::new("/tmp");

    // Test with title containing quotes (potential injection)
    let cmd = emulator.build_command(r#"Stage "test""#, workdir, Path::new("/tmp/wrapper.sh"));
    let args: Vec<_> = cmd.get_args().collect();
    let script = args[1].to_str().unwrap();

    // The quotes in the title should be escaped
    assert!(script.contains(r#"Stage \"test\""#));
}

#[test]
fn test_iterm2_escapes_special_chars() {
    let emulator = TerminalEmulator::ITerm2;
    let workdir = Path::new("/tmp");

    // Test with command containing backslashes and quotes
    let cmd = emulator.build_command("Test", workdir, Path::new(r#"/tmp/echo "path\to\file""#));
    let args: Vec<_> = cmd.get_args().collect();
    let script = args[1].to_str().unwrap();

    // Both quotes and backslashes should be escaped
    assert!(script.contains(r#"/tmp/echo \"path\\to\\file\""#));
}

#[test]
fn shell_boundary_preserves_adversarial_wrapper_path_as_one_word() {
    let raw = "/tmp/wrapper ' spaced;$(printf injected)\nnext";
    let quoted = shell_path(Path::new(raw));
    let script = format!("set -- {quoted}; printf '%s' \"$#:$1\"");
    let output = Command::new("sh")
        .args(["-c", &script])
        .output()
        .expect("shell parser should run");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("1:{raw}")
    );
}

#[test]
fn test_mate_terminal_escapes_command() {
    let emulator = TerminalEmulator::MateTerminal;
    let workdir = Path::new("/tmp");

    // Test with command containing single quotes (potential injection)
    let cmd = emulator.build_command("Test", workdir, Path::new("/tmp/it's wrapper"));
    let args: Vec<_> = cmd.get_args().collect();
    let last_arg = args.last().unwrap().to_str().unwrap();

    // The single quotes should be escaped to prevent injection
    assert!(last_arg.contains("'\\''"));
    assert!(last_arg.starts_with('\''));
}

#[test]
fn test_xterm_escapes_workdir() {
    let emulator = TerminalEmulator::XTerm;
    // Test with workdir containing single quote (potential injection)
    let workdir = Path::new("/tmp/test's dir");
    let wrapper = Path::new("/tmp/wrapper with spaces;$(touch nope)\n.sh");
    let cmd = emulator.build_command("Test", workdir, wrapper);
    let args: Vec<_> = cmd.get_args().collect();
    let last_arg = args.last().unwrap().to_str().unwrap();

    assert_eq!(Path::new(last_arg), wrapper);
}

#[test]
fn test_xterm_escapes_command() {
    let emulator = TerminalEmulator::XTerm;
    let workdir = Path::new("/tmp");
    // Command with shell metacharacters that could be injection vectors
    let wrapper = Path::new("/tmp/'hello'; rm -rf $(pwd)\nwrapper");
    let cmd = emulator.build_command("Test", workdir, wrapper);
    let args: Vec<_> = cmd.get_args().collect();
    let last_arg = args.last().unwrap().to_str().unwrap();
    assert_eq!(Path::new(last_arg), wrapper);
}

#[cfg(target_os = "macos")]
#[test]
fn test_ghostty_build_command_macos() {
    let emulator = TerminalEmulator::Ghostty;
    let workdir = Path::new("/tmp/test");
    let cmd = emulator.build_command("Test Title", workdir, Path::new("/tmp/wrapper.sh"));

    // On macOS, Ghostty is launched via `open -na` to avoid PATH issues
    assert_eq!(cmd.get_program(), "open");

    let args: Vec<_> = cmd.get_args().collect();
    assert!(args.iter().any(|a| a.to_str().unwrap() == "-na"));
    assert!(args.iter().any(|a| a.to_str().unwrap() == "Ghostty"));
    assert!(args.iter().any(|a| a.to_str().unwrap() == "--args"));
    assert!(args
        .iter()
        .any(|a| a.to_str().unwrap().starts_with("--working-directory=")));
    assert!(args
        .iter()
        .any(|a| a.to_str().unwrap().starts_with("--title=")));
    assert!(args.iter().any(|a| a.to_str().unwrap() == "-e"));
    assert!(args
        .iter()
        .any(|a| a.to_str().unwrap() == "/tmp/wrapper.sh"));
}

#[cfg(not(target_os = "macos"))]
#[test]
fn test_ghostty_build_command_linux() {
    let emulator = TerminalEmulator::Ghostty;
    let workdir = Path::new("/tmp/test");
    let cmd = emulator.build_command("Test Title", workdir, Path::new("/tmp/wrapper.sh"));

    // On Linux, Ghostty is invoked directly
    assert_eq!(cmd.get_program(), "ghostty");

    let args: Vec<_> = cmd.get_args().collect();
    assert_eq!(args[0].to_str().unwrap(), "--title");
    assert_eq!(args[1].to_str().unwrap(), "Test Title");
    assert_eq!(args[2].to_str().unwrap(), "--working-directory");
    assert_eq!(args[3].to_str().unwrap(), "/tmp/test");
    assert_eq!(args[4].to_str().unwrap(), "-e");
    assert_eq!(args[5].to_str().unwrap(), "/tmp/wrapper.sh");
}
