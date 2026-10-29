//! Scope-dependent permissions of the planning sandbox builder.

use std::fs;
use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};

use super::tests_kinds::Fixture;
use super::{assert_planning_settings, build_settings, SandboxInputs};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::scope::{
    self, KickoffGrant, KickoffScope, KickoffSize, SessionScope, SCOPE_FILE,
};
use crate::planning::paths::LockedSet;

fn session(f: &Fixture) -> Result<SessionDir> {
    let dir = SessionDir::new(f.runtime.sessions.join("scoped"));
    fs::create_dir_all(dir.path())?;
    Ok(dir)
}

fn build(f: &Fixture, dir: &SessionDir) -> Result<Value> {
    let inputs = SandboxInputs {
        project_root: &f.root,
        runtime: &f.runtime,
        session_dir: dir,
        kind: &SessionKind::Authoring,
        loom_bin: Path::new("/usr/bin/loom"),
        home: &f.home,
    };
    build_settings(&inputs, &LockedSet::scan(&f.root)?, &[])
}

fn team_scope() -> Result<SessionScope> {
    SessionScope::team(vec!["doc/product/new/section_1.md".into()], Vec::new())
}

#[test]
fn team_scope_keeps_peer_tools_in_process() -> Result<()> {
    let f = Fixture::new();
    let dir = session(&f)?;
    scope::write(&dir, &team_scope()?)?;

    let settings = build(&f, &dir)?;

    let deny = settings["permissions"]["deny"].as_array().unwrap();
    assert!(!deny.contains(&json!("SendMessage")));
    assert!(!deny.contains(&json!("ListAgents")));
    assert_eq!(settings["teammateMode"], json!("in-process"));
    assert_planning_settings(&settings)?;
    Ok(())
}

#[test]
fn open_and_kickoff_scopes_keep_the_same_other_permissions() -> Result<()> {
    let f = Fixture::new();
    let dir = session(&f)?;
    let missing = build(&f, &dir)?;
    scope::write(&dir, &SessionScope::Open)?;
    let open = build(&f, &dir)?;
    assert_eq!(missing, open);
    scope::write(
        &dir,
        &SessionScope::Kickoff(KickoffScope {
            existing: Vec::new(),
            grant: None,
        }),
    )?;

    let kickoff = build(&f, &dir)?;

    for settings in [&open, &kickoff] {
        let deny = settings["permissions"]["deny"].as_array().unwrap();
        assert!(deny.contains(&json!("SendMessage")));
        assert!(deny.contains(&json!("ListAgents")));
        assert_eq!(settings.get("teammateMode"), None);
        assert_planning_settings(settings)?;
    }
    let mut open_allow = open["permissions"]["allow"].as_array().unwrap().clone();
    open_allow
        .retain(|rule| rule != "Bash(loom knowledge *)" && rule != "Bash(loom plan summary *)");
    let mut kickoff_allow = kickoff["permissions"]["allow"].as_array().unwrap().clone();
    kickoff_allow.retain(|rule| rule != "Bash(loom plan summary status*)");
    assert_eq!(kickoff_allow, open_allow);
    Ok(())
}

#[test]
fn open_authoring_allows_doc_loom_writers() -> Result<()> {
    let f = Fixture::new();
    let dir = session(&f)?;
    scope::write(&dir, &SessionScope::Open)?;

    let settings = build(&f, &dir)?;

    let allow = settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.contains(&json!("Bash(loom knowledge *)")));
    assert!(allow.contains(&json!("Bash(loom plan summary *)")));
    assert!(allow.contains(&json!("Bash(loom plan verify *)")));
    assert!(allow.contains(&json!("Bash(loom plan check*)")));
    assert!(allow.contains(&json!("Bash(loom map *)")));
    Ok(())
}

