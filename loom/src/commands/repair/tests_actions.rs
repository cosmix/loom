use super::*;

fn assert_action(issues: &[DetectedIssue], prefix: &str, expected: RepairAction) {
    let matched: Vec<_> = issues
        .iter()
        .filter(|issue| issue.description.starts_with(prefix))
        .collect();
    assert_eq!(matched.len(), 1, "expected one {prefix} issue");
    assert_eq!(matched[0].action, expected, "{prefix}");
}

#[test]
fn detectors_select_expected_actions() {
    let root = tempfile::tempdir().unwrap();
    let cases = [
        (
            ".loom/work not found",
            RepairAction::Workspace(workspace::WorkspaceFix::GitignoreWork),
        ),
        (
            ".worktrees not found",
            RepairAction::Workspace(workspace::WorkspaceFix::GitignoreWorktrees),
        ),
        (
            "Project .claude/settings.json incomplete",
            RepairAction::Workspace(workspace::WorkspaceFix::HooksAndSettings),
        ),
        ("Settings not found", RepairAction::CleanSettings),
    ];
    let issues = check_all_issues(root.path());
    for (prefix, expected) in cases {
        let issue = issues
            .iter()
            .find(|issue| issue.description.starts_with(prefix))
            .expect("detector must report the issue");
        assert_eq!(issue.action, expected, "{prefix}");
    }
}

#[test]
fn workspace_detectors_attach_actions_and_ignore_wording() {
    let root = tempfile::tempdir().unwrap();
    let cases = [
        (
            "not found in .gitignore",
            workspace::WorkspaceFix::GitignoreWork,
        ),
        (
            ".worktrees not found",
            workspace::WorkspaceFix::GitignoreWorktrees,
        ),
    ];
    let mut issues = workspace::check(root.path());
    for (description, expected) in cases {
        let issue = issues
            .iter_mut()
            .find(|issue| issue.description.contains(description))
            .expect("detector must report the issue");
        assert!(matches!(&issue.action, RepairAction::Workspace(actual) if *actual == expected));
        issue.description = "Changed diagnostic wording".into();
        assert!(fix_issue(root.path(), issue).unwrap());
    }
}

#[test]
fn manual_issue_is_skipped_even_when_wording_looks_actionable() {
    let root = tempfile::tempdir().unwrap();
    let issue = DetectedIssue {
        severity: Severity::Warning,
        description: ".worktrees not found in .gitignore".into(),
        fix_description: String::new(),
        action: RepairAction::Manual,
    };
    assert!(!fix_issue(root.path(), &issue).unwrap());
    assert!(!root.path().join(".gitignore").exists());
}

#[test]
fn incoherent_stage_retains_id_and_reason_after_wording_changes() {
    use crate::models::stage::{Stage, StageStatus};

    let root = tempfile::tempdir().unwrap();
    let work = root.path().join(".loom/work");
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let mut stage = Stage::new("stage-one".into(), None);
    stage.status = StageStatus::Executing;
    crate::verify::transitions::save_stage(&stage, &work).unwrap();
    let mut issues = repair_coherence::check_incoherent_executing_stages(root.path());
    assert_eq!(issues.len(), 1);
    assert!(matches!(&issues[0].action,
        RepairAction::IncoherentStage { stage_id, reason }
            if stage_id == &stage.id && reason == "Executing with no session assigned"));
    issues[0].description = "Changed wording".into();
    assert!(fix_issue(root.path(), &issues[0]).unwrap());
    let repaired = crate::verify::transitions::load_stage(&stage.id, &work).unwrap();
    assert_eq!(repaired.status, StageStatus::Blocked);
}

