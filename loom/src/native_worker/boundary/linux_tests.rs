//! The stage cache's bubblewrap mount arguments.

use super::*;

fn paths(root: &str, worktree: &str, installed: &[&str], hosts: &[&str]) -> Vec<String> {
    let installed: Vec<PathBuf> = installed.iter().map(PathBuf::from).collect();
    let args = stage_cache_args(
        Path::new(root),
        Path::new(root),
        Path::new(worktree),
        &installed,
        hosts.iter().map(PathBuf::from),
    );
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn the_root_is_bound_writable_after_each_seed_is_bound_read_only() {
    let args = paths("/cache/root", "/repo/wt", &[], &["/host/go/mod"]);
    assert_eq!(
        args,
        [
            "--ro-bind",
            "/host/go/mod",
            "/host/go/mod",
            "--bind",
            "/cache/root",
            "/cache/root"
        ]
    );
}

#[test]
fn a_seed_under_an_installed_read_root_is_not_bound_again() {
    let args = paths(
        "/cache/root",
        "/repo/wt",
        &["/usr"],
        &["/usr/lib/go", "/opt/x"],
    );
    assert!(!args.contains(&"/usr/lib/go".to_string()), "{args:?}");
    assert!(args.contains(&"/opt/x".to_string()), "{args:?}");
}

#[test]
fn a_seed_overlapping_the_root_or_the_worktree_is_skipped_and_repeats_collapse() {
    let args = paths(
        "/cache/root",
        "/repo/wt",
        &[],
        &["/cache/root/inner", "/repo", "/repo/wt/sub", "/a", "/a"],
    );
    assert_eq!(
        args.iter()
            .filter(|arg| arg.as_str() == "--ro-bind")
            .count(),
        1,
        "{args:?}"
    );
    assert_eq!(args.last().map(String::as_str), Some("/cache/root"));
}
