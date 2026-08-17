//! Capsule-focused unit tests for `native/capsule.rs` and the capsule half of
//! `build_claude_command`, split out of `native/tests.rs` to keep both files
//! under the 400-line ceiling (CLAUDE.md Rule 17).

use super::*;
use crate::orchestrator::terminal::native::capsule::{capsule_from, resolved_settings_file};
use serial_test::serial;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn build_claude_command_empty_capsule_matches_legacy_argv() {
    // An unsupported or unprobed capsule must never change the command line.
    let cmd = build_claude_command(
        "/usr/bin/claude",
        "opus",
        "xhigh",
        "auto",
        &SessionCapsule::default(),
        &RemoteControlInvocation::Disabled,
        "'prompt'",
    );
    assert_eq!(
        cmd,
        "/usr/bin/claude --model opus --effort xhigh --permission-mode auto 'prompt'"
    );
}

#[test]
fn build_claude_command_emits_capsule_flags_in_order() {
    let capsule = SessionCapsule {
        settings_path: Some("/w/.claude/settings.local.json".into()),
        setting_sources: Some("user,project".into()),
        strict_mcp_config: true,
        append_system_prompt_file: None,
    };
    let cmd = build_claude_command(
        "/usr/bin/claude",
        "opus",
        "xhigh",
        "auto",
        &capsule,
        &RemoteControlInvocation::Disabled,
        "'prompt'",
    );
    assert_eq!(
        cmd,
        "/usr/bin/claude --model opus --effort xhigh --permission-mode auto --settings /w/.claude/settings.local.json --setting-sources user,project --strict-mcp-config 'prompt'"
    );
    let settings_idx = cmd.find("--settings").unwrap();
    let sources_idx = cmd.find("--setting-sources").unwrap();
    let strict_mcp_idx = cmd.find("--strict-mcp-config").unwrap();
    let prompt_idx = cmd.find("'prompt'").unwrap();
    assert!(
        settings_idx < sources_idx && sources_idx < strict_mcp_idx && strict_mcp_idx < prompt_idx
    );
}

#[test]
fn build_claude_command_escapes_capsule_settings_path() {
    let capsule = SessionCapsule {
        settings_path: Some("/tmp/a b;rm -rf /.json".into()),
        ..SessionCapsule::default()
    };
    let cmd = build_claude_command(
        "/usr/bin/claude",
        "opus",
        "xhigh",
        "auto",
        &capsule,
        &RemoteControlInvocation::Disabled,
        "'prompt'",
    );
    assert!(cmd.contains("--settings '/tmp/a b;rm -rf /.json'"));
    assert!(!cmd.contains("--settings /tmp/a b;rm -rf /.json"));
}

#[test]
fn build_claude_command_capsule_flags_precede_positional_prompt() {
    let capsule = SessionCapsule {
        settings_path: Some("/w/.claude/settings.local.json".into()),
        setting_sources: Some("user,project".into()),
        strict_mcp_config: true,
        append_system_prompt_file: None,
    };
    let cmd = build_claude_command(
        "/usr/bin/claude",
        "opus",
        "xhigh",
        "auto",
        &capsule,
        &RemoteControlInvocation::Bare,
        "'prompt'",
    );
    let capsule_idx = cmd.find("--strict-mcp-config").unwrap();
    let prompt_idx = cmd.find("'prompt'").unwrap();
    let remote_control_idx = cmd.find("--remote-control").unwrap();
    assert!(capsule_idx < prompt_idx && prompt_idx < remote_control_idx);
}

// `capsule_from` is the pure interlock underneath `session_capsule`: it must
// never emit `setting_sources` without also emitting `settings_path`, since
// `--setting-sources` alone (without `--settings` pinning loom's generated
// file) would strip the session's sandbox block, permission rules and hooks.

#[test]
fn capsule_from_sources_supported_but_settings_file_missing_omits_sources() {
    let capsule = capsule_from(true, true, true, false, None, None);
    assert_eq!(capsule.settings_path, None);
    assert_eq!(
        capsule.setting_sources, None,
        "no settings file to pin means --setting-sources must not be emitted either"
    );
}

#[test]
fn capsule_from_sources_supported_and_settings_file_present_pins_user_and_project() {
    let capsule = capsule_from(
        true,
        true,
        true,
        false,
        Some("/w/.claude/settings.local.json".to_string()),
        None,
    );
    assert_eq!(
        capsule.settings_path,
        Some("/w/.claude/settings.local.json".to_string())
    );
    assert_eq!(capsule.setting_sources, Some("user,project".to_string()));
}

#[test]
fn capsule_from_partial_probe_settings_only_omits_sources() {
    // Settings supported but --setting-sources is not: the settings file is
    // still pinned, but the sources flag (which the binary doesn't
    // understand) must not be emitted.
    let capsule = capsule_from(
        true,
        false,
        true,
        false,
        Some("/w/.claude/settings.local.json".to_string()),
        None,
    );
    assert_eq!(
        capsule.settings_path,
        Some("/w/.claude/settings.local.json".to_string())
    );
    assert_eq!(capsule.setting_sources, None);
}

