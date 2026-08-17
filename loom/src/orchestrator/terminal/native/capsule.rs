//! The session capsule: the `claude` flags loom pins a spawned session to.
//!
//! Split out of `native/mod.rs` to keep that module under its line ceiling.
//! The flag-emitting code (`build_claude_command`) deliberately stays in
//! `native/mod.rs`; only the capsule's construction and its support probe live
//! here.

use std::path::Path;
use std::sync::OnceLock;

/// The configuration a loom-spawned session is pinned to, expressed as
/// `claude` CLI flags rather than trusted to ambient settings discovery.
///
/// `user,project` drops only the `local` scope: Claude Code applies the main
/// repository's `.claude/settings.local.json` to sessions running in linked
/// worktrees, which is the actual cross-repository leak. Pinning loom's
/// generated local file explicitly via `--settings` and dropping `local`
/// closes that leak while keeping the repository's committed
/// `.claude/settings.json` policy in force.
///
/// `user` is deliberately RETAINED, not dropped alongside `local`:
/// `--setting-sources project` alone would also silence
/// `~/.claude/settings.json`, which breaks a user-scope codex plugin install
/// (`doc/loom/knowledge/architecture/codex-plugin.md` recommends `--scope
/// user`, and its documented install command defaults to it) and
/// `apiKeyHelper`-based authentication, plus any user `env`, model
/// selection, statusline, or user-authored hooks. None of that is loom's to
/// take away.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct SessionCapsule {
    /// `--settings <path>`: the settings file loom generated for this session.
    pub settings_path: Option<String>,
    /// `--setting-sources <list>`: which settings scopes may load at all.
    pub setting_sources: Option<String>,
    /// `--strict-mcp-config`: load no MCP servers other than those passed on
    /// the command line (loom passes none).
    pub strict_mcp_config: bool,
    /// `--append-system-prompt-file <path>`: the stage's immutable stable
    /// prefix, handed over separately so the volatile part of the signal
    /// stops invalidating its cache entry. `None` unless the split is
    /// explicitly enabled AND the installed claude supports the flag.
    pub append_system_prompt_file: Option<String>,
}

fn probed_capsule_support(claude_path: &Path) -> (bool, bool, bool, bool) {
    static CACHE: OnceLock<(bool, bool, bool, bool)> = OnceLock::new();
    *CACHE.get_or_init(|| {
        let output = match std::process::Command::new(claude_path)
            .arg("--help")
            .output()
        {
            Ok(output) if output.status.success() => output,
            Ok(_) | Err(_) => return (false, false, false, false),
        };
        let help = String::from_utf8_lossy(&output.stdout);
        (
            help.contains("--settings"),
            help.contains("--setting-sources"),
            help.contains("--strict-mcp-config"),
            help.contains("--append-system-prompt-file"),
        )
    })
}

/// Assemble a [`SessionCapsule`] from already-resolved probe/filesystem
/// facts. Pure and total, so the security-critical interlock below is
/// directly unit-testable without a subprocess `--help` probe or a real
/// filesystem (see `native/tests_capsule.rs`).
///
/// The interlock: `setting_sources` is `Some` ONLY when `settings_path` is
/// also `Some`. Emitting `--setting-sources` without `--settings` would
/// strip the session's entire sandbox block, permission rules and hooks —
/// `--setting-sources user,project` on its own says nothing about WHICH
/// file loom generated, so the settings-scoped flag must never be emitted
/// unless the settings-path flag is emitted alongside it.
///
/// The same discipline applies to `append_system_prompt_file`: it is `Some`
/// ONLY when the installed claude's `--help` advertised the flag AND the
/// caller resolved a real prefix-file path (i.e. the prompt-cache split is
/// both supported and explicitly enabled — see `native::launch`).
pub(super) fn capsule_from(
    settings_supported: bool,
    sources_supported: bool,
    strict_supported: bool,
    append_system_prompt_file_supported: bool,
    settings_file: Option<String>,
    append_system_prompt_file: Option<String>,
) -> SessionCapsule {
    let settings_path = settings_file.filter(|_| settings_supported);

    // `user,project` drops only the local scope, which leaks the main
    // repository's settings.local.json into linked worktrees. `user` is
    // deliberately retained (see the `SessionCapsule` doc comment). Narrowing
    // is safe only when `--settings` explicitly pins loom's generated local
    // file; otherwise it would strip the sandbox block, permission rules,
    // and hooks entirely.
    let setting_sources =
        (sources_supported && settings_path.is_some()).then(|| "user,project".to_string());

    let append_system_prompt_file =
        append_system_prompt_file.filter(|_| append_system_prompt_file_supported);

    SessionCapsule {
        settings_path,
        setting_sources,
        strict_mcp_config: strict_supported,
        append_system_prompt_file,
    }
}

/// Build the capsule for a session whose working directory is `cwd`.
///
/// `append_system_prompt_file` is the stable-prefix file path already
/// resolved by the caller (`Some` only when the prompt-cache split is
/// enabled and the file was written successfully); it is dropped here if the
/// installed claude does not support `--append-system-prompt-file`.
pub(crate) fn session_capsule(
    claude_path: &Path,
    cwd: &Path,
    append_system_prompt_file: Option<String>,
) -> SessionCapsule {
    let (
        settings_supported,
        setting_sources_supported,
        strict_mcp_supported,
        append_system_prompt_file_supported,
    ) = probed_capsule_support(claude_path);

    let settings_file = cwd.join(".claude").join("settings.local.json");
    let settings_file = settings_file
        .is_file()
        .then(|| settings_file.to_str().map(str::to_owned))
        .flatten();

    capsule_from(
        settings_supported,
        setting_sources_supported,
        strict_mcp_supported,
        append_system_prompt_file_supported,
        settings_file,
        append_system_prompt_file,
    )
}
