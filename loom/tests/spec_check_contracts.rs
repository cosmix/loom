//! Frozen contracts for the spec check: the sections a plan implements, and the run review's
//! Open Findings section, written by `fs::plan_review::generate` and read back by its parser.

use loom::fs::memory::{append_entry, MemoryEntry, MemoryEntryType, Receipt, ReceiptOutcome};
use loom::fs::plan_review::generate;
use loom::planning::implements::implemented_section_paths;
use loom::planning::run_review::{review_items, ReviewItem, ReviewSection};
use std::fs;
use std::path::{Path, PathBuf};

fn put(root: &Path, rel: &str, content: &str) {
    fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    fs::write(root.join(rel), content).unwrap();
}

/// A work dir under `root` for plan `PLAN-x` with the given `(file, frontmatter body)` stages.
fn work_dir(root: &Path, stages: &[(&str, &str)]) -> PathBuf {
    let work = root.join("work");
    put(
        &work,
        "config.toml",
        "[plan]\nplan_id = \"PLAN-x\"\nplan_name = \"X\"\n",
    );
    for (file, body) in stages {
        put(
            &work,
            &format!("stages/{file}"),
            &format!("---\n{body}---\n"),
        );
    }
    work
}

fn review_text(work: &Path, root: &Path) -> String {
    let path = generate(work, root, root, false).unwrap();
    fs::read_to_string(path).unwrap()
}

fn record(work: &Path, stage: &str, kind: MemoryEntryType, text: &str) -> String {
    let entry = MemoryEntry::new(kind, text.into());
    append_entry(work, stage, &entry).unwrap();
    entry.id
}

fn settle(work: &Path, event_id: String, outcome: ReceiptOutcome, target: Option<&str>) {
    let receipt = Receipt {
        event_id,
        outcome,
        target: target.map(String::from),
    };
    let entry = MemoryEntry::receipt(receipt, format!("settled as {outcome}"));
    append_entry(work, "knowledge-distill", &entry).unwrap();
}

#[test]
fn a_feature_plan_lists_its_implemented_sections() {
    let running = implemented_section_paths(
        Path::new("doc/plans/alpha/IN_PROGRESS-PLAN-x.md"),
        "# X\n\nImplements: section_02, section_10, section_02\n",
    );
    assert_eq!(
        running,
        [
            PathBuf::from("doc/product/alpha/section_02.md"),
            PathBuf::from("doc/product/alpha/section_10.md"),
        ]
    );
    let archived = implemented_section_paths(
        Path::new("doc/plans/alpha/archive/DONE-PLAN-x.md"),
        "# X\n\nImplements: section_03\n",
    );
    assert_eq!(archived, [PathBuf::from("doc/product/alpha/section_03.md")]);
}

#[test]
fn an_unmapped_or_standalone_plan_lists_no_sections() {
    let cases = [
        ("doc/plans/alpha/PLAN-x.md", "# X\n\nImplements: none\n"),
        ("doc/plans/alpha/PLAN-x.md", "# X\n\nBody.\n"),
        ("doc/plans/PLAN-x.md", "# X\n\nImplements: section_02\n"),
        ("doc/plans/Bad/PLAN-x.md", "# X\n\nImplements: section_02\n"),
    ];
    for (path, text) in cases {
        let sections = implemented_section_paths(Path::new(path), text);
        assert!(sections.is_empty(), "{path} {text:?}: {sections:?}");
    }
}

#[test]
fn the_run_review_round_trips_its_open_findings() {
    let stage = (
        "01-build.md",
        "id: build\nname: \"Build **it**: part (one)\"\nstatus: Completed\n",
    );
    let root = tempfile::tempdir().unwrap();
    let work = work_dir(root.path(), &[stage]);
    let text = "Rename the flag\nand its `**` docs: soon";
    record(&work, "build", MemoryEntryType::Suggestion, text);
    let review = review_text(&work, root.path());
    let items: Vec<ReviewItem> = review_items(&review, ReviewSection::OpenFindings);
    let fields: Vec<(&str, &str, &str)> = items
        .iter()
        .map(|i| (i.stage_name.as_str(), i.stage_id.as_str(), i.text.as_str()))
        .collect();
    assert_eq!(
        fields,
        [(
            "Build **it**: part (one)",
            "build",
            "Rename the flag and its `**` docs: soon"
        )],
        "review:\n{review}"
    );

    let empty_root = tempfile::tempdir().unwrap();
    let empty_work = work_dir(empty_root.path(), &[stage]);
    let review = review_text(&empty_work, empty_root.path());
    assert!(
        review.contains("## Open Findings\n\nNo open findings."),
        "review:\n{review}"
    );
    assert!(review_items(&review, ReviewSection::OpenFindings).is_empty());
}

#[test]
fn the_run_review_lists_unimplemented_suggestions_and_deferred_entries() {
    let root = tempfile::tempdir().unwrap();
    let stages = [
        ("01-build.md", "id: build\nname: Build\n"),
        (
            "02-knowledge-distill.md",
            "id: knowledge-distill\nname: Knowledge distill\n",
        ),
    ];
    let work = work_dir(root.path(), &stages);
    let suggestion = MemoryEntryType::Suggestion;
    let note = MemoryEntryType::Note;
    record(&work, "build", suggestion, "pending one");
    let promoted = record(&work, "build", suggestion, "promoted one");
    let implemented = record(&work, "build", suggestion, "implemented one");
    let discarded = record(&work, "build", suggestion, "discarded one");
    let deferred = record(&work, "build", note, "deferred note");
    let promoted_note = record(&work, "build", note, "promoted note");
    record(&work, "build", MemoryEntryType::Question, "a question");
    settle(
        &work,
        promoted,
        ReceiptOutcome::Promoted,
        Some("concerns.md#x"),
    );
    settle(&work, implemented, ReceiptOutcome::Implemented, None);
    settle(&work, discarded, ReceiptOutcome::Discarded, None);
    settle(&work, deferred, ReceiptOutcome::Deferred, None);
    settle(
        &work,
        promoted_note,
        ReceiptOutcome::Promoted,
        Some("patterns.md#y"),
    );

    let review = review_text(&work, root.path());
    let items = review_items(&review, ReviewSection::OpenFindings);
    let texts: Vec<&str> = items.iter().map(|i| i.text.as_str()).collect();
    assert_eq!(
        texts,
        ["pending one", "promoted one", "deferred note"],
        "review:\n{review}"
    );
    for item in &items {
        assert_eq!(
            (item.stage_id.as_str(), item.stage_name.as_str()),
            ("build", "Build")
        );
    }
}
