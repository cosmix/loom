use crate::models::stage::Stage;
use crate::plan::schema::SandboxConfig;

use super::resolve_spawn_policy;

#[test]
fn spawn_policy_expands_grants_and_refuses_disabled_sandbox() {
    let mut plan = SandboxConfig::default();
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing").to_string_lossy().into_owned();
    plan.filesystem.allow_write.push(missing.clone());
    plan.filesystem
        .allow_write
        .push("${HOME}/loom-missing-grant".into());
    let mut stage = Stage::new("example".into(), None);
    let resolved = resolve_spawn_policy(&plan, &stage).unwrap();
    assert!(resolved
        .filesystem
        .allow_write
        .iter()
        .any(|path| { path == &crate::sandbox::expand_env_vars("${HOME}/loom-missing-grant") }));
    assert!(
        crate::sandbox::missing_grant_paths(&resolved.filesystem.allow_write, None)
            .contains(&missing)
    );
    stage.sandbox.enabled = Some(false);
    assert!(resolve_spawn_policy(&plan, &stage).is_err());
}
