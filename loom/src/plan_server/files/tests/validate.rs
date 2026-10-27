//! Plan validation mapped to the wire, and the cache's plan selection.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::doc::{context, write_files};
use crate::plan_server::files::plan_view::plan_view;
use crate::plan_server::files::texts::TextStore;
use crate::plan_server::files::validate::{
    is_plan_file, validate_plan, CachedValidation, ValidationCache, NO_METADATA_NOTE,
};
use crate::plan_server::wire::files::{MetadataView, ValidationView};
use crate::planning::sections::sections_of;
use crate::planning::watch::Batch;

const PLAN: &str = "doc/plans/PLAN-x.md";

/// A plan of one stage; `extra` is appended to the stage's YAML lines.
pub(super) fn plan(working_dir: &str, acceptance: &str, extra: &str) -> String {
    format!(
        "# Plan X\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 1\n  stages:\n    \
         - id: stage-one\n      name: \"Stage One\"\n      stage_type: standard\n      \
         working_dir: \"{working_dir}\"\n      acceptance:\n        - \"{acceptance}\"\n\
         {extra}```\n\n<!-- END loom METADATA -->\n"
    )
}

pub(super) const SELF_DEPENDENCY: &str = "      dependencies:\n        - stage-one\n";

/// The 1-based number of the first line of `text` that equals `wanted` once trimmed.
fn line_of(text: &str, wanted: &str) -> u32 {
    let index = text.lines().position(|line| line.trim() == wanted).unwrap();
    u32::try_from(index + 1).unwrap()
}

fn root() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("doc/plans")).unwrap();
    root
}

#[test]
fn only_written_plan_files_are_validated() {
    assert!(is_plan_file(Path::new("doc/plans/PLAN-x.md")));
    for other in [
        "doc/plans/DONE-PLAN-x.md",
        "doc/plans/IN_PROGRESS-PLAN-x.md",
        "doc/plans/archive/PLAN-x.md",
        "doc/plans/briefs/x/PLAN-y.md",
        "doc/plans/PLAN-x.txt",
        "doc/product/section_01.md",
    ] {
        assert!(!is_plan_file(Path::new(other)), "{other}");
    }
}

#[test]
fn a_plan_without_metadata_is_valid_prose_with_a_note() {
    let root = root();
    let view = validate_plan(root.path(), Path::new(PLAN), "# Plan X\n\nJust prose.\n");
    assert!(view.valid);
    assert!(view.errors.is_empty());
    assert_eq!(view.metadata, MetadataView::Missing);
    assert_eq!(view.notes, vec![NO_METADATA_NOTE.to_owned()]);
}

#[test]
fn a_stage_error_points_at_the_stage_id_line() {
    let root = root();
    let text = plan(".", "true", SELF_DEPENDENCY);
    let view = validate_plan(root.path(), Path::new(PLAN), &text);
    assert!(!view.valid);
    assert_eq!(view.metadata, MetadataView::Present);
    let error = view
        .errors
        .iter()
        .find(|error| error.stage_id.as_deref() == Some("stage-one"))
        .expect("a stage error");
    assert_eq!(error.line, Some(line_of(&text, "- id: stage-one")));
}

#[test]
fn warnings_leave_a_plan_valid() {
    let root = root();
    let text = plan("loom", "loom/target/debug/loom --help", "");
    // The input checks need the plan itself on disk.
    std::fs::write(root.path().join(PLAN), &text).unwrap();
    let view = validate_plan(root.path(), Path::new(PLAN), &text);
    assert!(view.valid, "{view:?}");
    assert!(!view.warnings.structural.is_empty(), "{view:?}");
}

