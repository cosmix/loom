//! Tests for `declared_and_recommended_skills`, split out of `generate.rs`
//! to keep that file under the size ceiling.

use super::*;
use crate::models::stage::StageStatus;
use std::io::Write;
use tempfile::TempDir;

fn write_skill(dir: &Path, name: &str, description: &str) {
    let skill_dir = dir.join(name);
    std::fs::create_dir_all(&skill_dir).unwrap();
    let mut f = std::fs::File::create(skill_dir.join("SKILL.md")).unwrap();
    writeln!(f, "---").unwrap();
    writeln!(f, "name: {name}").unwrap();
    writeln!(f, "description: {description}").unwrap();
    writeln!(f, "---").unwrap();
}

fn test_stage(skills: Vec<String>) -> Stage {
    Stage {
        id: "stage-1".to_string(),
        name: "Stage One".to_string(),
        status: StageStatus::Queued,
        skills,
        ..Stage::default()
    }
}

fn test_worktree() -> Worktree {
    Worktree::new(
        "stage-1".to_string(),
        PathBuf::from("/repo/.worktrees/stage-1"),
        "loom/stage-1".to_string(),
    )
}

#[test]
fn declared_skill_leads_with_directive_marker() {
    let temp = TempDir::new().unwrap();
    write_skill(temp.path(), "loom-auth", "Auth patterns");
    let index = SkillIndex::load_from_directory(temp.path()).unwrap();

    let stage = test_stage(vec!["loom-auth".to_string()]);
    let skills = declared_and_recommended_skills(&index, &stage, &test_worktree(), &[]);

    assert_eq!(skills[0].name, "loom-auth");
    assert!(skills[0]
        .matched_triggers
        .iter()
        .any(|t| t == "declared-for-stage"));
}

#[test]
fn unresolved_declared_skill_is_silently_skipped() {
    let temp = TempDir::new().unwrap();
    let index = SkillIndex::load_from_directory(temp.path()).unwrap();

    let stage = test_stage(vec!["loom-does-not-exist".to_string()]);
    let skills = declared_and_recommended_skills(&index, &stage, &test_worktree(), &[]);

    assert!(skills.is_empty());
}

#[test]
fn declared_skill_is_not_duplicated_by_file_detection() {
    let temp = TempDir::new().unwrap();
    write_skill(temp.path(), "loom-rust", "Rust expertise");
    let index = SkillIndex::load_from_directory(temp.path()).unwrap();

    let mut stage = test_stage(vec!["loom-rust".to_string()]);
    stage.files = vec!["src/**/*.rs".to_string()];
    let skills = declared_and_recommended_skills(&index, &stage, &test_worktree(), &[]);

    assert_eq!(skills.iter().filter(|s| s.name == "loom-rust").count(), 1);
}