#[test]
fn capsule_from_nothing_supported_yields_empty_capsule() {
    let capsule = capsule_from(
        false,
        false,
        false,
        false,
        Some("/w/.claude/settings.local.json".to_string()),
        Some("/w/signals/prefix/stage.md".to_string()),
    );
    assert_eq!(capsule, SessionCapsule::default());
}

#[test]
fn capsule_from_never_emits_sources_without_a_pinned_settings_path() {
    // The security-critical invariant, checked directly across every corner
    // of the (settings_supported, sources_supported, settings_file) cube.
    for settings_supported in [false, true] {
        for sources_supported in [false, true] {
            for settings_file in [None, Some("/w/.claude/settings.local.json".to_string())] {
                let capsule = capsule_from(
                    settings_supported,
                    sources_supported,
                    true,
                    false,
                    settings_file,
                    None,
                );
                assert!(
                    capsule.setting_sources.is_none() || capsule.settings_path.is_some(),
                    "setting_sources.is_some() must imply settings_path.is_some(): {capsule:?}"
                );
            }
        }
    }
}

#[test]
fn build_claude_command_emits_append_system_prompt_file_flag() {
    let capsule = SessionCapsule {
        append_system_prompt_file: Some("/w/signals/prefix/my-stage.md".into()),
        ..SessionCapsule::default()
    };
    let cmd = build_claude_command(
        "/usr/bin/claude",
        "opus",
        "xhigh",
        "auto",
        &capsule,
        &RemoteControlInvocation::Disabled,
        "'prompt'",
    );
    assert!(cmd.contains("--append-system-prompt-file /w/signals/prefix/my-stage.md"));
    let flag_idx = cmd.find("--append-system-prompt-file").unwrap();
    let prompt_idx = cmd.find("'prompt'").unwrap();
    assert!(
        flag_idx < prompt_idx,
        "capsule flags must precede the positional prompt: {cmd}"
    );
}

#[test]
fn build_claude_command_escapes_append_system_prompt_file_path() {
    let capsule = SessionCapsule {
        append_system_prompt_file: Some("/tmp/a b;rm -rf /.md".into()),
        ..SessionCapsule::default()
    };
    let cmd = build_claude_command(
        "/usr/bin/claude",
        "opus",
        "xhigh",
        "auto",
        &capsule,
        &RemoteControlInvocation::Disabled,
        "'prompt'",
    );
    assert!(cmd.contains("--append-system-prompt-file '/tmp/a b;rm -rf /.md'"));
    assert!(!cmd.contains("--append-system-prompt-file /tmp/a b;rm -rf /.md"));
}

// `append_system_prompt_file` follows the SAME interlock discipline as
// `settings_path`/`setting_sources`: `Some` only when the flag is supported
// AND a real path was resolved upstream (native::launch, gated on the
// `prompt_cache_split` config key).
#[test]
fn capsule_from_append_system_prompt_file_requires_support_and_a_resolved_path() {
    let supported_and_resolved = capsule_from(
        false,
        false,
        false,
        true,
        None,
        Some("/w/signals/prefix/my-stage.md".to_string()),
    );
    assert_eq!(
        supported_and_resolved.append_system_prompt_file,
        Some("/w/signals/prefix/my-stage.md".to_string())
    );

    let supported_but_unresolved = capsule_from(false, false, false, true, None, None);
    assert_eq!(supported_but_unresolved.append_system_prompt_file, None);

    let resolved_but_unsupported = capsule_from(
        false,
        false,
        false,
        false,
        None,
        Some("/w/signals/prefix/my-stage.md".to_string()),
    );
    assert_eq!(resolved_but_unsupported.append_system_prompt_file, None);
}

// `resolved_settings_file` is what `session_capsule` calls to build the
// `--settings` path. It must absolutize `cwd` before probing for and
// returning the settings file: the wrapper script `cd`s into the working
// directory before `exec`ing claude (see `wrapper::absolute`'s doc comment),
// so a relative `--settings` value would resolve against that directory
// instead of the daemon's cwd, and claude would exit with "Settings file
// not found" even though the file exists. `set_current_dir` is
// process-global, so both tests run `#[serial]` and restore the original
// cwd afterward.

#[test]
#[serial]
fn resolved_settings_file_absolutizes_a_relative_cwd() {
    let temp = TempDir::new().unwrap();
    let worktree = temp.path().join("wt");
    std::fs::create_dir_all(worktree.join(".claude")).unwrap();
    let settings_path = worktree.join(".claude").join("settings.local.json");
    std::fs::write(&settings_path, "{}").unwrap();

    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();
    let result = resolved_settings_file(Path::new("./wt"));
    std::env::set_current_dir(&original_cwd).unwrap();

    let resolved = result.expect("settings file exists and must be found");
    let resolved_path = Path::new(&resolved);
    assert!(resolved_path.is_absolute(), "must be absolute: {resolved}");
    assert_eq!(
        resolved_path.canonicalize().unwrap(),
        settings_path.canonicalize().unwrap()
    );
}

#[test]
#[serial]
fn resolved_settings_file_missing_file_yields_none() {
    let temp = TempDir::new().unwrap();
    let worktree = temp.path().join("wt");
    std::fs::create_dir_all(&worktree).unwrap();

    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();
    let result = resolved_settings_file(Path::new("./wt"));
    std::env::set_current_dir(&original_cwd).unwrap();

    assert_eq!(result, None);
}
