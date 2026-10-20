//! Output the committed `.gitignore` names survives the ordinary verified completion
//! and landing path, in any language layout, under a broad stage declaration.
use std::fs;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use loom::commands::stage::completion_evidence::parse_evidence_record;
use loom::daemon::{await_ready, ReadyTiming};
use loom::handoff::worktree_head_commit;
use loom::models::session::{Session, SessionStatus};
use loom::models::stage::{StageStatus, StageType};
use loom::process::sandbox_probe::{skip_unless, unix_socket_bindable};
use loom::verify::transitions::update_stage;

use super::support::{run_git, ReplayFixture};

const GENERATED: &[&str] = &[
    "node_modules/.bun/example@1.0.0/node_modules/example/index.js",
    "apps/web/node_modules/example/index.js",
    "apps/api/.venv/lib/python3.12/site-packages/example/__init__.py",
    ".ruff_cache/.gitignore",
    "apps/api/.ruff_cache/0.14.0/cache",
    "apps/api/__pycache__/main.cpython-312.pyc",
    "apps/ex/_build/dev/lib/ex/ebin/ex.beam",
    "apps/jvm/build/classes/java/main/Main.class",
];

#[test]
fn standard_completion_verifies_accepts_and_merges_with_generated_dependencies() {
    if daemon_socket_unavailable() {
        return;
    }
    let fixture = ReplayFixture::new(&["sh scripts/acceptance.sh"]).unwrap();
    prepare_monorepo(&fixture);
    let verified_commit = worktree_head_commit(&fixture.worktree).unwrap();
    let _session = live_session(&fixture);
    let producer = fixture.run_completion();
    assert!(producer.status.success(), "producer: {producer:?}");
    let stdout = String::from_utf8_lossy(&producer.stdout);
    let evidence = parse_evidence_record(&stdout).unwrap();
    assert_eq!(evidence.commit, verified_commit);
    assert!(fixture.worktree.join(".ruff_cache/acceptance-ran").exists());
    assert_eq!(
        fixture.reload_stage().unwrap().status,
        StageStatus::Executing
    );

    let _daemon = start_daemon(&fixture);
    let hook = fixture.run_hook_post(&fixture.pinned_command(), &stdout, false);
    assert!(
        hook.status.success(),
        "broker: {hook:?}\nproducer: {producer:?}"
    );
    wait_for_landing(&fixture);

    let stage = fixture.reload_stage().unwrap();
    assert_eq!(stage.stage_type, StageType::Standard);
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert_eq!(
        stage.completed_commit.as_deref(),
        Some(verified_commit.as_str())
    );
    assert!(loom::git::branch::is_ancestor_of(&verified_commit, "main", fixture.root()).unwrap());
    assert_eq!(
        git_text(fixture.root(), &["rev-parse", "main^2"]),
        verified_commit
    );
    assert_eq!(
        git_text(fixture.root(), &["show", "main:apps/api/main.py"]),
        "print('ready')"
    );
    assert_no_generated_commits(fixture.root());
}

fn prepare_monorepo(fixture: &ReplayFixture) {
    write_monorepo_sources(&fixture.worktree);
    write(
        &fixture.worktree,
        "scripts/acceptance.sh",
        &acceptance_script(),
    );
    commit_monorepo(fixture);
    update_stage(
        &fixture.stage_id,
        &fixture.root().join(".loom/work"),
        |stage| {
            stage.files = vec!["apps/**".into()];
            stage.artifacts = vec!["apps/web/src/index.ts".into(), "apps/api/main.py".into()];
            Ok(())
        },
    )
    .unwrap();
    for path in GENERATED {
        write(&fixture.worktree, path, "generated dependency/cache\n");
    }
    for path in [".bashrc", ".mcp.json", ".gitmodules"] {
        write(&fixture.worktree, path, "");
    }
    fs::write(
        fixture.root().join(".loom/work/config.toml"),
        "[plan]\nbase_branch = \"main\"\n",
    )
    .unwrap();
}