#[test]
fn a_yaml_error_is_located_in_the_file_and_matches_the_stages_panel() {
    let root = root();
    let text = plan(".", "true", "").replace("name: \"Stage One\"", "name: a: b");
    let view = validate_plan(root.path(), Path::new(PLAN), &text);
    assert!(!view.valid);
    assert_eq!(view.errors.len(), 1, "{view:?}");
    let error = &view.errors[0];
    assert!(error.message.starts_with("YAML parse error"), "{error:?}");
    assert_eq!(error.line, Some(line_of(&text, "name: a: b")));
    assert!(error.column.is_some_and(|column| column > 1), "{error:?}");
    let panel = plan_view(
        root.path(),
        Path::new(PLAN),
        &text,
        &sections_of(&text),
        &BTreeMap::new(),
        None,
    );
    assert!(panel.stages.is_empty());
    let panel_error = panel
        .yaml_error
        .expect("the panel reports the same parse failure");
    assert_eq!(panel_error, error.message);
}

/// Every message of a view: errors, warnings and notes.
fn messages(view: &ValidationView) -> Vec<String> {
    let warnings = &view.warnings;
    let buckets = [
        &warnings.structural,
        &warnings.knowledge,
        &warnings.sandbox,
        &warnings.baseline,
        &view.notes,
    ];
    let errors = view.errors.iter().map(|error| error.message.clone());
    errors
        .chain(buckets.into_iter().flatten().cloned())
        .collect()
}

#[test]
fn messages_never_name_the_host_root() {
    let root = root();
    let bundle = root.path().join("doc/plans/briefs/x");
    std::fs::create_dir_all(&bundle).unwrap();
    // Not UTF-8: input discovery fails naming the brief by its absolute path.
    std::fs::write(bundle.join("bad.md"), [0xff, 0xfe, b'\n']).unwrap();
    let view = validate_plan(root.path(), Path::new(PLAN), &plan(".", "true", ""));
    let all = messages(&view);
    assert!(
        all.iter().any(|m| m.contains("doc/plans/briefs/x/bad.md")),
        "{all:?}"
    );
    let canonical = root.path().canonicalize().unwrap();
    for host in [root.path(), canonical.as_path()] {
        let host = host.to_string_lossy();
        assert!(!all.iter().any(|m| m.contains(host.as_ref())), "{all:?}");
    }
}

const SHARED_BRIEF: &str = "doc/plans/briefs/shared/extra.md";

/// A plan whose stage names `SHARED_BRIEF`, an input outside its bundle.
fn plan_with_shared_brief() -> String {
    let extra = format!("      description: \"See {SHARED_BRIEF} for context.\"\n");
    plan(".", "true", &extra)
}

fn mentions_shared_brief(cache: &ValidationCache) -> bool {
    let view = cache.view(Path::new(PLAN)).expect("the plan is cached");
    view.errors.iter().any(|e| e.message.contains(SHARED_BRIEF))
}

fn touched(paths: &[&str]) -> BTreeSet<PathBuf> {
    paths.iter().map(PathBuf::from).collect()
}

#[test]
fn a_batch_revalidates_a_plan_only_when_it_touches_an_input() {
    let dir = write_files(&[(PLAN, &plan_with_shared_brief())]);
    let ctx = context(&dir);
    let texts = ctx.extensions().get::<TextStore>().unwrap();
    let cache = ValidationCache::default();
    cache.revalidate(&ctx, &texts, &BTreeSet::new());
    assert!(mentions_shared_brief(&cache), "the input is missing");

    std::fs::create_dir_all(dir.path().join("doc/plans/briefs/shared")).unwrap();
    std::fs::write(dir.path().join(SHARED_BRIEF), "# Shared\n").unwrap();
    std::fs::write(dir.path().join("doc/product/section_01.md"), "# One\n").unwrap();
    cache.revalidate(&ctx, &texts, &touched(&["doc/product/section_01.md"]));
    assert!(
        mentions_shared_brief(&cache),
        "an unrelated document leaves the cached validation untouched"
    );

    cache.revalidate(&ctx, &texts, &touched(&[SHARED_BRIEF]));
    assert!(
        !mentions_shared_brief(&cache),
        "a touched input revalidates the plan"
    );
}

