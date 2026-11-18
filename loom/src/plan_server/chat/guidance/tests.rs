//! Guidance feature selection and private launch artifacts.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};

use tempfile::TempDir;

use super::*;
use crate::plan_server::context::BindInfo;
use crate::plan_server::prereq::ReadOnly;
use crate::planning::watch::OwnWrites;

fn fixture() -> (TempDir, PlanContext, SessionDir) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("doc/product/demo")).unwrap();
    fs::write(root.path().join("doc/product/demo/index.md"), "# Demo\n").unwrap();
    let bind = BindInfo::new("127.0.0.1:7272".parse().unwrap(), &"ab".repeat(32));
    let ctx = PlanContext::new(
        root.path().to_path_buf(),
        ReadOnly::from_problems(Vec::new()),
        bind,
        OwnWrites::default(),
    )
    .unwrap();
    let dir = SessionDir::new(root.path().join("session"));
    safe_create_dir_all_in_workdir(
        safe_open_dirfd(root.path()).unwrap().as_raw_fd(),
        Path::new("session"),
        0o700,
    )
    .unwrap();
    (root, ctx, dir)
}

#[test]
fn first_message_is_verbatim_and_takes_precedence_over_offer() {
    let (_root, ctx, dir) = fixture();
    let message = "  Plan the next change.\n@doc/product/demo/index.md\n";
    let guidance = FixedGuidance
        .authoring(&ctx, &dir, Some("offer-id"), Some(message), None)
        .unwrap();
    assert_eq!(guidance.opening_message, message);
    assert!(!guidance.opening_on_argv);
}

#[test]
fn offer_opens_its_step() {
    let (_root, ctx, dir) = fixture();
    let guidance = FixedGuidance
        .authoring(&ctx, &dir, Some("offer-id"), None, None)
        .unwrap();
    assert_eq!(
        guidance.opening_message,
        "Start the step \"offer-id\" for feature \"demo\"."
    );
}

#[test]
fn new_chat_and_blank_messages_have_a_nonempty_opening() {
    let (_root, ctx, dir) = fixture();
    for message in [None, Some(""), Some(" \n\t")] {
        let guidance = FixedGuidance
            .authoring(&ctx, &dir, None, message, None)
            .unwrap();
        assert_eq!(
            guidance.opening_message,
            "Start a planning session for feature \"demo\"."
        );
    }
}

#[test]
fn writes_private_brief_and_minimal_plugin_with_fixed_launch_settings() {
    let (_root, ctx, dir) = fixture();
    let guidance = FixedGuidance
        .authoring(&ctx, &dir, None, None, None)
        .unwrap();
    let plugin = dir.plugin().join(".claude-plugin/plugin.json");
    assert_eq!(guidance.brief_path, dir.brief());
    assert_eq!(guidance.plugin_dir, dir.plugin());
    assert_eq!(
        fs::read_to_string(dir.brief()).unwrap(),
        session_brief(&ctx.project(), ctx.project().feature("demo"))
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&plugin).unwrap()).unwrap(),
        serde_json::json!({
            "name": "loom-plan", "version": "0.0.0", "description": "loom plan session"
        })
    );
    for path in [dir.brief(), plugin] {
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        FixedGuidance
            .authoring(&ctx, &dir, None, None, None)
            .unwrap();
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert_eq!(guidance.model, "opus");
    assert_eq!(guidance.effort, "medium");
    assert_eq!(guidance.display_name, "loom plan: chat");
    assert!(!guidance.opening_on_argv);
    assert_eq!(FixedGuidance.closing_message(), CLOSING_MESSAGE);
    assert!(CLOSING_MESSAGE.contains("loom-closing"));
}

#[test]
fn checklist_guidance_refuses_an_offer_the_checklist_does_not_offer() {
    let (_root, ctx, dir) = fixture();
    let error = ChecklistGuidance
        .authoring(&ctx, &dir, Some("offer-id"), None, None)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "the step \"offer-id\" is not offered now"
    );
    assert!(!dir.brief().exists());
    let guidance = ChecklistGuidance
        .authoring(&ctx, &dir, None, None, None)
        .unwrap();
    assert_eq!(
        guidance.opening_message,
        "Start a planning session for feature \"demo\"."
    );
}

#[test]
fn kickoff_ids_are_offered_without_a_matching_checklist_or_feature() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("doc")).unwrap();
    let bind = BindInfo::new("127.0.0.1:7272".parse().unwrap(), &"ab".repeat(32));
    let ctx = PlanContext::new(
        temp.path().to_path_buf(),
        ReadOnly::from_problems(Vec::new()),
        bind,
        OwnWrites::default(),
    )
    .unwrap();
    for id in ["kickoff", "kickoff-small"] {
        for feature in [None, Some("absent")] {
            assert!(offered(&ctx, id, feature), "{id}: {feature:?}");
        }
    }
    assert!(!offered(&ctx, "unknown", None));
}

#[test]
fn plan_follow_ups_is_offered_for_a_feature_with_a_metaplan_that_is_not_done() {
    let temp = tempfile::tempdir().unwrap();
    let plans = temp.path().join("doc/plans/demo");
    fs::create_dir_all(&plans).unwrap();
    fs::write(
        plans.join("index.md"),
        "# Metaplan\n\n## 1. One (`PLAN-one.md`)\n",
    )
    .unwrap();
    fs::write(plans.join("PLAN-one.md"), "# Plan: one\n").unwrap();
    let bind = BindInfo::new("127.0.0.1:7272".parse().unwrap(), &"ab".repeat(32));
    let ctx = PlanContext::new(
        temp.path().to_path_buf(),
        ReadOnly::from_problems(Vec::new()),
        bind,
        OwnWrites::default(),
    )
    .unwrap();
    assert!(offered(&ctx, "plan-follow-ups", Some("demo")));
    assert!(!offered(&ctx, "plan-follow-ups", Some("absent")));
    assert!(!offered(&ctx, "plan-follow-ups", None));
}