fn write_monorepo_sources(root: &Path) {
    let files = [
        (
            ".gitignore",
            "node_modules/\n.venv/\n.ruff_cache/\n__pycache__/\n_build/\nbuild/\n.loom/\n.worktrees/\n",
        ),
        (
            "package.json",
            r#"{"private":true,"workspaces":["apps/*","packages/*"]}"#,
        ),
        ("bun.lock", "# committed workspace lock\n"),
        ("apps/web/src/index.ts", "export const ready = true;\n"),
        ("apps/api/main.py", "print('ready')\n"),
        (
            "apps/api/pyproject.toml",
            "[project]\nname = 'api'\nversion = '0.1.0'\n",
        ),
        ("apps/api/uv.lock", "version = 1\n"),
        ("apps/ex/mix.exs", "defmodule Ex.MixProject do\nend\n"),
        ("apps/jvm/build.gradle", "plugins { id 'java' }\n"),
    ];
    for (path, contents) in files {
        write(root, path, contents);
    }
}

fn commit_monorepo(fixture: &ReplayFixture) {
    run_git(
        &fixture.worktree,
        &[
            "add",
            ".gitignore",
            "package.json",
            "bun.lock",
            "apps",
            "scripts",
        ],
    )
    .unwrap();
    run_git(
        &fixture.worktree,
        &["commit", "-m", "feat(workspace): scaffold monorepo"],
    )
    .unwrap();
}

fn acceptance_script() -> String {
    let mut script = String::from("set -eu\n");
    for path in GENERATED {
        script.push_str(&format!("test -f '{path}'\n"));
    }
    script.push_str("test -f apps/web/src/index.ts\ntest -f apps/api/main.py\n");
    script.push_str("mkdir -p packages/ui/node_modules/.cache\n");
    script.push_str("printf generated > packages/ui/node_modules/.cache/build\n");
    script.push_str("printf verified > .ruff_cache/acceptance-ran\n");
    script
}

fn write(root: &Path, path: &str, contents: &str) {
    let target = root.join(path);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, contents).unwrap();
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn live_session(fixture: &ReplayFixture) -> ChildGuard {
    let child = ChildGuard(Command::new("sleep").arg("120").spawn().unwrap());
    let work_dir = fixture.root().join(".loom/work");
    let mut session = Session::new();
    session.id = fixture.session_id.clone();
    session.assign_to_stage(fixture.stage_id.clone());
    session.worktree_path = Some(fixture.worktree.clone());
    session.status = SessionStatus::Running;
    session.pid = Some(child.0.id());
    loom::fs::session_files::save_session(&session, &work_dir).unwrap();
    let start_time = loom::process::process_start_time(child.0.id()).unwrap();
    write(
        &work_dir,
        &format!("pids/{}-{}.pid", session.tracking_key, session.id),
        &format!("{}\n{start_time}\n", child.0.id()),
    );
    child
}

/// The daemon child binds its control socket under the fixture directory.
fn daemon_socket_unavailable() -> bool {
    skip_unless(
        unix_socket_bindable(&std::env::temp_dir()),
        "completion_replay::generated::standard_completion_verifies_accepts_and_merges_with_generated_dependencies",
        "the daemon cannot bind an AF_UNIX socket in this sandbox",
    )
}

fn start_daemon(fixture: &ReplayFixture) -> ChildGuard {
    fixture.release_daemon_lock().unwrap();
    let work_dir = fixture.root().join(".loom/work");
    let mut command = fixture.daemon_command();
    let (reader, writer) = std::io::pipe().unwrap();
    command
        .env("LOOM_TERMINAL", "xterm")
        .stdin(Stdio::null())
        .stdout(writer.try_clone().unwrap())
        .stderr(writer);
    let mut child = ChildGuard(command.spawn().unwrap());
    drop(command);
    await_ready(
        &mut child.0,
        reader,
        &work_dir.join("orchestrator.log"),
        ReadyTiming {
            deadline: Duration::from_secs(20),
            grace: Duration::from_millis(50),
        },
    )
    .unwrap();
    child
}

fn wait_for_landing(fixture: &ReplayFixture) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let stage = fixture.reload_stage().unwrap();
        if stage.merged {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "stage did not land: {stage:?}\ndaemon log:\n{}",
            fs::read_to_string(fixture.root().join(".loom/work/orchestrator.log"))
                .unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn git_text(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn assert_no_generated_commits(root: &Path) {
    let tracked = git_text(root, &["ls-tree", "-r", "--name-only", "main"]);
    for path in tracked.lines() {
        assert!(
            !path.split('/').any(|part| matches!(
                part,
                "node_modules" | ".venv" | ".ruff_cache" | "__pycache__" | "_build" | "build"
            )),
            "generated dependency/cache was committed: {path}"
        );
        assert!(
            ![".bashrc", ".mcp.json", ".gitmodules"].contains(&path),
            "sandbox placeholder was committed: {path}"
        );
    }
}
