//! Tests for files::doc: lock notices, truncation, repo-relative paths, refusals.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tempfile::TempDir;

use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::files::doc::{
    checked_path, doc_view, DONE_NOTICE, KNOWLEDGE_NOTICE, MAX_EDIT_BYTES, RUNNING_NOTICE,
};
use crate::plan_server::files::texts::TextStore;
use crate::plan_server::files::validate::ValidationCache;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::wire::files::{
    DocView, EntryKind, LockKind, MetadataView, ValidationView, ValidationWarnings,
};
use crate::planning::sections::normalized_hash;

/// A plan with one stage, enough for the locked set's input scan.
pub(super) const PLAN: &str = "# Plan X\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 1\n  \
     stages:\n    - id: stage-one\n      name: \"Stage One\"\n      stage_type: standard\n      \
     working_dir: \".\"\n      acceptance:\n        - \"true\"\n```\n\n<!-- END loom METADATA -->\n";

/// Write `doc/product/index.md` and `files` under a fresh temp root.
pub(super) fn write_files(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let index = [("doc/product/index.md", "# Demo\n")];
    for (rel, text) in index.iter().chain(files) {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }
    dir
}

/// A planning context over `dir` with the text store installed, as `files::install` does.
pub(super) fn context(dir: &TempDir) -> PlanContext {
    let local = SocketAddr::from(([127, 0, 0, 1], 0));
    let read_only = ReadOnly::from_problems(Vec::new());
    let ctx = PlanContext::new(
        dir.path().to_path_buf(),
        read_only,
        BindInfo::new(local, "t"),
    )
    .expect("context");
    let store = {
        let project = ctx.project();
        TextStore::load(ctx.root(), &project).expect("text store")
    };
    ctx.extensions().insert(Arc::new(store));
    ctx
}

fn view(ctx: &PlanContext, raw: &str) -> DocView {
    let rel = checked_path(Some(raw)).expect("syntax");
    doc_view(ctx, &rel).unwrap_or_else(|refusal| panic!("{raw}: {refusal:?}"))
}

#[test]
fn plain_section_is_editable_and_paths_are_repo_relative() {
    let text = "# Section 1\n\n## Purpose\nWhy.\n";
    let dir = write_files(&[("doc/product/section_01.md", text)]);
    let ctx = context(&dir);
    let doc = view(&ctx, "product/section_01.md");
    assert_eq!(doc.path, "doc/product/section_01.md");
    assert_eq!(doc.title, "Section 1");
    assert_eq!(doc.kind, EntryKind::Section);
    assert_eq!(doc.text, text);
    assert_eq!(doc.hash, normalized_hash(text));
    assert!(doc.editable && doc.lock.is_none() && !doc.too_large && !doc.truncated);
    let purpose = doc
        .sections
        .iter()
        .find(|s| s.anchor == "purpose")
        .expect("purpose");
    assert_eq!(
        (purpose.start_line, purpose.end_line),
        (3, 4),
        "1-based, end inclusive"
    );
}

fn locked_fixture() -> TempDir {
    write_files(&[
        ("doc/plans/DONE-PLAN-old.md", PLAN),
        ("doc/plans/RETIRED-PLAN-gone.md", PLAN),
        ("doc/plans/IN_PROGRESS-PLAN-run.md", PLAN),
        ("doc/plans/briefs/run/common.md", "# Common\n"),
        ("doc/plans/archive/PLAN-older.md", PLAN),
        ("doc/plans/archive/notes.md", "# Notes\n"),
        ("doc/loom/knowledge/INDEX.md", "# Index\n"),
    ])
}

#[test]
fn locked_documents_carry_the_verbatim_notices() {
    let dir = locked_fixture();
    let ctx = context(&dir);
    let cases = [
        ("plans/DONE-PLAN-old.md", LockKind::Done, DONE_NOTICE),
        ("plans/RETIRED-PLAN-gone.md", LockKind::Done, DONE_NOTICE),
        (
            "plans/IN_PROGRESS-PLAN-run.md",
            LockKind::Running,
            RUNNING_NOTICE,
        ),
        // Plan 1 locks an archived plan by its state (done) before the archive rule.
        ("plans/archive/PLAN-older.md", LockKind::Done, DONE_NOTICE),
        ("plans/archive/notes.md", LockKind::Archive, DONE_NOTICE),
        (
            "loom/knowledge/INDEX.md",
            LockKind::Knowledge,
            KNOWLEDGE_NOTICE,
        ),
    ];
    for (raw, reason, notice) in cases {
        let doc = view(&ctx, raw);
        let lock = doc.lock.unwrap_or_else(|| panic!("{raw} is locked"));
        assert_eq!(
            (lock.reason, lock.notice.as_str()),
            (reason, notice),
            "{raw}"
        );
        assert!(!doc.editable, "{raw}");
    }
    let brief = view(&ctx, "plans/briefs/run/common.md");
    let lock = brief.lock.expect("a brief of a running plan is locked");
    assert_eq!(lock.reason, LockKind::BriefOfLockedPlan);
    assert_eq!(
        lock.notice,
        "The plan `run` is running. `loom run` works from the committed copy."
    );
    assert_eq!(
        lock.plan.as_deref(),
        Some("doc/plans/IN_PROGRESS-PLAN-run.md")
    );
    assert!(!brief.editable);
}

