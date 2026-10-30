//! Contracts of the permission mode a session's launch line passes, by session kind.

use std::fs;
use std::path::{Path, PathBuf};

use super::script::launch_script;
use crate::plan_session::bridge::host::LaunchSpec;
use crate::plan_session::bridge::LaunchGuidance;
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::sandbox::permission_mode;
use crate::planning::runtime::runtime_paths_under;

/// A session of `kind` over a project under `base`; `launch_script` touches none of its paths.
fn spec(kind: SessionKind, base: &Path) -> LaunchSpec {
    let root = base.join("project");
    fs::create_dir_all(root.join("doc/plans")).unwrap();
    let runtime = runtime_paths_under(&base.join("loomhome"), &root).unwrap();
    let dir = SessionDir::new(runtime.sessions.join("s1"));
    let guidance = LaunchGuidance {
        brief_path: dir.brief(),
        plugin_dir: dir.plugin(),
        opening_message: String::new(),
        model: "opus".to_owned(),
        effort: "medium".to_owned(),
        display_name: "contract".to_owned(),
        opening_on_argv: false,
    };
    LaunchSpec {
        id: "11111111-1111-4111-8111-111111111111".to_owned(),
        kind,
        dir,
        socket: "loom-plan-111111111111".to_owned(),
        tmux_session: "plan-contract".to_owned(),
        guidance,
        extra_env: Vec::new(),
        opening_argv: None,
        project_root: root,
        runtime,
        loom_bin: PathBuf::from("/opt/loom"),
        claude_bin: PathBuf::from("/opt/claude"),
        home: base.join("home"),
    }
}

/// Every value `launch.sh` gives `--permission-mode`, in the order it gives them.
fn launched_modes(kind: SessionKind) -> Vec<String> {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let spec = spec(kind, &base);
    let script = launch_script(&spec, &spec.dir.settings(), &[]);
    let words: Vec<&str> = script.split_whitespace().collect();
    let flags = words.windows(2).filter(|w| w[0] == "--permission-mode");
    flags.map(|w| w[1].to_owned()).collect()
}

#[test]
fn authoring_launches_in_auto() {
    assert_eq!(permission_mode(&SessionKind::Authoring), "auto");
    assert_eq!(launched_modes(SessionKind::Authoring), ["auto"]);
}

#[test]
fn summary_stays_manual() {
    assert_eq!(permission_mode(&SessionKind::Summary), "default");
    assert_eq!(launched_modes(SessionKind::Summary), ["default"]);
}
