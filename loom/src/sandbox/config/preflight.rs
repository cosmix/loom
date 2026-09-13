//! The sandbox preflight shared by `loom run` and every spawn
//! (`doc/plans/PLAN-loom-state-confinement.md`, sections 11 and 12).
//!
//! Check 1 is `validate_config` over each stage's merged sandbox config.
//! Checks 2-4 look at the host a session's hooks run on: the installed hook
//! scripts must match this build, and neither `LOOM_BIN`, the hooks directory
//! nor any tool the hooks call through the filtered PATH may resolve under a
//! root a session can write. `loom run` refuses on checks 1-5, and the daemon
//! repeats checks 1-4 at each spawn. Check 5 and the `R/loom-hooks/**` rule
//! live in `hook_commands`; `local_settings` names the keys loom used to write
//! into the operator's `R/.claude/settings.local.json`.
//!
//! Every check takes its host facts as parameters; only the scripts, tools
//! and hook executables those facts name are read from disk.

mod hook_commands;
mod local_settings;

pub(crate) use hook_commands::{hook_documents, scan_hook_registrations, HookFindings, HookScan};
pub(crate) use local_settings::{settings_local_loom_keys, strip_settings_local_loom_keys};

use std::fmt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::{merge_config, validate_config, MergedSandboxConfig};
use crate::models::stage::Stage;
use crate::plan::schema::{PermissionMode, SandboxConfig, StageSandboxConfig};

/// The shell every loom hook runs under.
const HOOK_SHELL: &str = "/bin/bash";

/// The tools loom's hooks call through `LOOM_HOOK_PATH` (`gtimeout` is
/// `timeout` on macOS).
const HOOK_TOOLS: [&str; 16] = [
    "jq", "git", "timeout", "gtimeout", "rg", "mkdir", "mv", "cat", "sed", "awk", "tr", "date",
    "stat", "dd", "head", "tail",
];

/// One or more preflight refusals. A spawn that fails with this error is
/// recorded as a sandbox setup failure and never retried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SandboxPreflightRefusal {
    refusals: Vec<String>,
}

impl SandboxPreflightRefusal {
    pub(crate) fn new(refusals: Vec<String>) -> Self {
        Self { refusals }
    }

    /// `Ok` when `refusals` is empty, the refusal otherwise.
    pub(crate) fn check(refusals: Vec<String>) -> Result<(), Self> {
        if refusals.is_empty() {
            Ok(())
        } else {
            Err(Self::new(refusals))
        }
    }
}

impl fmt::Display for SandboxPreflightRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.refusals.as_slice() {
            [only] => f.write_str(only),
            all => {
                write!(f, "the sandbox preflight refused {} check(s):", all.len())?;
                all.iter()
                    .try_for_each(|refusal| write!(f, "\n  - {refusal}"))
            }
        }
    }
}

impl std::error::Error for SandboxPreflightRefusal {}

/// The host facts checks 2-4 read, resolved once per launch or per
/// `loom run` (`native::launch`).
#[derive(Debug, Clone)]
pub(crate) struct HostFacts {
    /// Every root a session could write (`session_writable_roots`).
    pub writable_roots: Vec<PathBuf>,
    /// The loom hooks directory, canonical and operator-owned; `None` when
    /// none is installed or it failed verification.
    pub hooks_dir: Option<PathBuf>,
    /// The loom binary every hook runs as `LOOM_BIN`.
    pub loom_bin: PathBuf,
    /// The PATH entries exported as `LOOM_HOOK_PATH`.
    pub hook_path: Vec<PathBuf>,
}

/// Check 1 over every stage: its merged sandbox config must pass
/// `validate_config`. Each refusal names the stage and whether the refused
/// setting came from the plan's `sandbox:` block or the stage's own override.
pub(crate) fn sandbox_policy_refusals(plan: &SandboxConfig, stages: &[Stage]) -> Vec<String> {
    stages
        .iter()
        .filter_map(|stage| {
            let merged = merge_config(plan, &stage.sandbox, stage.stage_type, &stage.implementers);
            let error = validate_config(&merged).err()?;
            let origin = policy_origin(&stage.sandbox, &merged);
            Some(format!("stage `{}`: {error} (set by {origin})", stage.id))
        })
        .collect()
}

/// Where the setting `validate_config` refused came from, tested in the order
/// it tests them.
fn policy_origin(stage: &StageSandboxConfig, merged: &MergedSandboxConfig) -> &'static str {
    let set_by_stage = if merged.permission_mode == PermissionMode::BypassPermissions {
        stage.permission_mode.is_some()
    } else if !merged.enabled {
        stage.enabled.is_some()
    } else {
        stage.allow_unsandboxed_escape.is_some()
    };
    if set_by_stage {
        "the stage's own `sandbox:` override"
    } else {
        "the plan's `sandbox:` block"
    }
}

