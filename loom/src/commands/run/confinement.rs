//! `loom run`'s refusal to start a run whose sessions would not be confined
//! (`doc/plans/PLAN-loom-state-confinement.md`, section 12): checks 1-5 of
//! `crate::sandbox::preflight` over every stage and the host its sessions
//! launch on, and a warning while the operator's `.claude/settings.local.json`
//! still carries keys loom used to write there. The daemon repeats checks 1-4
//! at each spawn.

use anyhow::{Context, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::fs::work_dir::WorkDir;
use crate::sandbox::preflight::{self, HookFindings, HookScan, HostFacts, SandboxPreflightRefusal};

/// Refuse to start unless checks 1-5 pass, printing their warnings first.
pub(super) fn require_confinement(work_dir: &WorkDir) -> Result<()> {
    let repo_root = work_dir
        .project_root()
        .context("cannot resolve the project root of the state directory")?;
    let home = dirs::home_dir();
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let path: Vec<PathBuf> = std::env::split_paths(&path_var).collect();
    let findings = confinement_findings(&RunHost {
        work_dir: work_dir.root(),
        repo_root,
        home: home.as_deref(),
        path: &path,
        facts: crate::orchestrator::terminal::native::run_host_facts(work_dir.root()),
    })?;
    for warning in &findings.warnings {
        eprintln!("{} {warning}", "⚠".yellow().bold());
    }
    SandboxPreflightRefusal::check(findings.refusals)
        .context("loom run refuses to start: its sessions would not be confined")
}

/// The host a run's sessions launch on, as `confinement_findings` reads it.
pub(super) struct RunHost<'a> {
    pub(super) work_dir: &'a Path,
    pub(super) repo_root: &'a Path,
    pub(super) home: Option<&'a Path>,
    /// The PATH hooks inherit, which a bare hook program resolves through.
    pub(super) path: &'a [PathBuf],
    /// What every spawn resolves and checks (checks 2-4).
    pub(super) facts: Result<HostFacts>,
}

/// Checks 1-5 over the stages in `host.work_dir` and over the host, plus the
/// local-settings warning. Facts that cannot be resolved are a refusal.
pub(super) fn confinement_findings(host: &RunHost<'_>) -> Result<HookFindings> {
    let plan = crate::fs::work_dir::read_plan_sandbox(host.work_dir)?.unwrap_or_default();
    let stages = crate::verify::list_all_stages(host.work_dir)?;
    let mut refusals = preflight::sandbox_policy_refusals(&plan, &stages);
    let writable_roots: &[PathBuf] = match &host.facts {
        Ok(facts) => {
            refusals.extend(preflight::host_refusals(facts));
            &facts.writable_roots
        }
        Err(error) => {
            refusals.push(format!(
                "the loom install and hooks fail the preflight: {error:#}"
            ));
            &[]
        }
    };
    let documents = preflight::hook_documents(host.repo_root, host.home);
    let hooks = preflight::scan_hook_registrations(&HookScan {
        documents: &documents,
        repo_root: host.repo_root,
        home: host.home,
        path: host.path,
        writable_roots,
    });
    refusals.extend(hooks.refusals);
    let mut warnings = hooks.warnings;
    warnings.extend(local_settings_warning(host.repo_root));
    Ok(HookFindings { refusals, warnings })
}

/// What `loom run` prints while `R/.claude/settings.local.json` still carries
/// loom-written keys; `None` once it does not.
pub(super) fn local_settings_warning(repo_root: &Path) -> Option<String> {
    let keys = preflight::settings_local_loom_keys(repo_root);
    (!keys.is_empty()).then(|| {
        format!(
            "{} still carries loom-written keys ({}); every session's capsule carries them \
             now. Run `loom repair --fix` to strip them.",
            repo_root
                .join(".claude")
                .join("settings.local.json")
                .display(),
            keys.summary()
        )
    })
}
