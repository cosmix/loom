//! `loom run`'s confinement refusals: the startup preflights both entry
//! points share refuse an unconfined stage, both entry points run them before
//! the plan is marked in progress, and the local-settings warning comes and
//! goes with the loom-written keys.

use super::super::confinement::{confinement_findings, local_settings_warning, RunHost};
use super::*;
use crate::sandbox::preflight::{
    strip_settings_local_loom_keys, HookFindings, HostFacts, SandboxPreflightRefusal,
};

/// A work dir holding one stage, `only`, with `sandbox` as its override.
fn work_dir_with_stage(temp: &TempDir, sandbox: StageSandboxConfig) -> WorkDir {
    let work_dir = WorkDir::new(temp.path()).unwrap();
    work_dir.initialize().unwrap();
    let mut stage = Stage::new("Only".to_string(), None);
    stage.id = "only".to_string();
    stage.sandbox = sandbox;
    let stages = work_dir.root().join("stages");
    fs::create_dir_all(&stages).unwrap();
    let markdown = serialize_stage_to_markdown(&stage).unwrap();
    fs::write(stages.join("0-only.md"), markdown).unwrap();
    work_dir
}

fn disabled() -> StageSandboxConfig {
    StageSandboxConfig {
        enabled: Some(false),
        ..StageSandboxConfig::default()
    }
}

/// `loom run` and `loom run --foreground` both reach the refusal through
/// `run_startup_preflights`; driven here on this host, whatever else its
/// hooks and install would add to the refusal.
#[test]
fn the_shared_startup_preflights_refuse_a_stage_that_disables_its_sandbox() {
    let temp = TempDir::new().unwrap();
    let work_dir = work_dir_with_stage(&temp, disabled());

    let error = super::super::run_startup_preflights(&work_dir)
        .expect_err("a stage with its sandbox disabled must refuse the run");

    let text = format!("{error:#}");
    assert!(
        text.contains("stage `only`: sandbox.enabled=false"),
        "{text}"
    );
    assert!(error.is::<SandboxPreflightRefusal>(), "{text}");
}

/// Both entry points go through `run_startup_preflights`, and do so before
/// the plan file is renamed in progress, so a refused run leaves it as it
/// was. Structural, like `inputs_and_rename_are_committed_before_graph_
/// publication_in_both_run_paths`: neither entry point can run in a unit test.
#[test]
fn both_run_entry_points_refuse_before_the_plan_is_marked_in_progress() {
    for (label, source) in [
        ("run/mod.rs", include_str!("../mod.rs")),
        ("run/foreground.rs", include_str!("../foreground.rs")),
    ] {
        let refusal = source
            .find("run_startup_preflights(&work_dir)")
            .unwrap_or_else(|| panic!("{label} must call run_startup_preflights"));
        let rename = source.find("mark_plan_in_progress(").unwrap();
        assert!(
            refusal < rename,
            "{label}: refuse before marking the plan in progress"
        );
    }
}

#[test]
fn confinement_findings_refuse_check_1_and_pass_a_confined_run() {
    let host = TempDir::new().unwrap();
    let hooks_dir = host.path().join("hooks");
    crate::fs::permissions::install_loom_hooks_to(&hooks_dir).unwrap();
    let loom_bin = host.path().join("loom");
    fs::write(&loom_bin, "").unwrap();
    let home = host.path().join("home");
    let findings = |work_dir: &WorkDir| {
        let repo_root = work_dir.project_root().unwrap();
        confinement_findings(&RunHost {
            work_dir: work_dir.root(),
            repo_root,
            home: Some(home.as_path()),
            path: &[],
            facts: Ok(HostFacts {
                writable_roots: vec![repo_root.to_path_buf()],
                hooks_dir: Some(hooks_dir.clone()),
                loom_bin: loom_bin.clone(),
                hook_path: Vec::new(),
            }),
        })
        .unwrap()
    };

    let confined = TempDir::new().unwrap();
    let confined = work_dir_with_stage(&confined, StageSandboxConfig::default());
    assert_eq!(findings(&confined), HookFindings::default());

    let unconfined = TempDir::new().unwrap();
    let refused = findings(&work_dir_with_stage(&unconfined, disabled()));
    assert_eq!(refused.refusals.len(), 1, "{refused:?}");
    let refusal = &refused.refusals[0];
    assert!(
        refusal.starts_with("stage `only`: sandbox.enabled=false"),
        "{refusal}"
    );
}

#[test]
fn loom_run_warns_while_local_settings_carry_loom_written_keys_and_not_after_a_strip() {
    let temp = TempDir::new().unwrap();
    let claude = temp.path().join(".claude");
    fs::create_dir_all(&claude).unwrap();
    let settings = serde_json::json!({
        "sandbox": { "enabled": true },
        "env": { "LOOM_WORK_DIR": "/gone", "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" }
    });
    fs::write(claude.join("settings.local.json"), settings.to_string()).unwrap();

    let warning = local_settings_warning(temp.path()).expect("loom-written keys must warn");
    assert!(
        warning.contains("(sandbox block, env.LOOM_WORK_DIR)"),
        "{warning}"
    );
    assert!(warning.contains("loom repair --fix"), "{warning}");

    strip_settings_local_loom_keys(temp.path()).unwrap();
    assert_eq!(local_settings_warning(temp.path()), None);
}
