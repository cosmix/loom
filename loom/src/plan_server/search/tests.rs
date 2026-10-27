//! Search indexing and updates.

use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use super::index::{Scope, SearchIndex};
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::files::texts::TextStore;
use crate::plan_server::prereq::ReadOnly;
use crate::planning::project::Project;
use crate::planning::watch::{Batch, OwnWrites};
use tempfile::TempDir;

fn write(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, text).unwrap();
}

fn setup(files: &[(&str, &str)]) -> (TempDir, Project, TextStore, SearchIndex) {
    let temp = TempDir::new().unwrap();
    for (path, text) in files {
        write(temp.path(), path, text);
    }
    let project = Project::load(temp.path()).unwrap();
    let texts = TextStore::load(temp.path(), &project).unwrap();
    let index = SearchIndex::build(&project, &texts);
    (temp, project, texts, index)
}

#[test]
fn scopes_include_done_and_archived_plans_and_exclude_summaries() {
    let (_, _, _, index) = setup(&[
        ("doc/product/index.md", "# Product\n\n## Topic\nquartz\n"),
        ("doc/plans/DONE-p.md", "# Done\n\n## Topic\nquartz\n"),
        (
            "doc/plans/archive/PLAN-old.md",
            "# Old\n\n## Topic\nquartz\n",
        ),
        (
            "doc/loom/knowledge/topic.md",
            "# Knowledge\n\n## Topic\nquartz\n",
        ),
        (
            "doc/loom/summaries/hidden.md",
            "# Hidden\n\n## Topic\nquartz\n",
        ),
    ]);
    assert_eq!(index.query("quartz", Scope::Product).hits.len(), 1);
    let plans = index.query("quartz", Scope::Plans);
    assert_eq!(plans.hits.len(), 2);
    assert!(plans.hits.iter().any(|h| h.path.contains("archive")));
    assert_eq!(index.query("quartz", Scope::Knowledge).hits.len(), 1);
    assert_eq!(index.query("quartz", Scope::All).hits.len(), 4);
    assert_eq!(plans.hits[0].heading_path.len(), 2);
    assert_eq!(plans.hits[0].anchor, "topic");
}

#[test]
fn literal_matching_is_case_sensitive_and_terms_are_tokenized() {
    let (_, _, _, index) = setup(&[
        (
            "doc/product/a.md",
            "# A\n\n## Topic\nIN_PROGRESS- example\n",
        ),
        ("doc/product/b.md", "# B\n\n## Topic\nprogress elsewhere\n"),
    ]);
    assert_eq!(index.query("\"IN_PROGRESS-\"", Scope::All).hits.len(), 1);
    assert!(index.query("\"in_progress-\"", Scope::All).hits.is_empty());
    assert_eq!(index.query("progress", Scope::All).hits.len(), 2);
    assert!(index.query("\"\"", Scope::All).hits.is_empty());
}

#[test]
fn results_are_capped_and_snippets_keep_unicode_boundaries() {
    let temp = TempDir::new().unwrap();
    for i in 0..60 {
        write(
            temp.path(),
            &format!("doc/product/{i}.md"),
            &format!(
                "# File {i}\n\n## Topic\n{}quartz{}\n",
                "é".repeat(200),
                "é".repeat(200)
            ),
        );
    }
    let project = Project::load(temp.path()).unwrap();
    let texts = TextStore::load(temp.path(), &project).unwrap();
    let results = SearchIndex::build(&project, &texts).query("\"quartz\"", Scope::All);
    assert_eq!(results.hits.len(), 50);
    assert!(results.truncated);
    assert!(results.hits[0].snippet.contains("quartz"));
    assert!(results.hits[0].snippet.starts_with('…'));
}

#[test]
fn apply_replaces_and_removes_changed_chunks() {
    let (temp, mut project, texts, index) =
        setup(&[("doc/product/a.md", "# A\n\n## Old\noldword\n")]);
    let path = "doc/product/a.md";
    write(temp.path(), path, "# A\n\n## New\nnewword\n");
    let batch = Batch {
        changed: vec![path.into()],
        ..Batch::default()
    };
    project.refresh(&batch).unwrap();
    texts.apply(temp.path(), &batch);
    index.apply(&project, &texts, &batch);
    assert!(index.query("oldword", Scope::All).hits.is_empty());
    assert_eq!(index.query("newword", Scope::All).hits[0].anchor, "new");
    fs::remove_file(temp.path().join(path)).unwrap();
    let batch = Batch {
        removed: vec![path.into()],
        ..Batch::default()
    };
    project.refresh(&batch).unwrap();
    texts.apply(temp.path(), &batch);
    index.apply(&project, &texts, &batch);
    assert!(index.query("newword", Scope::All).hits.is_empty());
}

#[test]
fn a_fence_the_chunker_and_the_sections_read_differently_keeps_every_anchor() {
    // The chunker closes the outer four-backtick fence at the inner three-backtick line, so
    // it splits at `## Fake`; the sections keep that line inside the fence.
    let text = "# Title\n\n## Alpha\nalphaword\n\n````md\n```\n## Fake\nfakeword\n```\n````\n\n\
                ## Beta\nbetaword\n\n## Gamma\ngammaword\n";
    let (_, _, _, index) = setup(&[("doc/product/a.md", text)]);
    let hit = |word: &str| index.query(word, Scope::All).hits.remove(0);
    let beta = hit("betaword");
    assert_eq!(beta.anchor, "beta");
    assert_eq!(beta.heading_path, ["Title", "Beta"]);
    let gamma = hit("gammaword");
    assert_eq!(gamma.anchor, "gamma");
    assert_eq!(gamma.heading_path, ["Title", "Gamma"]);
    // The chunk without a heading of its own belongs to the section it sits in.
    let fake = hit("fakeword");
    assert_eq!(fake.anchor, "alpha");
    assert_eq!(fake.heading_path, ["Title", "Alpha"]);
}

#[test]
fn a_literal_snippet_shows_the_case_sensitive_match_that_was_counted() {
    let body = format!("quartz{}Quartz", "x".repeat(400));
    let text = format!("# A\n\n## Topic\n{body}\n");
    let (_, _, _, index) = setup(&[("doc/product/a.md", text.as_str())]);
    let hits = index.query("\"Quartz\"", Scope::All).hits;
    assert_eq!(hits.len(), 1);
    assert!(hits[0].snippet.contains("Quartz"), "{}", hits[0].snippet);
    assert!(!hits[0].snippet.contains("quartz"), "{}", hits[0].snippet);
}

#[test]
fn install_without_a_text_store_installs_nothing() {
    let temp = TempDir::new().unwrap();
    write(temp.path(), "doc/product/index.md", "# Demo\n");
    let local = SocketAddr::from(([127, 0, 0, 1], 0));
    let read_only = ReadOnly::from_problems(Vec::new());
    let bind = BindInfo::new(local, "t");
    let root = temp.path().to_path_buf();
    let ctx = Arc::new(PlanContext::new(root, read_only, bind, OwnWrites::default()).unwrap());
    super::install(&ctx);
    assert!(ctx.extensions().get::<SearchIndex>().is_none());
}
