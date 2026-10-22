use super::*;

#[test]
fn stage_query_inputs_is_a_short_one_line_label() {
    // The whole point of the label is that it cannot grow with the stage.
    assert!(!STAGE_QUERY_INPUTS.contains('\n'));
    assert!(
        STAGE_QUERY_INPUTS.len() < 200,
        "the `Selected from:` label must stay bounded, got {} chars",
        STAGE_QUERY_INPUTS.len()
    );
}

/// A dependency with no accepted merge falls back to its declared `files` and
/// `artifacts` as the paths A.23 boosts. Duplicates collapse and order is
/// stable, because a query that varies run to run cannot be reproduced.
#[test]
fn dependency_file_and_artifact_paths_reach_the_query() {
    let temp = tempfile::TempDir::new().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut dependency = Stage::new("Upstream".to_string(), None);
    dependency.id = "upstream".to_string();
    dependency.files = vec!["./src/foo.ts".to_string(), "src/foo.ts".to_string()];
    dependency.artifacts = vec!["docs/foo.md".to_string()];
    std::fs::write(
        work_dir.join("stages/upstream.md"),
        crate::verify::transitions::serialize_stage_to_markdown(&dependency).unwrap(),
    )
    .unwrap();

    let mut stage = Stage::new("Downstream".to_string(), None);
    stage.id = "downstream".to_string();
    stage.dependencies = vec!["upstream".to_string(), "never-written".to_string()];

    assert_eq!(
        StageQuery::dependency_paths(&work_dir, &stage),
        vec!["src/foo.ts".to_string(), "docs/foo.md".to_string()],
        "normalized, deduplicated, files before artifacts, missing \
         dependencies skipped"
    );
}

#[test]
fn a_populated_knowledge_tree_is_not_reported_empty() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join(".loom").join("work")).unwrap();
    std::fs::create_dir_all(root.join("doc/loom/knowledge")).unwrap();
    std::fs::write(
        root.join("doc/loom/knowledge/architecture.md"),
        "# Architecture\n\n## Components\n\nReal content.\n",
    )
    .unwrap();

    assert!(!knowledge_tree_is_empty(&root.join(".loom").join("work")));
}

#[test]
fn a_project_without_a_knowledge_tree_is_reported_empty() {
    let temp = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(temp.path().join(".loom").join("work")).unwrap();

    assert!(knowledge_tree_is_empty(
        &temp.path().join(".loom").join("work")
    ));
}

#[test]
fn test_ensure_trailing_newline_appends_when_missing() {
    let mut content = String::from("no newline yet");
    ensure_trailing_newline(&mut content);
    assert_eq!(content, "no newline yet\n");
}

#[test]
fn test_ensure_trailing_newline_is_idempotent() {
    let mut content = String::from("already has one\n");
    ensure_trailing_newline(&mut content);
    assert_eq!(content, "already has one\n");
}

#[test]
fn a_stage_brief_reads_that_stages_own_overlay() {
    let mut stage = Stage::new("Ranker".to_string(), None);
    stage.id = "source-ranker".to_string();
    stage.plan_id = Some("PLAN-source-channel".to_string());

    assert_eq!(
        StageQuery::stage_overlay_scope(&stage),
        OverlayScope::Stage {
            plan: "PLAN-source-channel".to_string(),
            stage: "source-ranker".to_string(),
        },
        "a stage's brief must describe its own worktree, not the checkout \
         the daemon happens to run in"
    );
}

#[test]
fn a_stage_naming_no_plan_falls_back_to_the_delivery_plan_key() {
    let mut stage = Stage::new("Ranker".to_string(), None);
    stage.id = "source-ranker".to_string();
    assert_eq!(stage.plan_id, None);

    // The literal matters: `unwrap_or_default()` would key the overlay by an
    // empty path component, which resolves to the stage directory's parent.
    assert_eq!(
        StageQuery::stage_overlay_scope(&stage),
        OverlayScope::Stage {
            plan: "default".to_string(),
            stage: "source-ranker".to_string(),
        }
    );
}

#[test]
fn test_build_stage_query_text_joins_non_empty_fields() {
    let mut stage = Stage::new("My Stage".to_string(), Some("Does a thing".to_string()));
    stage.id = "my-stage".to_string();
    stage.working_dir = Some("crates/foo".to_string());
    stage.files = vec!["src/lib.rs".to_string()];
    stage.dependencies = vec!["dep-1".to_string()];

    let text = StageQuery::build_stage_query_text(&stage);
    assert!(text.contains("my-stage"));
    assert!(text.contains("My Stage"));
    assert!(text.contains("Does a thing"));
    assert!(text.contains("crates/foo"));
    assert!(text.contains("src/lib.rs"));
    assert!(text.contains("dep-1"));
    // No blank lines from unset optional fields (description/working_dir are
    // both set here, but artifacts/acceptance/wiring are empty and must not
    // contribute stray blank entries).
    assert!(!text.lines().any(|line| line.trim().is_empty()));
}

#[test]
fn acceptance_commands_and_wiring_patterns_are_not_query_text() {
    let mut stage = Stage::new("Query contract".to_string(), None);
    stage.acceptance = vec![crate::models::stage::AcceptanceCriterion::Simple(
        "cargo test --workspace".to_string(),
    )];
    stage.wiring = vec![WiringCheck {
        source: "src/private.rs".to_string(),
        pattern: "INTERNAL_PATTERN".to_string(),
        description: "producer reaches consumer".to_string(),
        literal: false,
    }];

    let text = StageQuery::build_stage_query_text(&stage);
    assert!(!text.contains("cargo test --workspace"), "{text}");
    assert!(!text.contains("src/private.rs"), "{text}");
    assert!(!text.contains("INTERNAL_PATTERN"), "{text}");
    assert!(text.contains("producer reaches consumer"), "{text}");
}

#[test]
fn the_stage_brief_budget_comes_from_config() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join(".loom/work")).unwrap();
    std::fs::write(
        root.join(".loom/config.toml"),
        "[retrieval]\nstage_brief_budget_tokens = 700\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("doc/loom/knowledge")).unwrap();
    std::fs::write(
        root.join("doc/loom/knowledge/retrieval.md"),
        "# Retrieval\n\n## Budget sentinel\n\nBudget sentinel retrieval.\n",
    )
    .unwrap();
    let mut stage = Stage::new(
        "Budget sentinel".to_string(),
        Some("Budget sentinel retrieval".to_string()),
    );
    stage.id = "budget-sentinel".to_string();

    let pack = retrieve_stage_pack(&root.join(".loom/work"), &stage)
        .expect("the matching knowledge fixture should produce a brief");
    assert_eq!(pack.budget_tokens, 700);
}
