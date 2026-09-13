//! Capsule-focused unit tests for `native/capsule.rs` and the capsule half of
//! `build_claude_command`, split out of `native/tests.rs` to keep both files
//! under the 400-line ceiling (CLAUDE.md Rule 17), and for the write denies a
//! session's settings capsule carries (`session_settings.rs`), on the fixture
//! `tests_session_settings.rs` builds.

use super::tests_session_settings::{
    checkout, read_capsule, sandbox_with, strings, write_capsule, Checkout, ALL_KINDS,
};
use super::*;
use crate::models::stage::Implementer;
use crate::orchestrator::terminal::native::capsule::capsule_from;
use std::collections::BTreeSet;

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

/// Plan section 10's write denies, whitespace-separated, with `{R}` the
/// repository, `{T}` the stage worktree and `{H}` the hooks directory: those
/// every capsule carries, then `[denyWrite, Edit]` for each location.
const SHARED_DENY_WRITE: &str = "{R}/.loom {R}/.git/hooks {R}/.git/config {H} \
    ~/.claude/hooks ~/.claude/settings.json ~/.claude.json ~/.loom ~/.claude/projects \
    ~/.claude/agents ~/.claude/skills ~/.claude/commands ~/.claude/loom-skill-catalog \
    ~/.claude/plugins ~/.codex/hooks ~/.codex/hooks.json ~/.codex/config.toml";
const SHARED_EDIT_DENIES: &str = "Edit(/{R}/.loom/**) Edit(.loom/**) \
    Edit(/{R}/.git/hooks/**) Edit(/{R}/.git/config) Edit(/{H}/**) Edit(~/.claude/hooks/**) \
    Edit(~/.claude/settings.json) Edit(~/.claude.json) Edit(~/.loom/**) \
    Edit(~/.claude/projects/**) Edit(~/.claude/agents/**) Edit(~/.claude/skills/**) \
    Edit(~/.claude/commands/**) Edit(~/.claude/loom-skill-catalog/**) \
    Edit(~/.claude/plugins/**) Edit(~/.codex/hooks/**) Edit(~/.codex/hooks.json) \
    Edit(~/.codex/config.toml)";
const CHECKOUT_DENIES: [&str; 2] = [
    "{R}/.worktrees {R}/.claude",
    "Edit(/{R}/.worktrees/**) Edit(.worktrees/**) Edit(/{R}/.claude/**) Edit(.claude/**)",
];
const WORKTREE_DENIES: [&str; 2] = [
    "{T}/.loom {T}/.claude",
    "Edit(/{T}/.claude/**) Edit(.claude/**)",
];

/// The whitespace-separated entries of `templates`, with `{R}`, `{T}` and
/// `{H}` spelled out as `checkout`'s repository, worktree and hooks directory.
fn expand(templates: &[&str], checkout: &Checkout) -> BTreeSet<String> {
    let [r, t, h] = [&checkout.repo, &checkout.worktree, &checkout.hooks_dir]
        .map(|path| path.display().to_string());
    let entries = templates
        .iter()
        .flat_map(|template| template.split_whitespace());
    entries
        .map(|entry| {
            entry
                .replace("{R}", &r)
                .replace("{T}", &t)
                .replace("{H}", &h)
        })
        .collect()
}

#[test]
fn every_kind_gets_exactly_the_section_10_write_denies_for_its_location() {
    let checkout = checkout();
    let hooks = Some(checkout.hooks_dir.as_path());
    let sandbox = sandbox_with(vec![Implementer::Claude]);
    let locations = [
        (&checkout.repo, CHECKOUT_DENIES),
        (&checkout.worktree, WORKTREE_DENIES),
    ];
    for kind in ALL_KINDS {
        for (cwd, [own_deny_write, own_edit]) in locations {
            let path = write_capsule(&checkout, "session-d1", kind, cwd, &sandbox, hooks);
            let capsule = read_capsule(&path.unwrap());
            let deny_write = expand(&[SHARED_DENY_WRITE, own_deny_write], &checkout);
            let edit = expand(&[SHARED_EDIT_DENIES, own_edit], &checkout);
            let label = format!("{kind} in {}", cwd.display());
            let written = strings(&capsule, "/sandbox/filesystem/denyWrite");
            assert_eq!(written, deny_write, "{label}");
            assert_eq!(strings(&capsule, "/permissions/deny"), edit, "{label}");
        }
    }
}

#[test]
fn the_codex_lane_capsule_keeps_its_grants_and_denies_every_plugin_entry_beside_them() {
    let checkout = checkout();
    let plugins = checkout.home.join(".claude").join("plugins");
    for dir in ["cache", "data/codex-openai-codex", "data/other"] {
        std::fs::create_dir_all(plugins.join(dir)).unwrap();
    }
    std::fs::write(plugins.join("installed_plugins.json"), "{}").unwrap();
    let sandbox = sandbox_with(vec![Implementer::Claude, Implementer::Codex]);
    let (stage, cwd) = (SessionType::Stage, &checkout.worktree);
    let hooks = Some(checkout.hooks_dir.as_path());

    let path = write_capsule(&checkout, "session-cx1", stage, cwd, &sandbox, hooks);

    let capsule = read_capsule(&path.unwrap());
    let allow_write = strings(&capsule, "/sandbox/filesystem/allowWrite");
    let deny_write = strings(&capsule, "/sandbox/filesystem/denyWrite");
    let deny = strings(&capsule, "/permissions/deny");
    for grant in ["~/.codex", "~/.claude/plugins/data/codex-openai-codex"] {
        assert!(allow_write.contains(grant), "{grant}: {allow_write:?}");
        let covers = |path: &String| path == grant || grant.starts_with(&format!("{path}/"));
        assert!(!deny_write.iter().any(covers), "{grant}: {deny_write:?}");
    }
    for surface in [
        ".codex/hooks/**",
        ".codex/hooks.json",
        ".codex/config.toml",
        ".claude/plugins/cache/**",
        ".claude/plugins/data/other/**",
        ".claude/plugins/installed_plugins.json",
    ] {
        let path = format!("~/{}", surface.trim_end_matches("/**"));
        assert!(deny_write.contains(&path), "{path}: {deny_write:?}");
        let rule = format!("Edit(~/{surface})");
        assert!(deny.contains(&rule), "{rule}: {deny:?}");
    }
}