#[test]
fn unconfirmed_kickoff_authoring_prompts_for_doc_loom_writers() -> Result<()> {
    let f = Fixture::new();
    let dir = session(&f)?;
    scope::write(
        &dir,
        &SessionScope::Kickoff(KickoffScope {
            existing: Vec::new(),
            grant: None,
        }),
    )?;

    let settings = build(&f, &dir)?;

    let allow = settings["permissions"]["allow"].as_array().unwrap();
    assert!(!allow.contains(&json!("Bash(loom knowledge *)")));
    assert!(!allow.contains(&json!("Bash(loom plan summary *)")));
    assert!(allow.contains(&json!("Bash(loom plan summary status*)")));
    assert!(allow.contains(&json!("Bash(loom plan verify *)")));
    assert!(allow.contains(&json!("Bash(loom plan check*)")));
    assert!(allow.contains(&json!("Bash(loom map *)")));
    Ok(())
}

#[test]
fn confirmed_kickoff_authoring_prompts_for_doc_loom_writers() -> Result<()> {
    let f = Fixture::new();
    let dir = session(&f)?;
    scope::write(
        &dir,
        &SessionScope::Kickoff(KickoffScope {
            existing: Vec::new(),
            grant: Some(KickoffGrant {
                size: KickoffSize::Feature,
                name: "new-feature".to_owned(),
            }),
        }),
    )?;

    let settings = build(&f, &dir)?;

    let allow = settings["permissions"]["allow"].as_array().unwrap();
    assert!(!allow.contains(&json!("Bash(loom knowledge *)")));
    assert!(!allow.contains(&json!("Bash(loom plan summary *)")));
    assert!(allow.contains(&json!("Bash(loom plan summary status*)")));
    assert!(allow.contains(&json!("Bash(loom plan verify *)")));
    assert!(allow.contains(&json!("Bash(loom plan check*)")));
    assert!(allow.contains(&json!("Bash(loom map *)")));
    Ok(())
}

#[test]
fn team_authoring_allows_summaries_and_prompts_for_knowledge_writes() -> Result<()> {
    let f = Fixture::new();
    let dir = session(&f)?;
    scope::write(&dir, &team_scope()?)?;

    let settings = build(&f, &dir)?;

    let allow = settings["permissions"]["allow"].as_array().unwrap();
    assert!(!allow.contains(&json!("Bash(loom knowledge *)")));
    assert!(allow.contains(&json!("Bash(loom plan summary *)")));
    assert!(allow.contains(&json!("Bash(loom plan verify *)")));
    assert!(allow.contains(&json!("Bash(loom plan check*)")));
    assert!(allow.contains(&json!("Bash(loom map *)")));
    Ok(())
}

#[test]
fn missing_peer_denies_require_in_process_teammates() -> Result<()> {
    let f = Fixture::new();
    let dir = session(&f)?;
    scope::write(&dir, &team_scope()?)?;
    let settings = build(&f, &dir)?;

    for mode in [None, Some(json!("tmux"))] {
        let mut settings = settings.clone();
        let object = settings.as_object_mut().unwrap();
        object.remove("teammateMode");
        if let Some(mode) = mode {
            object.insert("teammateMode".to_owned(), mode);
        }
        let error = assert_planning_settings(&settings).unwrap_err();
        assert!(error
            .to_string()
            .contains("/permissions/deny lacks SendMessage"));
    }
    Ok(())
}

#[test]
fn team_scope_still_requires_other_tool_denies() -> Result<()> {
    let f = Fixture::new();
    let dir = session(&f)?;
    scope::write(&dir, &team_scope()?)?;
    let settings = build(&f, &dir)?;

    for tool in ["Workflow", "AskUserQuestion"] {
        let mut settings = settings.clone();
        settings["permissions"]["deny"]
            .as_array_mut()
            .unwrap()
            .retain(|rule| rule != tool);
        let error = assert_planning_settings(&settings).unwrap_err();
        assert!(error
            .to_string()
            .contains(&format!("/permissions/deny lacks {tool}")));
    }
    Ok(())
}

#[test]
fn malformed_scope_fails_the_settings_build() -> Result<()> {
    let f = Fixture::new();
    let dir = session(&f)?;
    fs::write(dir.path().join(SCOPE_FILE), b"{malformed")?;

    assert!(build(&f, &dir).is_err());
    Ok(())
}