#[test]
fn home_asset_detectors_retain_names_and_select_actions() {
    let home = tempfile::tempdir().unwrap();
    let claude = home.path().join(".claude");
    for name in ["rust's", "loom-rust's"] {
        std::fs::create_dir_all(claude.join("skills").join(name)).unwrap();
    }
    std::fs::create_dir_all(claude.join("agents")).unwrap();
    for name in ["worker's.md", "loom-worker's.md"] {
        std::fs::write(claude.join("agents").join(name), "fixture").unwrap();
    }
    std::fs::write(
        claude.join("settings.json"),
        r#"{"permissions":{"allow":["Skill(rust)"]}}"#,
    )
    .unwrap();
    let mut issues = home_assets::check_old_skills_at(&claude.join("skills"));
    issues.extend(home_assets::check_old_agents_at(&claude.join("agents")));
    issues.extend(home_assets::check_old_skill_refs_at(
        &claude.join("settings.json"),
    ));
    for (prefix, action) in [
        ("Ambiguous unprefixed skill", RepairAction::Manual),
        ("Ambiguous unprefixed agent", RepairAction::Manual),
        ("Settings.json references", RepairAction::SkillRefs),
    ] {
        assert_action(&issues, prefix, action);
    }
    for issue in &issues {
        if issue.action == RepairAction::Manual {
            assert!(!fix_issue(home.path(), issue).unwrap());
        }
    }
    assert!(claude.join("skills/rust's").exists());
    assert!(claude.join("agents/worker's.md").exists());
}

#[test]
fn settings_detectors_select_distinct_actions() {
    let root = tempfile::tempdir().unwrap();
    let claude = root.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join("settings.json"),
        serde_json::json!({
            "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "~/.claude/hooks/loom/commit-guard.sh"}]}]},
            "env": {"LOOM_STAGE_ID": "stale"}
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        claude.join("settings.local.json"),
        serde_json::json!({
            "sandbox": {}, "permissions": {"deny": [
                "Read(.loom/work/admin.token)", "Read(~/private/**)",
                "Edit(doc/loom/knowledge/**)"
            ]}
        })
        .to_string(),
    )
    .unwrap();
    let mut issues = settings_checks::check(root.path());
    issues.extend(sandbox_settings::check_stale_knowledge_denies(root.path()));
    issues.extend(
        sandbox_settings::check_read_denies(root.path())
            .into_iter()
            .filter(|issue| issue.description.contains(root.path().to_str().unwrap())),
    );
    for (prefix, action) in [
        (
            "Hooks found",
            RepairAction::Workspace(workspace::WorkspaceFix::HooksAndSettings),
        ),
        ("Stale loom session env", RepairAction::IdentityDrift),
        ("Loom config drifted", RepairAction::RewriteLocal),
        ("Loom-written keys", RepairAction::CleanSettings),
        (
            "Stale knowledge-directory deny",
            RepairAction::CleanSettings,
        ),
        ("Read deny rule in", RepairAction::ReadDenies),
        ("Operator-authored Read deny rule", RepairAction::Manual),
    ] {
        assert_action(&issues, prefix, action);
    }
}

#[test]
fn detected_read_denies_dispatch_by_action_and_preserve_operator_rules() {
    let root = tempfile::tempdir().unwrap();
    let claude = root.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    let path = claude.join("settings.local.json");
    std::fs::write(
        &path,
        r#"{"permissions":{"deny":["Read(.loom/work/admin.token)","Read(~/private/**)"]}}"#,
    )
    .unwrap();
    let issues = sandbox_settings::check_read_denies(root.path());
    let mut local: Vec<_> = issues
        .into_iter()
        .filter(|issue| issue.description.contains(path.to_str().unwrap()))
        .collect();
    assert_eq!(local.len(), 2);
    assert_eq!(
        local
            .iter()
            .filter(|issue| issue.action == RepairAction::ReadDenies)
            .count(),
        1
    );
    assert_eq!(
        local
            .iter()
            .filter(|issue| issue.action == RepairAction::Manual)
            .count(),
        1
    );
    for issue in &mut local {
        issue.description = "Changed diagnostic wording".into();
        match issue.action {
            RepairAction::ReadDenies => assert!(fix_issue(root.path(), issue).unwrap()),
            RepairAction::Manual => assert!(!fix_issue(root.path(), issue).unwrap()),
            ref other => panic!("unexpected action: {other:?}"),
        }
    }
    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        settings["permissions"]["deny"],
        serde_json::json!(["Read(~/private/**)"])
    );
}

