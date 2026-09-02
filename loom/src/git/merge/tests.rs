use super::*;

fn isolated_git(root: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}

fn git_ok(root: &Path, args: &[&str]) {
    let out = isolated_git(root, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn test_parse_merge_stats() {
    let output = " 3 files changed, 10 insertions(+), 5 deletions(-)\n";
    let (files, ins, del) = parse_merge_stats(output);
    assert_eq!(files, 3);
    assert_eq!(ins, 10);
    assert_eq!(del, 5);
}

#[test]
fn test_parse_merge_stats_single_file() {
    let output = " 1 file changed, 2 insertions(+)\n";
    let (files, ins, del) = parse_merge_stats(output);
    assert_eq!(files, 1);
    assert_eq!(ins, 2);
    assert_eq!(del, 0);
}

fn start_conflicting_merge(root: &Path, branch: &str, branch_text: &str, main_text: &str) {
    git_ok(root, &["checkout", "-b", branch]);
    std::fs::write(root.join("a.txt"), branch_text).unwrap();
    git_ok(root, &["add", "a.txt"]);
    git_ok(root, &["commit", "-m", "branch"]);
    git_ok(root, &["checkout", "main"]);
    std::fs::write(root.join("a.txt"), main_text).unwrap();
    git_ok(root, &["add", "a.txt"]);
    git_ok(root, &["commit", "-m", "main"]);
    let merge = isolated_git(root, &["merge", "--no-ff", branch]);
    assert!(
        merge_head_exists(root).unwrap(),
        "MERGE_HEAD missing; stdout={}, stderr={}",
        String::from_utf8_lossy(&merge.stdout),
        String::from_utf8_lossy(&merge.stderr),
    );
}

fn init_repo() -> tempfile::TempDir {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path();
    git_ok(root, &["init", "-b", "main"]);
    git_ok(root, &["config", "user.email", "t@t.com"]);
    git_ok(root, &["config", "user.name", "t"]);
    std::fs::write(root.join("a.txt"), "seed").unwrap();
    git_ok(root, &["add", "a.txt"]);
    git_ok(root, &["commit", "-m", "seed"]);
    temp
}

#[test]
fn merge_stage_refuses_when_merge_head_set() {
    let temp = init_repo();
    let root = temp.path();
    start_conflicting_merge(root, "loom/blockee", "branch", "main");
    let work_dir = root.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();

    assert!(merge_stage("blockee", "main", root, &work_dir).is_err());
    assert!(merge_head_exists(root).unwrap());
}

#[test]
fn get_conflicting_files_from_status_refuses_when_merge_head_set() {
    let temp = init_repo();
    let root = temp.path();
    start_conflicting_merge(root, "loom/x", "x", "y");
    let work_dir = root.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();

    assert!(get_conflicting_files_from_status("loom/x", "main", root, &work_dir).is_err());
    assert!(merge_head_exists(root).unwrap());
}

#[test]
fn test_conflict_resolution_instructions() {
    let instructions = conflict_resolution_instructions(
        "stage-1",
        "main",
        &["src/lib.rs".to_string(), "Cargo.toml".to_string()],
    );

    assert!(instructions.contains("loom/stage-1"));
    assert!(instructions.contains("src/lib.rs"));
    assert!(instructions.contains("Cargo.toml"));
    assert!(instructions.contains("loom worktree remove stage-1"));
    assert!(!instructions.contains("loom merge stage-1"));
}
