use crate::plan_session::kind::SessionDir;
use crate::planning::guidance::{
    render_session_guidance, AuthoringStart, GuidanceModels, PressureCommand, SessionGuidanceKind,
    StartBlocks,
};
use crate::planning::project::Project;
use crate::planning::runtime::{runtime_paths_under, RuntimePaths};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::Path;

const REASON: &str = "session hooks changed; review configuration before starting";

struct EnvGuard {
    name: &'static str,
    previous: Option<OsString>,
}

impl EnvGuard {
    fn set(name: &'static str, value: &Path) -> Self {
        let previous = std::env::var_os(name);
        std::env::set_var(name, value);
        Self { name, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(self.name, value),
            None => std::env::remove_var(self.name),
        }
    }
}

struct Fixture {
    project: Project,
    runtime: RuntimePaths,
    models: GuidanceModels,
}

impl Fixture {
    fn new(base: &Path) -> Self {
        let root = base.join("repo");
        for (relative, text) in [
            (
                "doc/product/index.md",
                "# Vision\n\n## Specification sections\n\n\
                 | # | Section |\n| --- | --- |\n| 01 | Alpha |\n| 02 | Beta |\n",
            ),
            ("doc/product/section_01.md", "# Alpha\n\nAlpha body.\n"),
            ("doc/product/section_02.md", "# Beta\n\nBeta body.\n"),
            (
                "doc/plans/index.md",
                "# Metaplan\n\n## 1. Delivery truth (`PLAN-foo.md`)\n",
            ),
            (
                "doc/plans/IN_PROGRESS-PLAN-foo.md",
                "# Plan\n\nImplements: 01\n",
            ),
        ] {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Self::load(base)
    }

    fn load(base: &Path) -> Self {
        let root = base.join("repo");
        Self {
            project: Project::load(&root).unwrap(),
            runtime: runtime_paths_under(base, &root).unwrap(),
            models: GuidanceModels::resolve(&Default::default(), &Default::default()),
        }
    }
}

fn kinds() -> [SessionGuidanceKind; 4] {
    [
        SessionGuidanceKind::Authoring(AuthoringStart::NoStep),
        SessionGuidanceKind::Summary {
            ids: vec!["product/section_01".into()],
        },
        SessionGuidanceKind::Pressure {
            command: PressureCommand::Pressure,
            plan_rel: "doc/plans/PLAN-foo.md".into(),
            brief: "Review this plan.\n".into(),
            model: "opus".into(),
            effort: "high".into(),
        },
        SessionGuidanceKind::Pressure {
            command: PressureCommand::Address,
            plan_rel: "doc/plans/PLAN-foo.md".into(),
            brief: "Address this plan's review.\n".into(),
            model: "opus".into(),
            effort: "high".into(),
        },
    ]
}

fn session(base: &Path, name: &str) -> SessionDir {
    let parent = base.join("sessions");
    fs::create_dir_all(&parent).unwrap();
    let path = parent.join(name);
    fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    SessionDir::new(path)
}

fn assert_refused(
    fixture: &Fixture,
    kind: &SessionGuidanceKind,
    session: &SessionDir,
    reason: &str,
) {
    let error = render_session_guidance(
        kind,
        &fixture.project,
        &fixture.runtime,
        session,
        &fixture.models,
    )
    .expect_err("a launch block must refuse guidance");

    let message = format!("{error:#}");
    assert!(message.contains(reason), "{kind:?}: {message}");
    assert!(!session.brief().exists(), "{kind:?} wrote a brief");
    assert!(!session.plugin().exists(), "{kind:?} wrote a plugin");
}

#[test]
#[serial_test::serial]
fn test_start_block_refuses_every_kind_without_writing_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let _home = EnvGuard::set("LOOM_HOME", temp.path());
    let fixture = Fixture::new(temp.path());
    StartBlocks::block(&fixture.runtime, REASON, &[".claude/hooks.json".into()]).unwrap();

    for (index, kind) in kinds().iter().enumerate() {
        let session = session(temp.path(), &format!("blocked-{index}"));
        assert_refused(&fixture, kind, &session, REASON);
    }
}