#[test]
fn refuses_a_symlinked_plugin_directory() {
    let (_root, ctx, dir) = fixture();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), dir.plugin()).unwrap();
    assert!(FixedGuidance
        .authoring(&ctx, &dir, None, None, None)
        .is_err());
    assert!(!outside.path().join(".claude-plugin").exists());
}

#[test]
fn slot_replaces_provider_without_invalidating_an_existing_snapshot() {
    let initial: Arc<dyn GuidanceProvider> = Arc::new(FixedGuidance);
    let replacement: Arc<dyn GuidanceProvider> = Arc::new(FixedGuidance);
    let slot = GuidanceSlot::new(Arc::clone(&initial));
    let snapshot = slot.get();
    slot.set(Arc::clone(&replacement));
    assert!(Arc::ptr_eq(&snapshot, &initial));
    assert!(Arc::ptr_eq(&slot.get(), &replacement));
}

#[test]
fn feature_guidance_names_its_vision_and_document_directories() {
    let (_root, ctx, dir) = fixture();

    let guidance = FixedGuidance
        .authoring(&ctx, &dir, None, None, None)
        .unwrap();

    let brief = fs::read_to_string(guidance.brief_path).unwrap();
    assert!(brief.contains("Feature \"demo\""));
    assert!(brief.contains("vision doc/product/demo/index.md"));
    assert!(brief.contains("product sections under doc/product/demo"));
    assert!(brief.contains("metaplans and plans under doc/plans/demo"));
}

#[test]
fn an_offer_shared_by_two_features_asks_the_session_to_select() {
    let (root, ctx, dir) = fixture();
    fs::create_dir_all(root.path().join("doc/product/other")).unwrap();
    fs::write(root.path().join("doc/product/other/notes.md"), "# Notes\n").unwrap();
    let batch = crate::planning::watch::Batch {
        added: vec!["doc/product/other/notes.md".into()],
        ..Default::default()
    };
    ctx.project_mut().refresh(&batch).unwrap();

    ChecklistGuidance
        .authoring(&ctx, &dir, Some("start-metaplan"), None, None)
        .unwrap();

    let brief = fs::read_to_string(dir.brief()).unwrap();
    assert!(brief.contains("Feature \"demo\""));
    assert!(brief.contains("Feature \"other\""));
    assert!(brief.contains("Ask which feature"));
}

#[test]
fn a_chat_without_a_selected_feature_lists_each_feature_path() {
    let (root, ctx, dir) = fixture();
    fs::create_dir_all(root.path().join("doc/product/other")).unwrap();
    fs::write(root.path().join("doc/product/other/notes.md"), "# Notes\n").unwrap();
    let batch = crate::planning::watch::Batch {
        added: vec!["doc/product/other/notes.md".into()],
        ..Default::default()
    };
    ctx.project_mut().refresh(&batch).unwrap();

    FixedGuidance
        .authoring(&ctx, &dir, None, None, None)
        .unwrap();

    let brief = fs::read_to_string(dir.brief()).unwrap();
    assert!(brief.contains("vision doc/product/demo/index.md"));
    assert!(brief.contains("vision doc/product/other/index.md"));
    assert!(brief.contains("Ask which feature"));
}

#[test]
fn an_explicit_feature_resolves_a_shared_offer_in_the_fixed_provider() {
    let (root, ctx, dir) = fixture();
    fs::create_dir_all(root.path().join("doc/product/other")).unwrap();
    fs::write(root.path().join("doc/product/other/notes.md"), "# Notes\n").unwrap();
    let batch = crate::planning::watch::Batch {
        added: vec!["doc/product/other/notes.md".into()],
        ..Default::default()
    };
    ctx.project_mut().refresh(&batch).unwrap();

    let guidance = FixedGuidance
        .authoring(&ctx, &dir, Some("start-metaplan"), None, Some("other"))
        .unwrap();

    assert_eq!(
        guidance.opening_message,
        "Start the step \"start-metaplan\" for feature \"other\"."
    );
    let brief = fs::read_to_string(dir.brief()).unwrap();
    assert!(brief.contains("Feature \"other\""));
    assert!(!brief.contains("Feature \"demo\""));
}

#[test]
fn checklist_provider_refuses_an_offer_from_another_feature() {
    let (root, ctx, dir) = fixture();
    fs::create_dir_all(root.path().join("doc/plans/other")).unwrap();
    fs::write(root.path().join("doc/plans/other/index.md"), "# Metaplan\n").unwrap();
    let batch = crate::planning::watch::Batch {
        added: vec!["doc/plans/other/index.md".into()],
        ..Default::default()
    };
    ctx.project_mut().refresh(&batch).unwrap();
    {
        let mut project = ctx.project_mut();
        let other = project
            .features
            .iter_mut()
            .find(|f| f.name == "other")
            .unwrap();
        other.checklist.primary = None;
        other.checklist.secondary.clear();
    }

    let error = ChecklistGuidance
        .authoring(&ctx, &dir, Some("start-metaplan"), None, Some("other"))
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "the step \"start-metaplan\" is not offered now"
    );
    assert!(!dir.brief().exists());
}
