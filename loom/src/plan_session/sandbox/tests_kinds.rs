//! Per-kind snapshot and tool-deny tests of the planning sandbox builder. The shared fixture
//! and expected-value helpers are reused by `tests_assert`.

use std::fs;
use std::path::{Path, PathBuf};

use glob::{MatchOptions, Pattern};
use serde_json::{json, Value};
use tempfile::TempDir;

use super::tests_kinds_fixture::{expected_common, expected_common_for, expected_edit_asks, kinds};
use super::{
    assert_planning_settings, build_settings, ensure_runtime_grants, HookSpec, SandboxInputs,
};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::planning::paths::LockedSet;
use crate::planning::runtime::{runtime_paths_under, RuntimePaths};

pub(super) struct Fixture {
    _temp: TempDir,
    pub(super) root: PathBuf,
    pub(super) home: PathBuf,
    pub(super) runtime: RuntimePaths,
    session: SessionDir,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let root = base.join("repo");
        for dir in [
            ".git",
            "loom",
            ".claude",
            ".loom/work",
            "doc/product",
            "doc/notes",
            "doc/plans/archive",
            "doc/plans/briefs/b",
            "doc/plans/briefs/a",
            "doc/loom/knowledge",
            "doc/loom/summaries",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in [
            "README.md",
            "planning-omissions.md",
            "doc/plans/PLAN-a.md",
            "doc/plans/IN_PROGRESS-PLAN-b.md",
            "doc/plans/REVIEW-PLAN-b.md",
            "doc/plans/archive/DONE-PLAN-c.md",
            "doc/plans/briefs/b/x.md",
            "doc/plans/briefs/a/y.md",
        ] {
            fs::write(root.join(file), "# Fixture\n").unwrap();
        }
        let home = base.join("home");
        fs::create_dir_all(home.join(".loom/plan")).unwrap();
        let runtime = runtime_paths_under(&base.join("loomhome"), &root).unwrap();
        ensure_runtime_grants(&runtime).unwrap();
        let session = SessionDir::new(runtime.sessions.join("session-a"));
        fs::create_dir_all(session.done()).unwrap();
        Self {
            _temp: temp,
            root,
            home,
            runtime,
            session,
        }
    }

    pub(super) fn build(&self, kind: &SessionKind, hooks: &[HookSpec]) -> anyhow::Result<Value> {
        let inputs = SandboxInputs {
            project_root: &self.root,
            runtime: &self.runtime,
            session_dir: &self.session,
            kind,
            loom_bin: Path::new("/usr/bin/loom"),
            home: &self.home,
        };
        build_settings(&inputs, &LockedSet::scan(&self.root)?, hooks)
    }

    fn paths(&self, relative: &[&str]) -> Vec<PathBuf> {
        relative.iter().map(|path| self.root.join(path)).collect()
    }

    pub(super) fn command(&self, event: &str) -> String {
        format!(
            "/usr/bin/loom plan hook {event} --session-dir {}",
            self.session.path().display()
        )
    }
}

#[test]
fn summary_whole_json_snapshot() {
    let f = Fixture::new();
    let mut expected = expected_common(&f);
    expected["sandbox"]["filesystem"]["denyWrite"] = json!(f.paths(&[
        ".claude",
        ".git",
        ".loom",
        "README.md",
        "doc/loom/knowledge",
        "doc/notes",
        "doc/plans",
        "doc/product",
        "loom",
        "planning-omissions.md"
    ]));
    expected["sandbox"]["filesystem"]["allowWrite"] =
        json!([f.runtime.claims, f.runtime.drafts, f.runtime.summaries_lock]);
    expected["permissions"]["allow"] = json!([
        format!("Edit(/{}/**)", f.runtime.drafts.display()),
        format!("Read(/{}/**)", f.runtime.drafts.display()),
        format!("Read(/{}/**)", f.runtime.claims.display()),
        format!("Read(/{}/**)", f.runtime.attachments.display()),
        "Bash(loom plan summary *)"
    ]);
    expected["permissions"]["deny"] = json!([
        "LSP",
        "RemoteTrigger",
        "CronCreate",
        "PushNotification",
        "Monitor",
        "PowerShell",
        "Workflow",
        "SendMessage",
        "ListAgents",
        "AskUserQuestion",
        "WebSearch",
        "WebFetch"
    ]);

    assert_eq!(f.build(&SessionKind::Summary, &[]).unwrap(), expected);
}

