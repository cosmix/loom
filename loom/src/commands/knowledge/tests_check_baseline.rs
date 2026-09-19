//! Tests for the `check --baseline` ratchet, tier-2 size limits and
//! cross-file duplicate headings.

use super::*;
use std::fs;
use tempfile::TempDir;

fn oversized_file(file: &str, lines: usize) -> CatalogIssue {
    CatalogIssue::OversizedFile {
        file: PathBuf::from(file),
        lines,
    }
}

fn broken_link(file: &str, target: &str) -> CatalogIssue {
    CatalogIssue::BrokenLink {
        file: PathBuf::from(file),
        target: target.to_string(),
    }
}

fn knowledge_root_with(files: &[(&str, String)]) -> TempDir {
    let temp = TempDir::new().expect("temp dir");
    for (path, content) in files {
        let path = temp.path().join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
        fs::write(path, content).expect("write knowledge file");
    }
    temp
}

#[test]
fn a_rendered_baseline_records_every_structural_issue_and_tolerates_them() {
    let issues = vec![
        oversized_file("architecture/big.md", 450),
        broken_link("patterns.md", "missing.md"),
    ];
    let baseline = CheckBaseline::parse(&catalog::render_baseline(&issues)).expect("parse");

    let comparison = baseline.compare(&issues);

    assert!(comparison.new.is_empty(), "new: {:?}", comparison.new);
    assert!(comparison.tightenable.is_empty());
    assert_eq!(
        strict_failure_count(true, false, comparison.new.len(), &issues),
        0
    );
}

#[test]
fn an_issue_absent_from_the_baseline_fails_strict() {
    let recorded = vec![broken_link("patterns.md", "missing.md")];
    let baseline = CheckBaseline::parse(&catalog::render_baseline(&recorded)).expect("parse");
    let current = vec![
        broken_link("patterns.md", "missing.md"),
        broken_link("patterns.md", "also-missing.md"),
    ];

    let comparison = baseline.compare(&current);

    assert_eq!(comparison.new, vec![&current[1]]);
    assert_eq!(
        strict_failure_count(true, false, comparison.new.len(), &current),
        1
    );
    let report = baseline_report(Path::new("baseline.txt"), &comparison);
    assert_eq!(report.len(), 1);
    assert!(report[0].contains("also-missing.md"), "report: {report:?}");
}

#[test]
fn a_recorded_issue_keeps_matching_when_its_line_count_moves() {
    let baseline = CheckBaseline::parse(&catalog::render_baseline(&[oversized_file(
        "topic/a.md",
        450,
    )]))
    .expect("parse");

    let current = [oversized_file("topic/a.md", 470)];
    let comparison = baseline.compare(&current);

    assert!(comparison.new.is_empty());
}

#[test]
fn a_recorded_issue_that_is_gone_prints_one_tightening_line_and_does_not_fail() {
    let recorded = vec![
        broken_link("patterns.md", "missing.md"),
        oversized_file("topic/a.md", 450),
    ];
    let baseline = CheckBaseline::parse(&catalog::render_baseline(&recorded)).expect("parse");

    let comparison = baseline.compare(&[]);

    assert_eq!(comparison.tightenable.len(), 2);
    assert_eq!(
        strict_failure_count(true, false, comparison.new.len(), &[]),
        0
    );
    let report = baseline_report(Path::new("baseline.txt"), &comparison);
    assert_eq!(report.len(), 1, "report: {report:?}");
    assert!(report[0].starts_with("baseline can be tightened"));
}

#[test]
fn review_only_issues_are_never_written_to_a_baseline() {
    let issues = vec![CatalogIssue::DuplicateHeadingAcrossFiles {
        heading: "Shared long heading".to_string(),
        files: vec![PathBuf::from("a/x.md"), PathBuf::from("b/y.md")],
    }];

    let text = catalog::render_baseline(&issues);

    assert!(text.lines().all(|line| line.starts_with('#')), "{text}");
}

