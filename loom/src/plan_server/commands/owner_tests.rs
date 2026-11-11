//! The side process's spec: the live session's environment without its planning identity.

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use super::side_spec;
use crate::plan_session::bridge::host::LaunchSpec;
use crate::plan_session::bridge::LaunchGuidance;
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::planning::runtime::runtime_paths_under;

fn launch_spec(base: &Path, extra_env: Vec<(String, String)>) -> LaunchSpec {
    let root = base.join("project");
    fs::create_dir_all(&root).unwrap();
    let runtime = runtime_paths_under(&base.join("loom-home"), &root).unwrap();
    let dir = SessionDir::new(runtime.sessions.join("session-side"));
    fs::create_dir_all(dir.path()).unwrap();
    fs::write(dir.settings(), r#"{"hooks": {}, "permissions": {}}"#).unwrap();
    LaunchSpec {
        id: "session-side".into(),
        kind: SessionKind::Authoring,
        socket: "loom-plan-side".into(),
        tmux_session: "plan-authoring".into(),
        guidance: LaunchGuidance {
            brief_path: dir.brief(),
            plugin_dir: dir.plugin(),
            opening_message: "OPENING".into(),
            model: "opus".into(),
            effort: "medium".into(),
            display_name: "side".into(),
            opening_on_argv: false,
        },
        dir,
        extra_env,
        opening_argv: None,
        project_root: root,
        runtime,
        loom_bin: base.join("loom"),
        claude_bin: base.join("claude"),
        home: base.join("home"),
    }
}

#[test]
fn the_side_process_gets_the_sessions_path_and_extras_without_its_identity() {
    let base = tempfile::tempdir().unwrap();
    let extra = vec![
        ("LOOM_SIDE_EXTRA".to_owned(), "from-the-start".to_owned()),
        ("LOOM_PLAN_SESSION".to_owned(), "forged".to_owned()),
    ];
    let spec = launch_spec(base.path(), extra);
    let side = side_spec(&spec).unwrap();
    let value = |name: &str| {
        let found = side.env.iter().find(|(key, _)| key == name);
        found.map(|(_, value)| value.clone())
    };
    let parent = std::env::var_os("PATH").filter(|path| !path.is_empty());
    let path = parent.unwrap_or_else(|| OsString::from("/usr/bin:/bin"));
    assert_eq!(value("PATH"), Some(path), "{:?}", side.env);
    assert_eq!(value("LOOM_SIDE_EXTRA"), Some("from-the-start".into()));
    assert_eq!(value("HOME"), Some(spec.home.clone().into_os_string()));
    for identity in ["LOOM_PLAN_SESSION", "LOOM_PLAN_STATE"] {
        assert_eq!(value(identity), None, "{identity} reached the side process");
    }
    assert_eq!(side.program, spec.claude_bin);
    assert_eq!(side.cwd, spec.project_root);
}