#[test]
fn codex_config_action_retains_detected_path_after_wording_changes() {
    let root = tempfile::tempdir().unwrap();
    let config_path = root.path().join("config.toml");
    std::fs::write(&config_path, "[sandbox_workspace_write]\n").unwrap();

    let mut issue = settings_checks::codex_slash_tmp_issue_for(Some(&config_path)).unwrap();
    assert_eq!(
        issue.action,
        RepairAction::CodexSlashTmp(config_path.clone())
    );
    issue.description = "Changed diagnostic wording".into();
    assert!(fix_issue(root.path(), &issue).unwrap());
    assert!(crate::codex::codex_config_excludes_slash_tmp(&config_path));
    assert!(settings_checks::codex_slash_tmp_issue_for(Some(&config_path)).is_none());
}

#[test]
fn damaged_workspace_shapes_select_their_own_actions() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join(".loom");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("work"), "invalid").unwrap();
    assert_action(
        &workspace::check(root.path()),
        ".loom/work exists but is neither",
        RepairAction::Workspace(workspace::WorkspaceFix::InvalidWork),
    );
    #[cfg(unix)]
    {
        std::fs::remove_file(nested.join("work")).unwrap();
        std::os::unix::fs::symlink(root.path().join("missing"), nested.join("work")).unwrap();
        assert_action(
            &workspace::check(root.path()),
            ".loom/work is a symlink",
            RepairAction::Workspace(workspace::WorkspaceFix::WorkSymlink),
        );
    }
}

#[test]
fn missing_pre_commit_hook_selects_hook_installation() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".git/hooks")).unwrap();
    assert_action(
        &hooks::check(root.path()),
        "Git pre-commit hook not installed",
        RepairAction::Workspace(workspace::WorkspaceFix::PreCommitHook),
    );
}

#[test]
fn host_dependent_hook_and_jq_detectors_select_actions() {
    let root = tempfile::tempdir().unwrap();
    let expected = RepairAction::Workspace(workspace::WorkspaceFix::HookScripts);
    for mut issue in [
        hooks::codex_hooks_issue_for(true).unwrap(),
        settings_checks::hook_scripts_issue_for(1, "fixture hooks").unwrap(),
    ] {
        assert_eq!(issue.action, expected);
        issue.description = "Changed diagnostic wording".into();
        assert_eq!(issue.action, expected);
    }
    assert!(hooks::codex_hooks_issue_for(false).is_none());
    assert!(settings_checks::hook_scripts_issue_for(0, "fixture hooks").is_none());

    let mut jq = settings_checks::jq_missing_issue_for(false).unwrap();
    assert_eq!(jq.action, RepairAction::Manual);
    jq.description = "Hook scripts need reinstalling".into();
    assert!(!fix_issue(root.path(), &jq).unwrap());
    assert!(settings_checks::jq_missing_issue_for(true).is_none());
}

#[test]
fn stage_audit_manual_findings_remain_manual() {
    use crate::models::stage::Stage;

    let root = tempfile::tempdir().unwrap();
    let work = root.path().join(".loom/work");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join("stages"), "not a directory").unwrap();
    assert_action(
        &merge_state::check(root.path()),
        "Could not audit stage merge status",
        RepairAction::Manual,
    );
    assert_action(
        &repair_coherence::check_incoherent_executing_stages(root.path()),
        "Could not audit executing stages",
        RepairAction::Manual,
    );
    std::fs::remove_file(work.join("stages")).unwrap();
    let mut stage = Stage::new("missing-commit".into(), None);
    stage.id = "missing-commit".into();
    stage.merged = true;
    crate::verify::transitions::save_stage(&stage, &work).unwrap();
    assert_action(
        &merge_state::check(root.path()),
        "Stage missing-commit marked merged",
        RepairAction::Manual,
    );
}
