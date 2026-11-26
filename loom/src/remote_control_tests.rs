use super::*;
use serial_test::serial;
use std::ffi::OsString;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::mpsc::channel;

#[test]
fn default_config_mode_is_auto() {
    let config = RemoteControlConfig::default();
    assert_eq!(config.mode, RemoteControlMode::Auto);
}

#[test]
fn default_mode_is_auto() {
    assert_eq!(RemoteControlMode::default(), RemoteControlMode::Auto);
}

#[test]
fn config_round_trips_through_toml() {
    let config = RemoteControlConfig {
        mode: RemoteControlMode::Off,
    };
    let rendered = toml::to_string(&config).unwrap();
    assert!(rendered.contains("off"), "rendered: {rendered}");
    let parsed: RemoteControlConfig = toml::from_str(&rendered).unwrap();
    assert_eq!(parsed, config);
}

#[test]
fn missing_mode_defaults_to_auto() {
    let parsed: RemoteControlConfig = toml::from_str("").unwrap();
    assert_eq!(parsed.mode, RemoteControlMode::Auto);
}

#[test]
fn parse_version_handles_plain_and_noisy() {
    assert_eq!(parse_version("2.1.51"), Some((2, 1, 51)));
    assert_eq!(parse_version("2.1.51 (Claude Code)"), Some((2, 1, 51)));
    assert_eq!(parse_version("v10.20.30"), Some((10, 20, 30)));
    assert_eq!(parse_version("not a version"), None);
    assert_eq!(parse_version("2.1"), None);
}

#[test]
fn version_supported_covers_boundaries() {
    // Exact minimum supported version.
    assert!(version_supported(MIN_REMOTE_CONTROL_VERSION));
    assert!(version_supported((2, 1, 51)));
    // One patch below the minimum — unsupported.
    assert!(!version_supported((2, 1, 50)));
    // Newer patch / minor / major — all supported.
    assert!(version_supported((2, 1, 52)));
    assert!(version_supported((2, 2, 0)));
    assert!(version_supported((3, 0, 0)));
    // Older minor / major — unsupported.
    assert!(!version_supported((2, 0, 99)));
    assert!(!version_supported((1, 9, 9)));
}

#[test]
fn status_is_enabled_reports_correctly() {
    assert!(RemoteControlStatus::Enabled.is_enabled());
    assert!(!RemoteControlStatus::Disabled {
        reason: "x".to_string()
    }
    .is_enabled());
}

#[test]
fn supports_remote_control_false_for_missing_binary() {
    // A path that does not exist must fail closed.
    assert!(!claude_supports_remote_control(Path::new(
        "/nonexistent/claude-binary-xyz"
    )));
}

#[test]
#[serial]
fn resolve_false_when_mode_off() {
    let temp = tempfile::TempDir::new().unwrap();
    let work_dir = temp.path();
    crate::fs::work_dir::write_remote_control_config(
        work_dir,
        &RemoteControlConfig {
            mode: RemoteControlMode::Off,
        },
    )
    .unwrap();
    assert!(!resolve(work_dir));
}

#[test]
#[serial]
fn disable_for_this_process_latches_and_keeps_the_first_reason() {
    reset_disabled_for_process();
    let temp = tempfile::TempDir::new().unwrap();

    disable_for_this_process("first reason");
    disable_for_this_process("second reason");
    assert!(!resolve(temp.path()));
    assert_eq!(
        DISABLED_FOR_PROCESS.lock().unwrap().as_deref(),
        Some("first reason")
    );

    reset_disabled_for_process();
    assert!(!disabled_for_process());
}

#[test]
fn help_indicates_named_arg_detects_optional_name() {
    let help = "Usage: claude [options] [prompt]\n\
                \x20 --permission-mode <mode>  Permission mode\n\
                \x20 --remote-control [name]   Enable remote control\n";
    assert!(help_indicates_named_arg(help));
}