/// Checks 2-4 over `facts`, one message per refusal.
pub(crate) fn host_refusals(facts: &HostFacts) -> Vec<String> {
    let roots = WritableRoots::new(&facts.writable_roots);
    let mut refusals = hook_script_refusals(facts.hooks_dir.as_deref());
    refusals.extend(install_refusals(facts, &roots));
    refusals.extend(tool_refusals(&facts.hook_path, &roots));
    refusals
}

/// Checks 2-4 as the error a spawn fails with.
pub(crate) fn require_confined_host(facts: &HostFacts) -> Result<(), SandboxPreflightRefusal> {
    SandboxPreflightRefusal::check(host_refusals(facts))
}

/// Check 2: every installed hook script, `loom-relay.sh` included, is present
/// and matches the copy embedded in this build.
fn hook_script_refusals(hooks_dir: Option<&Path>) -> Vec<String> {
    let Some(dir) = hooks_dir else {
        return vec![
            "no verified loom hooks directory: neither LOOM_HOOKS_DIR nor ~/.claude/hooks/loom \
             is a directory the operator owns. Install the hooks from this loom build \
             (`loom repair --fix`)."
                .to_string(),
        ];
    };
    let drifted = crate::fs::permissions::hook_scripts_needing_install(dir);
    if drifted.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "loom hook scripts in {} are missing or differ from this loom build: {}. Reinstall \
         them from this build (`loom repair --fix` installs into ~/.claude/hooks/loom).",
        dir.display(),
        drifted.join(", ")
    )]
}

/// Check 3: neither `LOOM_BIN` nor the hooks directory resolves under a
/// session-writable root.
fn install_refusals(facts: &HostFacts, roots: &WritableRoots) -> Vec<String> {
    let mut installs = vec![("LOOM_BIN", facts.loom_bin.as_path())];
    installs.extend(
        facts
            .hooks_dir
            .as_deref()
            .map(|dir| ("the loom hooks directory", dir)),
    );
    installs
        .into_iter()
        .filter_map(|(name, path)| {
            let path = resolved(path);
            let root = roots.containing(&path)?;
            Some(format!(
                "{name} {} lies under the session-writable root {}: a session could replace \
                 what every hook runs. Install it outside every writable root.",
                path.display(),
                root.display()
            ))
        })
        .collect()
}

/// Check 4: `/bin/bash` and every tool the hooks find on `hook_path` resolve
/// outside every session-writable root. `hook_path` already leaves out the
/// writable directories themselves; this catches a link from outside one of
/// them into it.
fn tool_refusals(hook_path: &[PathBuf], roots: &WritableRoots) -> Vec<String> {
    let shell = (HOOK_SHELL, Some(PathBuf::from(HOOK_SHELL)));
    let tools = HOOK_TOOLS
        .iter()
        .map(|tool| (*tool, find_executable(tool, hook_path)));
    std::iter::once(shell)
        .chain(tools)
        .filter_map(|(tool, found)| {
            let path = resolved(&found?);
            let root = roots.containing(&path)?;
            Some(format!(
                "hook tool `{tool}` resolves to {} under the session-writable root {}: every \
                 loom hook runs it. Reinstall it outside every writable root.",
                path.display(),
                root.display()
            ))
        })
        .collect()
}

/// The first executable regular file named `name` in `dirs`.
fn find_executable(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter().map(|dir| dir.join(name)).find(|candidate| {
        std::fs::metadata(candidate)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    })
}

/// `path` with its deepest existing ancestor canonicalized, so a path that
/// does not exist yet is judged by where it would land.
fn resolved(path: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut existing = path;
    loop {
        if let Ok(canonical) = existing.canonicalize() {
            return missing
                .iter()
                .rev()
                .fold(canonical, |joined, name| joined.join(name));
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name);
                existing = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Session-writable roots, each in its given and its canonical spelling, so
/// a canonical path matches a root however the root was spelled.
struct WritableRoots(Vec<(PathBuf, PathBuf)>);

impl WritableRoots {
    fn new(roots: &[PathBuf]) -> Self {
        let mut forms = Vec::new();
        for root in roots {
            forms.push((root.clone(), root.clone()));
            if let Ok(canonical) = root.canonicalize() {
                forms.push((canonical, root.clone()));
            }
        }
        Self(forms)
    }

    /// The root `path` lies under, as it was given.
    fn containing(&self, path: &Path) -> Option<&Path> {
        self.0
            .iter()
            .find(|(form, _)| path.starts_with(form))
            .map(|(_, root)| root.as_path())
    }
}

#[cfg(test)]
mod tests;