const PRESSURE_DENIED_TOOLS: [&str; 8] = [
    "LSP",
    "RemoteTrigger",
    "CronCreate",
    "PushNotification",
    "Monitor",
    "PowerShell",
    "Workflow",
    "AskUserQuestion",
];

#[test]
fn pressure_whole_json_snapshot() {
    let f = Fixture::new();
    let mut expected = expected_common_for(&f, &kinds()[2]);
    expected["sandbox"]["filesystem"]["denyWrite"] = json!(f.paths(&[
        ".claude",
        ".git",
        ".loom",
        "README.md",
        "doc/loom",
        "doc/notes",
        "doc/plans/IN_PROGRESS-PLAN-b.md",
        "doc/plans/REVIEW-PLAN-b.md",
        "doc/plans/archive",
        "doc/plans/briefs/b",
        "doc/product",
        "loom"
    ]));
    expected["sandbox"]["filesystem"]["allowWrite"] = json!([f.session.done()]);
    expected["sandbox"]["filesystem"]["allowRead"]
        .as_array_mut()
        .unwrap()
        .push(json!(f.session.done()));
    expected["permissions"]["allow"] = json!([
        format!("Edit(/{}/doc/plans/PLAN-a.md)", f.root.display()),
        format!("Edit(/{}/doc/plans/briefs/a/**)", f.root.display()),
        format!("Edit(/{}/planning-omissions.md)", f.root.display()),
        format!("Bash(touch {})", f.session.marker().display()),
        "Bash(loom knowledge *)",
        "Bash(loom plan summary *)",
        "Bash(loom plan verify *)",
        "Bash(loom plan check*)",
        "Bash(loom map *)",
        "WebSearch"
    ]);
    let asks = expected_edit_asks(&f.root);
    expected["permissions"]["ask"] = json!(std::iter::once("WebFetch".to_owned())
        .chain(asks)
        .collect::<Vec<_>>());
    expected["permissions"]["deny"] = json!(PRESSURE_DENIED_TOOLS);
    expected["teammateMode"] = json!("in-process");

    assert_eq!(f.build(&kinds()[2], &[]).unwrap(), expected);
}

#[test]
fn only_the_pressure_settings_keep_teammates_in_process() {
    let f = Fixture::new();
    for kind in [SessionKind::Authoring, SessionKind::Summary] {
        let settings = f.build(&kind, &[]).unwrap();
        assert_eq!(settings.get("teammateMode"), None, "{kind:?}");
        assert_eq!(
            settings["permissions"].get("teammateMode"),
            None,
            "{kind:?}"
        );
    }
    let pressure = f.build(&kinds()[2], &[]).unwrap();
    assert_eq!(pressure["teammateMode"], json!("in-process"));
    assert_eq!(pressure["permissions"].get("teammateMode"), None);
}

macro_rules! tool_denies_test {
    ($name:ident, $index:expr, $peers:expr) => {
        #[test]
        fn $name() {
            let settings = Fixture::new().build(&kinds()[$index], &[]).unwrap();
            let deny = settings["permissions"]["deny"].as_array().unwrap();
            for tool in [
                "LSP",
                "RemoteTrigger",
                "CronCreate",
                "PushNotification",
                "Monitor",
                "PowerShell",
                "Workflow",
            ] {
                assert!(deny.contains(&json!(tool)), "missing {tool}");
            }
            for tool in ["SendMessage", "ListAgents"] {
                assert_eq!(deny.contains(&json!(tool)), $peers, "{tool}");
            }
            assert_planning_settings(&settings).unwrap();
        }
    };
}
tool_denies_test!(authoring_tool_denies, 0, true);
tool_denies_test!(summary_tool_denies, 1, true);
tool_denies_test!(pressure_tool_denies, 2, false);

