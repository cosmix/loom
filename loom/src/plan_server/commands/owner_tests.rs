//! The side process's spec: the live session's environment without its planning identity.

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use serde_json::json;

use super::side_spec;
use crate::plan_session::bridge::host::LaunchSpec;
use crate::plan_session::bridge::LaunchGuidance;
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::launch::script::claude_argv;
use crate::planning::runtime::runtime_paths_under;

fn launch_spec(base: &Path, extra_env: Vec<(String, String)>) -> LaunchSpec {
    let root = base.join("project");
    fs::create_dir_all(&root).unwrap();
    let runtime = runtime_paths_under(&base.join("loom-home"), &root).unwrap();
    let dir = SessionDir::new(runtime.sessions.join("session-side"));
    fs::create_dir_all(dir.path()).unwrap();
    let settings = r#"{"hooks": {"Stop": [{"hooks": []}]}, "permissions": {"allow": ["Read"]},
        "enabledPlugins": {"loom-ui@skills-dir": false}}"#;
    fs::write(dir.settings(), settings).unwrap();
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

/// The value after `flag` in `words`.
fn value_after<'a>(words: &'a [OsString], flag: &str) -> Option<&'a OsString> {
    let at = words.iter().position(|word| word == flag)?;
    words.get(at + 1)
}

#[test]
fn the_side_process_runs_on_its_own_hookless_settings_and_none_of_the_sessions_identity() {
    let base = tempfile::tempdir().unwrap();
    let spec = launch_spec(base.path(), Vec::new());
    let side = side_spec(&spec).unwrap();

    // Its own settings file beside the session's: no hooks, hooks disabled, the rest kept.
    assert_eq!(
        side.settings_path,
        spec.dir.path().join("side-settings.json")
    );
    assert_ne!(side.settings_path, spec.dir.settings());
    let expected = json!({
        "disableAllHooks": true,
        "permissions": {"allow": ["Read"]},
        "enabledPlugins": {"loom-ui@skills-dir": false},
    });
    assert_eq!(side.settings, expected);
    assert!(side.settings.get("hooks").is_none());

    // The command line is the side one: `--settings` names the side file, the session's own
    // settings path never appears, and the identity, name and brief flags (with their values) are
    // gone although the session's argv holds them.
    let session = claude_argv(&spec, &spec.dir.settings());
    assert_eq!(
        value_after(&side.args, "--settings"),
        Some(&side.settings_path.clone().into_os_string())
    );
    assert!(!side.args.contains(&spec.dir.settings().into_os_string()));
    for flag in ["--session-id", "--name", "--append-system-prompt-file"] {
        let value = value_after(&session, flag).expect(flag);
        assert!(
            !side.args.iter().any(|word| word == flag),
            "{flag} in {:?}",
            side.args
        );
        assert!(
            !side.args.contains(value),
            "{flag}'s value {value:?} in {:?}",
            side.args
        );
    }
    assert!(
        !side.args.iter().any(|word| word == &spec.claude_bin),
        "the program is not an argument"
    );
}
