//! The session capsule: the `claude` flags loom pins a spawned session to.
//!
//! Construction and support probing live here; `native::build_claude_command`
//! emits the flags.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::{bail, Context, Result};

/// The configuration a loom-spawned session is pinned to, expressed as
/// `claude` CLI flags rather than trusted to ambient settings discovery.
///
/// `user,project` drops only the `local` scope: Claude Code applies the main
/// repository's `.claude/settings.local.json` to sessions running in linked
/// worktrees, which is the actual cross-repository leak. Pinning loom's
/// generated per-session capsule explicitly via `--settings` and dropping
/// `local` closes that leak while keeping the repository's committed
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

/// Which capsule flags the installed claude's `--help` advertises.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CapsuleSupport {
    pub settings: bool,
    pub setting_sources: bool,
    pub strict_mcp_config: bool,
    pub append_system_prompt_file: bool,
}

/// Probe once after success. Failed probes are retried on the next launch.
pub(super) fn probed_capsule_support(claude_path: &Path) -> Result<CapsuleSupport> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CapsuleSupport>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(entries) = cache.lock() {
        if let Some(support) = entries.get(claude_path) {
            return Ok(*support);
        }
    }
    let support = probe_capsule_support(claude_path)?;
    if let Ok(mut entries) = cache.lock() {
        entries.insert(claude_path.to_path_buf(), support);
    }
    Ok(support)
}

pub(super) fn probe_capsule_support(claude_path: &Path) -> Result<CapsuleSupport> {
    let output = crate::process::run_bounded_output(
        std::process::Command::new(claude_path).arg("--help"),
        Duration::from_secs(5),
        "Claude capsule capability probe",
    )
    .with_context(|| {
        format!(
            "cannot probe Claude capsule flags at {}",
            claude_path.display()
        )
    })?;
    if !output.status.success() {
        bail!(
            "Claude capsule capability probe at {} exited with {}; stderr: {}",
            claude_path.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let help = String::from_utf8_lossy(&output.stdout);
    let support = CapsuleSupport {
        settings: help.contains("--settings"),
        setting_sources: help.contains("--setting-sources"),
        strict_mcp_config: help.contains("--strict-mcp-config"),
        append_system_prompt_file: help.contains("--append-system-prompt-file"),
    };
    support.require_isolation()?;
    Ok(support)
}

impl CapsuleSupport {
    pub(super) fn require_isolation(self) -> Result<()> {
        let mut missing = Vec::new();
        if !self.settings {
            missing.push("--settings");
        }
        if !self.setting_sources {
            missing.push("--setting-sources");
        }
        if !self.strict_mcp_config {
            missing.push("--strict-mcp-config");
        }
        if !missing.is_empty() {
            bail!(
                "Claude lacks required session isolation flags: {}; update the Claude executable before launching a session",
                missing.join(", ")
            );
        }
        Ok(())
    }
}

impl SessionCapsule {
    pub(super) fn require_isolation(&self, expected_settings: &str) -> Result<()> {
        if self.settings_path.as_deref() != Some(expected_settings)
            || self.setting_sources.as_deref() != Some("user,project")
            || !self.strict_mcp_config
        {
            bail!("Claude session invocation lacks the generated settings capsule or required isolation flags; refusing to spawn");
        }
        Ok(())
    }
}

/// Assemble a [`SessionCapsule`] from already-resolved probe/filesystem
/// facts. Pure and total for command-rendering tests; launch preparation
/// independently requires all isolation flags before calling this and checks
/// the resulting capsule before creating a wrapper.
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
    // is safe only when `--settings` explicitly pins loom's generated
    // capsule; otherwise it would strip the sandbox block, permission rules,
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

/// Build the capsule from the session's generated settings file
/// (`native::session_settings::write_session_capsule`) and the flags the
/// installed claude supports ([`probed_capsule_support`], resolved once per
/// launch so tests can supply their own), through the interlock in
/// [`capsule_from`].
///
/// `append_system_prompt_file` is the stable-prefix file path already
/// resolved by the caller (`Some` only when the prompt-cache split is
/// enabled and the file was written successfully); it is dropped here if the
/// installed claude does not support `--append-system-prompt-file`.
pub(crate) fn session_capsule(
    support: CapsuleSupport,
    settings_file: Option<String>,
    append_system_prompt_file: Option<String>,
) -> SessionCapsule {
    capsule_from(
        support.settings,
        support.setting_sources,
        support.strict_mcp_config,
        support.append_system_prompt_file,
        settings_file,
        append_system_prompt_file,
    )
}