#[test]
fn help_indicates_named_arg_false_without_optional_name() {
    // Older claude: the flag exists but takes no argument.
    let help = "  --remote-control        Enable remote control\n";
    assert!(!help_indicates_named_arg(help));
}

#[test]
fn probe_named_arg_support_false_for_missing_binary() {
    // A path that does not exist must fail closed (bare flag).
    assert!(!probe_named_arg_support(Path::new(
        "/nonexistent/claude-binary-xyz"
    )));
}

#[test]
#[serial]
fn resolve_invocation_disabled_when_mode_off() {
    let temp = tempfile::TempDir::new().unwrap();
    let work_dir = temp.path();
    crate::fs::work_dir::write_remote_control_config(
        work_dir,
        &RemoteControlConfig {
            mode: RemoteControlMode::Off,
        },
    )
    .unwrap();
    assert_eq!(
        resolve_invocation(work_dir, "anything"),
        RemoteControlInvocation::Disabled
    );
}

fn logged_in(method: &str) -> AuthProbe {
    AuthProbe::LoggedIn {
        method: method.to_string(),
    }
}

#[test]
fn eligibility_accepts_only_a_claude_ai_login() {
    assert!(eligibility_from(&logged_in("claude.ai")).is_ok());
}

#[test]
fn eligibility_rejects_other_logins_and_unknown_verdicts() {
    let console = eligibility_from(&logged_in("console")).unwrap_err();
    assert!(console.to_string().contains("console"), "{console}");
    assert!(console.to_string().contains("claude.ai"), "{console}");

    let logged_out = eligibility_from(&AuthProbe::NotLoggedIn).unwrap_err();
    assert!(logged_out.to_string().contains("not logged in"));

    let unknown = eligibility_from(&AuthProbe::Unknown("timed out after 30s".into())).unwrap_err();
    assert!(unknown.to_string().contains("timed out after 30s"));
}

#[test]
fn eligibility_errors_never_carry_an_email() {
    let probes = [
        logged_in("console"),
        AuthProbe::NotLoggedIn,
        AuthProbe::Unknown("unparseable auth status".into()),
    ];
    for probe in probes {
        let text = eligibility_from(&probe).unwrap_err().to_string();
        assert!(!text.contains('@'), "{text}");
    }
}

#[test]
fn eligibility_reason_carries_only_a_bounded_plain_auth_method() {
    let hostile = format!(
        "con\nsole\u{1b}[31m <a href=\"x\">{}",
        "x".repeat(MAX_METHOD_CHARS * 2)
    );
    let reason = eligibility_from(&logged_in(&hostile))
        .unwrap_err()
        .to_string();

    assert_eq!(reason.lines().count(), 1, "{reason:?}");
    assert!(!reason.contains('\u{1b}'), "{reason:?}");
    assert!(!reason.contains('<'), "{reason:?}");
    assert!(
        reason.contains("logged in with console31mahrefx"),
        "{reason:?}"
    );
    assert_eq!(method_label(&hostile).chars().count(), MAX_METHOD_CHARS);
    assert_eq!(method_label("claude.ai-2_x"), "claude.ai-2_x");
}

#[test]
#[serial]
fn a_poisoned_latch_still_latches() {
    DISABLED_FOR_PROCESS.clear_poison();
    reset_disabled_for_process();
    let poisoner = std::thread::spawn(|| {
        let _held = DISABLED_FOR_PROCESS.lock().unwrap();
        panic!("poison the latch");
    });
    assert!(poisoner.join().is_err());

    assert!(!disabled_for_process());
    disable_for_this_process("x");
    assert!(disabled_for_process());

    DISABLED_FOR_PROCESS.clear_poison();
    reset_disabled_for_process();
}

