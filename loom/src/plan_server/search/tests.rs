//! Search indexing and updates.

use std::fs;
use std::path::Path;

use super::index::{Scope, SearchIndex};
use crate::plan_server::files::texts::TextStore;
use crate::planning::project::Project;
use crate::planning::watch::Batch;
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
