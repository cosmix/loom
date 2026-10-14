//! The merge signal's rule for the paths the plan's `deny_write` protects.

use std::fs;
use tempfile::TempDir;

use super::{create_test_session, create_test_stage, generate_merge_signal};
use crate::fs::work_dir::write_plan_sandbox;
use crate::plan::schema::{FilesystemConfig, SandboxConfig};

/// The merge signal generated in a work dir whose plan snapshot `prepare`
/// writes.
fn merge_signal_with_plan(prepare: impl FnOnce(&std::path::Path)) -> String {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".loom").join("work");
    fs::create_dir_all(&work_dir).unwrap();
    prepare(&work_dir);
    let path = generate_merge_signal(
        &create_test_session(),
        &create_test_stage(),
        "loom/stage-1",
        "main",
        &["doc/specs/a.md".to_string()],
        &work_dir,
    )
    .unwrap();
    fs::read_to_string(path).unwrap()
}

fn plan_denying(entries: &[&str]) -> impl FnOnce(&std::path::Path) {
    let deny_write = entries.iter().map(|entry| entry.to_string()).collect();
    move |work_dir| {
        let sandbox = SandboxConfig {
            filesystem: FilesystemConfig {
                deny_write,
                ..FilesystemConfig::default()
            },
            ..SandboxConfig::default()
        };
        write_plan_sandbox(work_dir, &sandbox).unwrap();
    }
}

#[test]
fn signal_tells_the_resolver_how_protected_paths_are_checked() {
    let content = merge_signal_with_plan(plan_denying(&["doc/specs/**", "/data/shared/**"]));

    assert!(content.contains("## Protected Paths"), "{content}");
    assert!(content.contains("- `doc/specs/**`\n"), "{content}");
    assert!(!content.contains("/data/shared"), "{content}");
    assert!(
        content.contains("exactly what git's merge produced"),
        "{content}"
    );
    assert!(
        content.contains("prints the exact command for each path it names"),
        "{content}"
    );
    assert!(
        content.contains(
            "is quoted as `$'...'`, which bash and zsh read, with each such character written as the `\\xHH` of its UTF-8 bytes"
        ),
        "{content}"
    );
    for source in ["HEAD", "MERGE_HEAD"] {
        let command = format!(
            "`git --literal-pathspecs restore --source={source} --staged --worktree -- <path>`"
        );
        assert!(content.contains(&command), "{source}: {content}");
    }
}

#[test]
fn the_task_points_at_the_protected_paths_rule_only_when_the_rule_is_there() {
    let with = merge_signal_with_plan(plan_denying(&["doc/specs/**"]));
    let without = merge_signal_with_plan(|_| {});

    assert!(
        with.contains(
            "preserving intent from both sides; a path under Protected Paths keeps exactly one \
             side instead, as that section says\n"
        ),
        "{with}"
    );
    assert!(
        without.contains("preserving intent from both sides\n"),
        "{without}"
    );
    assert!(!without.contains("Protected Paths"), "{without}");
}

#[test]
fn a_hostile_glob_renders_as_one_bullet_and_one_code_span() {
    let content = merge_signal_with_plan(plan_denying(&["doc/\n## Injected heading\n`x`/**"]));

    let section = content.split("## Protected Paths").nth(1).expect(&content);
    let bullets: Vec<&str> = section
        .lines()
        .filter(|line| line.starts_with("- "))
        .collect();
    assert_eq!(bullets.len(), 1, "{content}");
    assert_eq!(bullets[0].matches('`').count(), 2, "{content}");
    assert!(
        !content.lines().any(|line| line.starts_with("## Injected")),
        "{content}"
    );
}

#[test]
fn signal_without_worktree_denies_has_no_protected_paths_rule() {
    for content in [
        merge_signal_with_plan(|_| {}),
        merge_signal_with_plan(plan_denying(&["/data/shared/**", "~/keep/**"])),
    ] {
        assert!(!content.contains("Protected Paths"), "{content}");
    }
}

#[test]
fn signal_is_still_generated_when_the_plan_snapshot_cannot_be_read() {
    // Valid TOML whose `deny_write` is not a list: the section cannot be read.
    let content = merge_signal_with_plan(|work_dir| {
        fs::write(
            work_dir.join("config.toml"),
            "[plan_sandbox.filesystem]\ndeny_write = \"doc/specs/**\"\n",
        )
        .unwrap();
    });

    assert!(content.contains("## Your Task"), "{content}");
    assert!(!content.contains("Protected Paths"), "{content}");
}
