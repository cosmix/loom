//! Acceptance can invoke the running Loom binary without depending on shell setup.

use super::helpers::{
    git_checked, init_test_repo, loom_bin_path, loom_cmd, write_main_checkout_identity,
};
use loom::fs::work_dir::WorkDir;
use loom::models::stage::{AcceptanceCriterion, Stage, StageStatus, StageType};
use loom::verify::transitions::{load_stage, save_stage};

const SESSION: &str = "knowledge-session";

#[test]
fn knowledge_completion_finds_loom_with_minimal_path() {
    let binary_dir = std::path::Path::new(loom_bin_path()).parent().unwrap();
    for path in [
        "/usr/bin:/bin".to_string(),
        format!("{}:/usr/bin:/bin", binary_dir.display()),
    ] {
        let repo = init_test_repo();
        let work = WorkDir::new(repo.path()).unwrap();
        work.initialize().unwrap();
        commit_knowledge(repo.path());
        let work_dir = repo.path().join(".loom/work");
        write_main_checkout_identity(repo.path(), &work_dir);
        write_session_baseline(repo.path(), &work_dir);
        let stage = Stage {
            id: "knowledge-bootstrap".into(),
            name: "Knowledge bootstrap".into(),
            stage_type: StageType::Knowledge,
            status: StageStatus::Executing,
            session: Some(SESSION.into()),
            working_dir: Some(".".into()),
            acceptance: vec![
                AcceptanceCriterion::Simple("loom --version".into()),
                AcceptanceCriterion::Simple("loom knowledge check".into()),
            ],
            ..Default::default()
        };
        save_stage(&stage, &work_dir).unwrap();
        let output = loom_cmd()
            .args(["stage", "complete", &stage.id, "--no-cache"])
            .env("PATH", &path)
            .env("HOME", repo.path())
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "PATH={path}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let completed = load_stage(&stage.id, &work_dir).unwrap();
        assert_eq!(completed.status, StageStatus::Completed);
        assert!(completed.merged);
    }
}

/// The trusted starting commit the daemon records when it spawns a knowledge
/// session; completion validates knowledge changes against it.
fn write_session_baseline(repo: &std::path::Path, work_dir: &std::path::Path) {
    let head = git_checked(repo, &["rev-parse", "HEAD"]);
    let baselines = work_dir.join("knowledge-baselines");
    std::fs::create_dir_all(&baselines).unwrap();
    std::fs::write(baselines.join(SESSION), head.stdout).unwrap();
}

fn commit_knowledge(repo: &std::path::Path) {
    git_checked(repo, &["add", "doc/loom/knowledge"]);
    git_checked(repo, &["commit", "-m", "Initialize knowledge"]);
}