#[test]
fn a_plan_answers_its_plan_view() {
    let dir = write_files(&[("doc/plans/PLAN-x.md", PLAN)]);
    let ctx = context(&dir);
    let cached = ValidationView {
        valid: true,
        errors: Vec::new(),
        warnings: ValidationWarnings {
            structural: vec!["cached".to_owned()],
            knowledge: Vec::new(),
            sandbox: Vec::new(),
            baseline: Vec::new(),
        },
        notes: Vec::new(),
        metadata: MetadataView::Present,
    };
    let cache = HashMap::from([(PathBuf::from("doc/plans/PLAN-x.md"), cached.clone())]);
    ctx.extensions()
        .insert(Arc::new(ValidationCache::new(cache)));
    let doc = view(&ctx, "plans/PLAN-x.md");
    assert_eq!(doc.kind, EntryKind::Plan);
    assert!(doc.editable);
    let plan = doc.plan.expect("a plan has a plan view");
    assert_eq!(plan.slug, "x");
    assert_eq!(
        plan.validation,
        Some(cached),
        "validation comes from the cache"
    );
    let stage = plan.stages.first().expect("one stage");
    assert_eq!(stage.id, "stage-one");
    let id_line = PLAN
        .lines()
        .position(|l| l.contains("- id: stage-one"))
        .expect("id line");
    assert_eq!(stage.line, Some(u32::try_from(id_line + 1).expect("line")));
}

#[test]
fn over_one_mib_is_cut_on_a_char_boundary() {
    // 7 header bytes, then two-byte chars: byte 1 MiB falls inside a char.
    let full = format!("# Big\n\n{}", "é".repeat(MAX_EDIT_BYTES / 2 + 10));
    let dir = write_files(&[("doc/product/big.md", &full)]);
    let ctx = context(&dir);
    let doc = view(&ctx, "product/big.md");
    assert!(doc.too_large && doc.truncated && !doc.editable);
    assert_eq!(doc.size, u64::try_from(full.len()).expect("size"));
    assert_eq!(doc.text.len(), MAX_EDIT_BYTES - 1);
    assert!(full.starts_with(&doc.text));
    assert_eq!(
        doc.hash,
        normalized_hash(&full),
        "the hash is the whole file's"
    );
}

#[test]
fn a_symlink_inside_doc_is_served_through_the_store() {
    let dir = write_files(&[("doc/product/a.md", "# A\n\nTarget.\n")]);
    std::fs::create_dir_all(dir.path().join("doc/notes")).expect("mkdir");
    std::os::unix::fs::symlink("../product/a.md", dir.path().join("doc/notes/link.md"))
        .expect("symlink");
    let ctx = context(&dir);
    let doc = view(&ctx, "notes/link.md");
    assert_eq!(doc.path, "doc/notes/link.md");
    assert!(doc.text.contains("Target."));
}

#[test]
fn bad_and_undiscovered_paths_are_refused() {
    let dir = write_files(&[(".env", "SECRET=hunter2\n")]);
    std::fs::create_dir_all(dir.path().join("doc/notes")).expect("mkdir");
    std::os::unix::fs::symlink("../../.env", dir.path().join("doc/notes/out.md")).expect("symlink");
    let ctx = context(&dir);
    for raw in [
        "../.env",
        "product/../../.env",
        "/etc/passwd",
        "a//b.md",
        "",
    ] {
        let refusal = checked_path(Some(raw)).expect_err(raw);
        assert_eq!(refusal.status, 400, "{raw}");
    }
    assert_eq!(checked_path(None).expect_err("missing").status, 400);
    for raw in ["notes/out.md", "product/missing.md"] {
        let rel = checked_path(Some(raw)).expect("syntax");
        let refusal = doc_view(&ctx, &rel).expect_err(raw);
        assert_eq!(refusal.status, 404, "{raw}");
        assert!(!String::from_utf8_lossy(&refusal.body).contains("hunter2"));
    }
    assert!(
        doc_view(&ctx, Path::new(".env")).is_err(),
        "outside doc/ is never served"
    );
}