#[test]
#[serial_test::serial]
fn test_start_block_file_is_private() {
    let temp = tempfile::tempdir().unwrap();
    let _home = EnvGuard::set("LOOM_HOME", temp.path());
    let fixture = Fixture::new(temp.path());

    StartBlocks::block(&fixture.runtime, REASON, &[".mcp.json".into()]).unwrap();

    let metadata = fs::metadata(fixture.runtime.dir.join("config-block.json")).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o7777, 0o600);
}

#[test]
#[serial_test::serial]
fn test_start_block_survives_reloading_all_guidance_inputs() {
    let temp = tempfile::tempdir().unwrap();
    let _home = EnvGuard::set("LOOM_HOME", temp.path());
    {
        let fixture = Fixture::new(temp.path());
        StartBlocks::block(&fixture.runtime, REASON, &[".mcp.json".into()]).unwrap();
    }

    let restarted = Fixture::load(temp.path());
    let block = StartBlocks::current(&restarted.runtime).unwrap().unwrap();
    assert_eq!(block.reason, REASON);
    assert_eq!(block.changed, vec![".mcp.json".to_owned()]);
    let session = session(temp.path(), "restarted");
    let kind = SessionGuidanceKind::Authoring(AuthoringStart::NoStep);
    assert_refused(&restarted, &kind, &session, REASON);
}

#[test]
#[serial_test::serial]
fn test_malformed_start_block_refuses_every_kind_without_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let _home = EnvGuard::set("LOOM_HOME", temp.path());
    let fixture = Fixture::new(temp.path());
    fs::create_dir_all(&fixture.runtime.dir).unwrap();
    fs::write(
        fixture.runtime.dir.join("config-block.json"),
        "{broken json",
    )
    .unwrap();

    for (index, kind) in kinds().iter().enumerate() {
        let session = session(temp.path(), &format!("malformed-{index}"));
        assert_refused(&fixture, kind, &session, "start blocked");
    }
}

#[test]
#[serial_test::serial]
fn test_clearing_start_block_allows_every_kind_to_render() {
    let temp = tempfile::tempdir().unwrap();
    let _home = EnvGuard::set("LOOM_HOME", temp.path());
    let fixture = Fixture::new(temp.path());
    StartBlocks::block(&fixture.runtime, REASON, &[]).unwrap();

    StartBlocks::clear(&fixture.runtime).unwrap();

    assert!(StartBlocks::current(&fixture.runtime).unwrap().is_none());
    assert!(!fixture.runtime.dir.join("config-block.json").exists());
    for (index, kind) in kinds().iter().enumerate() {
        let session = session(temp.path(), &format!("cleared-{index}"));
        let guidance = render_session_guidance(
            kind,
            &fixture.project,
            &fixture.runtime,
            &session,
            &fixture.models,
        )
        .unwrap();
        assert_eq!(guidance.brief_path, session.brief());
        assert_eq!(guidance.plugin_dir, session.plugin());
        assert!(session.brief().is_file(), "{kind:?} did not write a brief");
        assert!(session.plugin().is_dir(), "{kind:?} did not write a plugin");
    }
}

#[test]
#[serial_test::serial]
fn test_start_block_uses_server_runtime_despite_session_state_environment() {
    let temp = tempfile::tempdir().unwrap();
    let other_state = tempfile::tempdir().unwrap();
    let _home = EnvGuard::set("LOOM_HOME", temp.path());
    let fixture = Fixture::new(temp.path());
    StartBlocks::block(&fixture.runtime, REASON, &[".mcp.json".into()]).unwrap();
    let _state = EnvGuard::set("LOOM_PLAN_STATE", other_state.path());
    assert_ne!(fixture.runtime.dir, other_state.path());

    for (index, kind) in kinds().iter().enumerate() {
        let session = session(temp.path(), &format!("server-runtime-{index}"));
        assert_refused(&fixture, kind, &session, REASON);
    }
    assert!(!other_state.path().join("config-block.json").exists());
}
