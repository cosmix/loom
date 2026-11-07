//! Brief tests for the integration file-list budget and a failed stage listing.

use super::*;

fn realistic_paths(count: usize) -> Vec<String> {
    (0..count)
        .map(|i| format!("loom/src/commands/hook/reviewer/file_{i:03}_x.rs"))
        .collect()
}

#[test]
fn two_hundred_realistic_paths_are_all_listed() {
    let files = realistic_paths(200);
    assert!(files.iter().all(|path| path.len() >= 45));
    let mut ctx = context(&[]);
    ctx.changed = Ok(PlanChange {
        files: files.clone(),
        missing: Vec::new(),
    });
    let text = render_integration(&ctx);
    for path in &files {
        assert!(
            text.contains(&format!("  - {path}\n")),
            "{path} is not listed"
        );
    }
    assert!(!text.contains("more\n"), "{text}");
    assert!(!text.contains("by directory"), "{text}");
}

#[test]
fn unlisted_paths_are_summarised_by_directory() {
    let mut files = realistic_paths(250);
    files.extend(["skills/a/SKILL.md".to_string(), "zzz.md".to_string()]);
    let mut ctx = context(&[]);
    ctx.changed = Ok(PlanChange {
        files,
        missing: Vec::new(),
    });
    let text = render_integration(&ctx);
    assert!(text.contains("  - ... 52 more\n"), "{text}");
    assert!(
        text.contains(
            "  - Not listed, by directory: loom/src (50), (repository root) (1), skills/a (1)\n"
        ),
        "{text}"
    );
}

#[test]
fn a_failed_stage_listing_is_reported_not_read_as_an_empty_plan() {
    let temp = TempDir::new().expect("tempdir");
    let ctx = IntegrationContext::collect(
        temp.path(),
        temp.path(),
        "main",
        Err(anyhow::anyhow!("Failed to parse stage from: stages/bad.md")),
        "stage-x",
    );
    let text = render_integration(&ctx);
    assert!(
        text.contains("- Files the plan changed: unavailable (stage listing failed: Failed to parse stage from: stages/bad.md)\n"),
        "{text}"
    );
    assert!(!text.contains("has completed"), "{text}");
}
