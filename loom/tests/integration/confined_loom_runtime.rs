//! Acceptance can invoke the running Loom binary without depending on shell setup.

use super::helpers::{init_test_repo, loom_bin_path, loom_cmd};
use loom::fs::work_dir::WorkDir;
use loom::models::stage::{AcceptanceCriterion, Stage, StageStatus, StageType};
use loom::verify::transitions::{load_stage, save_stage};

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
        let stage = Stage {
            id: "knowledge-bootstrap".into(),
            name: "Knowledge bootstrap".into(),
            stage_type: StageType::Knowledge,
            status: StageStatus::Executing,
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

fn commit_knowledge(repo: &std::path::Path) {
    for args in [
        vec!["add", "doc/loom/knowledge"],
        vec![
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "Initialize knowledge",
        ],
    ] {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