#[test]
fn redirected_runtime_denies_the_canonical_planning_root_and_home_root() {
    let mut f = Fixture::new();
    let base = f.home.parent().unwrap();
    fs::create_dir_all(base.join("real-loomhome/plan")).unwrap();
    std::os::unix::fs::symlink(base.join("real-loomhome"), base.join("elsewhere-loomhome"))
        .unwrap();
    f.runtime = runtime_paths_under(&base.join("elsewhere-loomhome"), &f.root).unwrap();
    for kind in kinds() {
        let settings = f.build(&kind, &[]).unwrap();
        let deny = settings["sandbox"]["filesystem"]["denyRead"]
            .as_array()
            .unwrap();
        assert!(deny.contains(&json!(f.runtime.planning_root.canonicalize().unwrap())));
        assert!(deny.contains(&json!(f.home.join(".loom/plan"))));
        assert!(!deny.contains(&json!(f.runtime.dir)));
    }
}

#[test]
fn no_kind_allows_an_edit_under_doc_loom() {
    let f = Fixture::new();
    let options = MatchOptions {
        require_literal_separator: true,
        ..MatchOptions::new()
    };
    for kind in kinds() {
        let settings = f.build(&kind, &[]).unwrap();
        for rule in settings["permissions"]["allow"].as_array().unwrap() {
            let Some(pattern) = rule.as_str().unwrap().strip_prefix("Edit(/") else {
                continue;
            };
            let pattern = Pattern::new(pattern.strip_suffix(')').unwrap()).unwrap();
            // Gitignore semantics: a pattern matching an ancestor directory covers the file.
            for rel in [
                "doc/loom",
                "doc/loom/knowledge",
                "doc/loom/summaries",
                "doc/loom/knowledge/x.md",
                "doc/loom/summaries/x.md",
            ] {
                assert!(
                    !pattern.matches_path_with(&f.root.join(rel), options),
                    "{kind:?}: {rule}"
                );
            }
        }
    }
}

#[test]
fn non_utf8_state_root_fails_the_build() {
    use std::os::unix::ffi::OsStrExt;
    let f = Fixture::new();
    let work = f.root.join(".loom/work");
    let target = f
        .home
        .parent()
        .unwrap()
        .join(std::ffi::OsStr::from_bytes(b"state-\xff"));
    fs::create_dir(&target).unwrap();
    fs::remove_dir(&work).unwrap();
    std::os::unix::fs::symlink(&target, &work).unwrap();
    let error = f.build(&SessionKind::Authoring, &[]).unwrap_err();
    assert!(
        format!("{error:#}").contains("not valid UTF-8"),
        "{error:#}"
    );
}

#[test]
fn kind_rules_use_absolute_edits_and_no_inert_writes_or_read_denies() {
    let f = Fixture::new();
    for kind in kinds() {
        let settings = f.build(&kind, &[]).unwrap();
        for section in ["allow", "ask", "deny"] {
            for rule in settings["permissions"][section].as_array().unwrap() {
                let rule = rule.as_str().unwrap();
                assert!(!rule.starts_with("Write("), "{rule}");
                assert!(
                    !rule.starts_with("Edit(") || rule.starts_with("Edit(//"),
                    "{rule}"
                );
                assert!(section != "deny" || !rule.starts_with("Read("), "{rule}");
            }
        }
        let deny = settings["sandbox"]["filesystem"]["denyWrite"]
            .as_array()
            .unwrap();
        for (index, path) in deny.iter().enumerate() {
            for (other, ancestor) in deny.iter().enumerate() {
                assert!(
                    index == other
                        || !Path::new(path.as_str().unwrap())
                            .starts_with(ancestor.as_str().unwrap()),
                    "{path} beneath {ancestor}"
                );
            }
        }
    }
}

#[test]
fn summary_denies_web_search_without_allowing_it() {
    let settings = Fixture::new().build(&SessionKind::Summary, &[]).unwrap();
    assert!(!settings["permissions"]["allow"]
        .as_array()
        .unwrap()
        .contains(&json!("WebSearch")));
    assert!(settings["permissions"]["deny"]
        .as_array()
        .unwrap()
        .contains(&json!("WebSearch")));
}

#[test]
fn pressure_touch_rule_is_the_exact_completion_command() {
    let f = Fixture::new();
    let settings = f.build(&kinds()[2], &[]).unwrap();
    let touch: Vec<_> = settings["permissions"]["allow"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|rule| rule.as_str().unwrap().starts_with("Bash(touch "))
        .collect();
    assert_eq!(
        touch,
        vec![&json!(format!(
            "Bash(touch {})",
            f.session.marker().display()
        ))]
    );
}