#[test]
fn baseline_parsing_ignores_comments_and_blanks_and_rejects_duplicates() {
    let clean = CheckBaseline::parse("# note\n\nbroken-link patterns.md x.md\n").expect("parse");
    assert!(clean
        .compare(&[broken_link("patterns.md", "x.md")])
        .new
        .is_empty());

    let errors = CheckBaseline::parse("oversized-file a/b.md\noversized-file a/b.md\n")
        .expect_err("duplicate must be rejected");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("line 2"), "{errors:?}");
}

#[test]
fn a_missing_baseline_file_is_an_empty_baseline() {
    let temp = TempDir::new().expect("temp dir");
    let baseline = CheckBaseline::read(&temp.path().join("absent.txt")).expect("read");

    let issues = [broken_link("patterns.md", "x.md")];
    assert_eq!(baseline.compare(&issues).new.len(), 1);
}

#[test]
fn tier_two_files_and_sections_over_their_limits_are_reported() {
    let long_section = format!("## Long section\n{}", "line\n".repeat(85));
    let long_file = format!("# Big\n\n> Blurb.\n\n{}", "text\n".repeat(401));
    let temp = knowledge_root_with(&[
        ("architecture/sections.md", format!("# S\n\n{long_section}")),
        ("architecture/big.md", long_file),
        (
            "architecture/fine.md",
            "# F\n\n## Short\n\nok\n".to_string(),
        ),
    ]);

    let issues = catalog::build(temp.path()).expect("build").issues;

    assert!(issues.iter().any(|issue| matches!(issue,
        CatalogIssue::OversizedSection { file, .. } if file == Path::new("architecture/sections.md"))));
    assert!(issues.iter().any(|issue| matches!(issue,
        CatalogIssue::OversizedFile { file, .. } if file == Path::new("architecture/big.md"))));
    assert!(!issues.iter().any(|issue| matches!(issue,
        CatalogIssue::OversizedSection { file, .. } | CatalogIssue::OversizedFile { file, .. }
            if file == Path::new("architecture/fine.md"))));
}

#[test]
fn a_tier_two_size_line_names_the_tier_two_limit() {
    let line = issue_line(&oversized_file("architecture/big.md", 420));
    assert!(line.contains("tier-2"), "line: {line}");
    assert!(line.contains("400"), "line: {line}");
}

/// `--write-baseline` writes the current issue set and exits before honouring
/// any other flag - combining it with `--strict`/`--strict-evidence`/`--json`/
/// `--baseline` must error instead of silently writing and ignoring the rest.
#[test]
fn write_baseline_combined_with_strict_errors_and_writes_no_file() {
    let temp = TempDir::new().expect("temp dir");
    let baseline_path = temp.path().join("baseline.txt");

    let options = CheckOptions {
        strict: true,
        write_baseline: Some(baseline_path.clone()),
        ..CheckOptions::default()
    };
    let result = check(options);

    assert!(result.is_err(), "--write-baseline with --strict must error");
    assert!(
        !baseline_path.exists(),
        "no baseline file must be written when the combination is rejected"
    );
}

#[test]
fn a_long_heading_in_two_files_is_a_review_note_and_short_or_shared_ones_are_not() {
    let body =
        "# T\n\n## Worktree isolation rules\n\nx\n\n## Related\n\ny\n\n## What Happened\n\nz\n";
    let temp = knowledge_root_with(&[
        ("architecture/one.md", body.to_string()),
        ("mistakes/two.md", body.to_string()),
    ]);

    let issues = catalog::build(temp.path()).expect("build").issues;
    let across: Vec<_> = issues
        .iter()
        .filter(|issue| matches!(issue, CatalogIssue::DuplicateHeadingAcrossFiles { .. }))
        .collect();

    assert_eq!(across.len(), 1, "issues: {issues:?}");
    assert!(across[0].is_review_only());
    let line = issue_line(across[0]);
    assert!(line.starts_with("note:"), "line: {line}");
    assert!(line.contains("architecture/one.md"), "line: {line}");
    assert!(line.contains("mistakes/two.md"), "line: {line}");
}
