//! Contract: completion routing follows the exact command path, so a leaf
//! command never borrows the subcommands of a same-named nested command.

use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn workspace_with_stages() -> TempDir {
    let temp = TempDir::new().unwrap();
    let stages = temp.path().join(".loom/work/stages");
    fs::create_dir_all(&stages).unwrap();
    fs::write(
        stages.join("01-core-architecture.md"),
        "---\nstatus: executing\n---\n",
    )
    .unwrap();
    fs::write(
        stages.join("02-math-core.md"),
        "---\nstatus: blocked\n---\n",
    )
    .unwrap();
    temp
}

fn route(cwd: &Path, prefix: &str, prev: &str, path: &[&str]) -> Vec<String> {
    let cmdline = format!("loom {}", path.join(" "));
    super::route_completion(cwd, prefix, prev, path, &cmdline).unwrap()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn routes_follow_the_exact_command_path() {
    let workspace = workspace_with_stages();
    let cwd = workspace.path();

    assert_eq!(
        route(cwd, "", "review", &["review"]),
        Vec::<String>::new(),
        "top-level `review` is a leaf and must offer no subcommands"
    );
    assert_eq!(
        route(cwd, "", "review", &["stage", "review"]),
        strings(&["integrity", "status"]),
        "`stage review` subcommands"
    );
    assert_eq!(
        route(cwd, "", "contracts", &["stage", "contracts"]),
        strings(&["freeze", "restore", "show"]),
        "`stage contracts` subcommands"
    );
    assert_eq!(
        route(cwd, "core", "show", &["stage", "contracts", "show"]),
        strings(&["core-architecture"]),
        "`stage contracts show` completes stage ids"
    );
}
