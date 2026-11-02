//! Launch refusal and invocation-boundary tests for capsule capabilities.

use super::*;
use crate::orchestrator::terminal::native::capsule::probe_capsule_support;

#[test]
fn partial_capabilities_refuse_before_writing_a_wrapper_or_capsule() {
    for missing in ["settings", "sources", "mcp"] {
        let mut fixture = fixture();
        match missing {
            "settings" => fixture.host.capsule_support.settings = false,
            "sources" => fixture.host.capsule_support.setting_sources = false,
            _ => fixture.host.capsule_support.strict_mcp_config = false,
        }
        let stage = Stage {
            id: "stage-1".into(),
            ..Stage::default()
        };
        let session = Session::new();
        let session_id = session.id.clone();
        let error = prepare_session_launch_with(
            &fixture.host,
            &fixture.work_dir,
            SessionType::Stage,
            &stage,
            session,
            Path::new("/signal"),
            &fixture.repo.join(".worktrees/stage-1"),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("required session isolation flags"),
            "{error}"
        );
        assert!(!session_settings_path(&fixture.work_dir, &session_id).exists());
        assert!(!fixture.work_dir.join("wrappers").exists());
    }
}

#[test]
fn supported_executable_receives_exact_generated_capsule_path() {
    use std::os::unix::fs::PermissionsExt;
    let mut fixture = fixture();
    let executable = fixture.temp.path().join("claude-stub");
    std::fs::write(
        &executable,
        "#!/bin/sh\necho '--settings --setting-sources --strict-mcp-config'\n",
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    fixture.host.capsule_support = probe_capsule_support(&executable).unwrap();
    fixture.host.claude_path = executable.clone();
    let (session, script) = launch(&fixture, SessionType::Stage);
    let capsule = session_settings_path(&fixture.work_dir, &session.id)
        .canonicalize()
        .unwrap();
    let exact = format!(
        "--settings {} --setting-sources user,project --strict-mcp-config",
        escape(Cow::Owned(capsule.display().to_string()))
    );
    assert!(script.contains(&exact), "{script}");
    assert!(script.contains(&executable.display().to_string()));
}