/// Writes an executable `claude` stand-in into `dir` that reports `version`.
fn fake_claude(dir: &Path, version: &str) -> PathBuf {
    let script = format!(
        "#!/bin/sh\n\
         case \"$1\" in\n\
         --version) echo \"{version} (Claude Code)\" ;;\n\
         --help) echo \"  --remote-control [name]\" ;;\n\
         auth) echo '{{\"loggedIn\": true, \"authMethod\": \"claude.ai\"}}' ;;\n\
         esac\n"
    );
    let path = dir.join("claude");
    // Replace through a rename so a running copy never sees a half-written script, and retry
    // once when another thread's fork still holds the previous file open for writing.
    let staged = dir.join("claude.new");
    for attempt in 0..2 {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o755)
            .open(&staged)
            .unwrap();
        file.write_all(script.as_bytes()).unwrap();
        file.sync_all().unwrap();
        drop(file);
        match std::fs::rename(&staged, &path) {
            Ok(()) => break,
            Err(error) if attempt == 0 => drop(error),
            Err(error) => panic!("replacing the fake claude: {error}"),
        }
    }
    path
}

/// Puts one directory first on `PATH` for the life of the guard. The other entries stay: a test
/// on another thread resolves its own programs from the same `PATH`.
struct PathGuard(Option<OsString>);

impl PathGuard {
    fn first(dir: &Path) -> Self {
        let previous = std::env::var_os("PATH");
        let entries = std::iter::once(dir.to_path_buf())
            .chain(previous.iter().flat_map(std::env::split_paths));
        std::env::set_var("PATH", std::env::join_paths(entries).unwrap());
        Self(previous)
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        match self.0.take() {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
    }
}

#[test]
#[serial]
fn resolve_follows_a_replaced_binary() {
    let bin = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    fake_claude(bin.path(), "2.1.60");
    let _path = PathGuard::first(bin.path());
    reset_disabled_for_process();

    assert!(resolve(work_dir.path()));
    fake_claude(bin.path(), "2.0.0");
    assert!(!resolve(work_dir.path()));
    fake_claude(bin.path(), "2.1.60");
    assert!(resolve(work_dir.path()));
}

#[test]
#[serial]
fn preflight_reports_the_replaced_version() {
    let bin = tempfile::tempdir().unwrap();
    let path = fake_claude(bin.path(), "2.1.60");
    let _path = PathGuard::first(bin.path());

    assert_eq!(preflight(&path), RemoteControlStatus::Enabled);
    fake_claude(bin.path(), "2.0.0");
    match preflight(&path) {
        RemoteControlStatus::Disabled { reason } => assert!(reason.contains("2.0.0"), "{reason}"),
        RemoteControlStatus::Enabled => panic!("a replaced, older claude still passes"),
    }
}

#[test]
fn memo_reprobes_on_expiry_key_change_and_survives_a_panicking_probe() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("binary");
    std::fs::write(&file, "one").unwrap();
    let ttl = Duration::from_secs(60);
    let start = Instant::now();
    let memo: Memo<u32> = Memo::new();

    assert_eq!(memo.get(&file, Some(ttl), start, || 1), 1);
    assert_eq!(memo.get(&file, Some(ttl), start + ttl / 2, || 2), 1);
    let later = start + ttl + Duration::from_secs(1);
    assert_eq!(memo.get(&file, Some(ttl), later, || 3), 3);

    let forever: Memo<u32> = Memo::new();
    assert_eq!(forever.get(&file, None, start, || 10), 10);
    assert_eq!(forever.get(&file, None, later, || 11), 10);
    std::fs::write(&file, "longer content").unwrap();
    assert_eq!(forever.get(&file, None, later, || 12), 12);

    let (sent, _received) = channel::<()>();
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _keep = &sent;
        forever.get(&dir.path().join("other"), None, later, || panic!("probe"))
    }));
    assert!(panicked.is_err());
    assert_eq!(forever.get(&file, None, later, || 13), 12);
    assert_eq!(
        forever.get(&dir.path().join("other"), None, later, || 14),
        14
    );
}