#[test]
fn a_batch_revalidates_a_changed_plan_and_drops_unlisted_ones() {
    let dir = write_files(&[(PLAN, &plan(".", "true", ""))]);
    let ctx = context(&dir);
    let texts = ctx.extensions().get::<TextStore>().unwrap();
    let mut entries = HashMap::new();
    let gone = PathBuf::from("doc/plans/PLAN-gone.md");
    let stale = CachedValidation {
        view: validate_plan(dir.path(), &gone, "# Gone\n"),
        inputs: Default::default(),
    };
    entries.insert(gone.clone(), stale);
    let cache = ValidationCache::new(entries);

    cache.revalidate(&ctx, &texts, &BTreeSet::new());
    let counts = cache.error_counts();
    assert!(!counts.contains_key(&gone), "{counts:?}");
    assert_eq!(
        counts.get(Path::new(PLAN)),
        Some(&0),
        "a listed plan is validated"
    );

    std::fs::write(dir.path().join(PLAN), plan(".", "true", SELF_DEPENDENCY)).unwrap();
    let batch = Batch {
        changed: vec![PathBuf::from(PLAN)],
        ..Batch::default()
    };
    texts.apply(dir.path(), &batch);
    cache.revalidate(&ctx, &texts, &touched(&["doc/product/index.md"]));
    assert!(
        cache.view(Path::new(PLAN)).unwrap().valid,
        "not yet revalidated"
    );
    cache.revalidate(&ctx, &texts, &touched(&[PLAN]));
    assert!(!cache.view(Path::new(PLAN)).unwrap().valid);
}

const KNOWLEDGE_FILE: &str = "doc/loom/knowledge/architecture.md";

fn knowledge_warnings(cache: &ValidationCache) -> Vec<String> {
    let view = cache.view(Path::new(PLAN)).expect("the plan is cached");
    view.warnings.knowledge
}

#[test]
fn a_batch_under_the_knowledge_tree_revalidates_every_plan() {
    let dir = write_files(&[(PLAN, &plan(".", "true", ""))]);
    let ctx = context(&dir);
    let texts = ctx.extensions().get::<TextStore>().unwrap();
    let cache = ValidationCache::default();
    cache.revalidate(&ctx, &texts, &BTreeSet::new());
    assert!(!knowledge_warnings(&cache).is_empty(), "the tree is empty");

    std::fs::create_dir_all(dir.path().join("doc/loom/knowledge")).unwrap();
    std::fs::write(
        dir.path().join(KNOWLEDGE_FILE),
        "# Architecture\n\n## Overview\nHow it fits.\n",
    )
    .unwrap();
    cache.revalidate(&ctx, &texts, &touched(&["doc/product/section_01.md"]));
    assert!(
        !knowledge_warnings(&cache).is_empty(),
        "an unrelated document leaves the cached validation untouched"
    );
    cache.revalidate(&ctx, &texts, &touched(&[KNOWLEDGE_FILE]));
    assert!(
        knowledge_warnings(&cache).is_empty(),
        "a knowledge file revalidates the plan: {:?}",
        cache.view(Path::new(PLAN))
    );
}

#[test]
fn a_failed_input_discovery_leaves_every_brief_watched() {
    let dir = write_files(&[(PLAN, &plan_with_shared_brief())]);
    std::fs::create_dir_all(dir.path().join("doc/plans/briefs/shared")).unwrap();
    // Not UTF-8: discovery fails reading the brief, which lies outside the plan's own bundle.
    std::fs::write(dir.path().join(SHARED_BRIEF), [0xff, 0xfe, b'\n']).unwrap();
    let ctx = context(&dir);
    let texts = ctx.extensions().get::<TextStore>().unwrap();
    let cache = ValidationCache::default();
    cache.revalidate(&ctx, &texts, &BTreeSet::new());
    assert!(
        mentions_shared_brief(&cache),
        "discovery fails on the brief"
    );

    std::fs::write(dir.path().join(SHARED_BRIEF), "# Shared\n").unwrap();
    cache.revalidate(&ctx, &texts, &touched(&[SHARED_BRIEF]));
    assert!(
        !mentions_shared_brief(&cache),
        "fixing the brief revalidates the plan: {:?}",
        cache.view(Path::new(PLAN))
    );
}
