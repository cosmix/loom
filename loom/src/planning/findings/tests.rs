use super::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn entry(id: &str, title: &str, status: FindingStatus, text: &str) -> String {
    let about = vec!["section_01#one".to_owned(), "PLAN-a.md#stage:x".to_owned()];
    entry_text(id, title, &about, Some("review 2026-11-05"), &status, text)
}

fn ledger_of(entries: &[String]) -> String {
    format!("# Findings\n\n{}", entries.join("\n"))
}

#[test]
fn entry_text_round_trips_through_parse() {
    let status = FindingStatus::Planned("PLAN-next.md".into());
    let text = ledger_of(&[entry("F-1", "Rebuild budget", status.clone(), "Body text.")]);
    let ledger = parse(&text);
    assert_eq!(ledger.problems, Vec::new());
    assert_eq!(
        ledger.findings,
        vec![Finding {
            id: "F-1".into(),
            title: "Rebuild budget".into(),
            about: vec!["section_01#one".into(), "PLAN-a.md#stage:x".into()],
            source: Some("review 2026-11-05".into()),
            status,
            text: "Body text.".into(),
        }]
    );
    assert_eq!(next_id(&ledger), "F-2");
}

#[test]
fn every_status_round_trips() {
    for status in [
        FindingStatus::Open,
        FindingStatus::Planned("PLAN-a.md".into()),
        FindingStatus::Folded("metaplan#two".into()),
        FindingStatus::Dismissed("not worth it".into()),
    ] {
        let parsed = parse(&ledger_of(&[entry("F-1", "T", status.clone(), "x")]));
        assert_eq!(parsed.findings[0].status, status);
        assert_eq!(parsed.problems, Vec::new());
    }
}

#[test]
fn set_status_leaves_other_entries_byte_for_byte() {
    let first = entry("F-1", "One", FindingStatus::Open, "first");
    let second = entry("F-2", "Two", FindingStatus::Open, "second");
    let third = entry("F-3", "Three", FindingStatus::Open, "third");
    let text = ledger_of(&[first.clone(), second.clone(), third.clone()]);
    let status = FindingStatus::Dismissed("done elsewhere".into());
    let changed = set_status(&text, "F-2", &status).unwrap();
    let expected = ledger_of(&[
        first,
        second.replace("Status: open", "Status: dismissed: done elsewhere"),
        third,
    ]);
    assert_eq!(changed, expected);
    assert_eq!(parse(&changed).findings[1].status, status);
}

#[test]
fn set_status_inserts_a_missing_status_line() {
    let text =
        "# Findings\n\n## F-1 One\n\nAbout: section_01\n\nText.\n\n## F-2 Two\n\nText two.\n";
    let status = FindingStatus::Folded("metaplan#a".into());
    let one = set_status(text, "F-1", &status).unwrap();
    assert_eq!(
        one,
        "# Findings\n\n## F-1 One\n\nAbout: section_01\nStatus: folded metaplan#a\n\nText.\n\n\
         ## F-2 Two\n\nText two.\n"
    );
    let two = set_status(text, "F-2", &status).unwrap();
    assert!(
        two.ends_with("## F-2 Two\n\nStatus: folded metaplan#a\n\nText two.\n"),
        "{two}"
    );
    assert_eq!(parse(&two).findings[1].status, status);
    assert_eq!(parse(&two).findings[1].text, "Text two.");
}

#[test]
fn set_status_refuses_an_unknown_id_and_an_empty_reason() {
    let text = ledger_of(&[entry("F-1", "One", FindingStatus::Open, "x")]);
    assert!(set_status(&text, "F-9", &FindingStatus::Open).is_err());
    assert!(set_status(&text, "F-1", &FindingStatus::Dismissed(" \n\u{7} ".into())).is_err());
    assert!(set_status(&text, "F-1", &FindingStatus::Planned("".into())).is_err());
}

#[test]
fn an_unknown_or_missing_status_reads_open_with_a_problem() {
    let text = "# Findings\n\n## F-1 One\n\nStatus: maybe\n\nx\n\n## F-2 Two\n\nAbout: a\n\ny\n";
    let ledger = parse(text);
    assert_eq!(ledger.findings.len(), 2);
    assert!(ledger
        .findings
        .iter()
        .all(|f| f.status == FindingStatus::Open));
    let problems: Vec<(&str, usize)> = ledger.problems.iter().map(|p| (p.code, p.line)).collect();
    assert_eq!(problems, vec![(BAD_STATUS, 3), (BAD_STATUS, 9)]);
}

#[test]
fn bad_and_duplicate_headings_are_reported_and_skipped() {
    let fence = "`".repeat(3);
    let text = format!(
        "# Findings\n\n## Notes\n\n## F-0 Zero\n\n## F-1 One\n\nStatus: open\n\nx\n\n\
         ## F-1 Again\n\nStatus: open\n\ny\n\n{fence}\n## F-7 fenced\n{fence}\n"
    );
    let ledger = parse(&text);
    assert_eq!(ledger.findings.len(), 1);
    assert_eq!(ledger.findings[0].title, "One");
    let codes: Vec<&str> = ledger.problems.iter().map(|p| p.code).collect();
    assert_eq!(codes, vec![BAD_ENTRY, BAD_ENTRY, DUPLICATE_ID]);
}

#[test]
fn reasons_flatten_whitespace_controls_and_stop_at_200_chars() {
    assert_eq!(
        reason("  too\u{7} late\n now \t").as_deref(),
        Some("too late  now")
    );
    assert_eq!(reason(" \n\u{1b}\t "), None);
    // A line break stays a word break: the reason cannot open a ledger heading.
    assert_eq!(
        reason("not needed\n## F-2 x").as_deref(),
        Some("not needed ## F-2 x")
    );
    let long = "é".repeat(300);
    assert_eq!(reason(&long).unwrap().chars().count(), 200);
}

#[test]
fn titles_cut_on_a_char_boundary_and_paragraphs_never_open_blocks() {
    assert_eq!(title(&"日".repeat(100)), "日".repeat(80));
    assert_eq!(title("a\nb\r\nc"), "a b  c");
    assert_eq!(title("Fix C# ###"), "Fix C#");
    assert_eq!(title("###"), "Untitled finding");
    assert_eq!(title("Port the parser to C#"), "Port the parser to C#");
    assert_eq!(title("Port to C# #"), "Port to C#");
    assert_eq!(title("Fix it # #"), "Fix it");
    assert_eq!(title("a # # #"), "a");
    assert_eq!(paragraph("# H\nmore"), "\\# H more");
    assert_eq!(paragraph("```rust"), "\\```rust");
    assert_eq!(paragraph("~~~"), "\\~~~");
    assert_eq!(paragraph("plain"), "plain");
}

#[test]
fn a_fence_opening_text_cannot_hide_later_entries() {
    let fence = "`".repeat(3);
    let first = entry("F-1", "One", FindingStatus::Open, &fence);
    let second = entry("F-2", "Two", FindingStatus::Open, "after");
    let ledger = parse(&ledger_of(&[first, second]));
    assert_eq!(ledger.findings.len(), 2);
    assert_eq!(ledger.findings[0].text, fence);
}

fn project() -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();
    let plan = root.join("doc/plans/demo/IN_PROGRESS-PLAN-first.md");
    fs::create_dir_all(plan.parent().unwrap()).unwrap();
    fs::write(&plan, "# First\n\nImplements: section_01#one, section_02\n").unwrap();
    (temp, root)
}

const PLAN: &str = "doc/plans/demo/IN_PROGRESS-PLAN-first.md";

fn review(findings: &[&str], questions: &[&str]) -> String {
    let bullets = |items: &[&str], empty: &str| {
        if items.is_empty() {
            return format!("{empty}\n");
        }
        items
            .iter()
            .map(|t| format!("- **Build** (`build`): {t}\n"))
            .collect::<String>()
    };
    format!(
        "# Code Review: First\n\n## Open Questions\n\n{}\n## Open Findings\n\n{}",
        bullets(questions, "No open questions."),
        bullets(findings, "No open findings.")
    )
}

fn ledger_text(root: &Path) -> String {
    fs::read_to_string(root.join("doc/plans/demo/findings.md")).unwrap()
}

#[test]
fn import_reads_findings_and_questions_with_their_sources() {
    let (_temp, root) = project();
    let text = review(&["Rename the flag"], &["Which flag?"]);
    assert_eq!(import_review(&root, Path::new(PLAN), &text).unwrap(), 2);
    let ledger = parse(&ledger_text(&root));
    assert_eq!(ledger.problems, Vec::new());
    let found = &ledger.findings;
    assert_eq!(
        found[0].about,
        vec!["PLAN-first.md#stage:build", "section_01#one", "section_02"]
    );
    assert!(found[0]
        .source
        .as_deref()
        .unwrap()
        .starts_with("REVIEW-PLAN-first.md#open-findings (build, "));
    assert!(found[1]
        .source
        .as_deref()
        .unwrap()
        .starts_with("REVIEW-PLAN-first.md#open-questions (build, "));
    assert_eq!(found[1].text, "Which flag?");
    assert_eq!(import_review(&root, Path::new(PLAN), &text).unwrap(), 0);
}

#[test]
fn import_keeps_existing_entries_and_adds_changed_items_as_new_ones() {
    let (_temp, root) = project();
    let existing = ledger_of(&[entry(
        "F-4",
        "Hand written",
        FindingStatus::Planned("PLAN-z.md".into()),
        "kept",
    )]);
    fs::write(root.join("doc/plans/demo/findings.md"), &existing).unwrap();

    let plan = Path::new(PLAN);
    assert_eq!(
        import_review(&root, plan, &review(&["First wording"], &[])).unwrap(),
        1
    );
    let after_first = ledger_text(&root);
    assert!(after_first.starts_with(&existing), "ledger:\n{after_first}");
    assert!(after_first.contains("\n## F-5 First wording\n"));

    assert_eq!(
        import_review(&root, plan, &review(&["First wording"], &[])).unwrap(),
        0
    );
    assert_eq!(
        import_review(&root, plan, &review(&["First wording, revised"], &[])).unwrap(),
        1
    );
    let ledger = parse(&ledger_text(&root));
    let ids: Vec<&str> = ledger.findings.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, vec!["F-4", "F-5", "F-6"]);
    assert_eq!(
        ledger.findings[0].status,
        FindingStatus::Planned("PLAN-z.md".into())
    );
}

#[test]
fn import_text_cannot_open_a_heading() {
    let (_temp, root) = project();
    import_review(&root, Path::new(PLAN), &review(&["## F-9 injected"], &[])).unwrap();
    let text = ledger_text(&root);
    assert_eq!(
        text.lines().filter(|l| l.starts_with("## ")).count(),
        1,
        "{text}"
    );
    let parsed = parse(&text).findings;
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].text, "## F-9 injected");
}

#[test]
fn import_refuses_a_symlinked_ledger_file() {
    let (_temp, root) = project();
    let outside = TempDir::new().unwrap();
    let target = outside.path().join("findings.md");
    fs::write(&target, "keep\n").unwrap();
    std::os::unix::fs::symlink(&target, root.join("doc/plans/demo/findings.md")).unwrap();
    assert!(import_review(&root, Path::new(PLAN), &review(&["x"], &[])).is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep\n");
}

#[test]
fn import_refuses_a_symlinked_directory() {
    let (_temp, root) = project();
    let outside = TempDir::new().unwrap();
    fs::remove_dir_all(root.join("doc/plans/demo")).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join("doc/plans/demo")).unwrap();
    let result = import_review(&root, Path::new(PLAN), &review(&["x"], &[]));
    assert!(result.is_err());
    assert!(!outside.path().join("findings.md").exists());
}

#[test]
fn import_of_an_empty_review_writes_nothing() {
    let (_temp, root) = project();
    let text = review(&[], &[]);
    assert_eq!(import_review(&root, Path::new(PLAN), &text).unwrap(), 0);
    assert!(!root.join("doc/plans/demo/findings.md").exists());
}

#[test]
fn blank_lines_may_separate_metadata_lines() {
    let text =
        "# Findings\n\n## F-1 T\n\nAbout: section_01\n\nStatus: dismissed: dup\n\nThe text.\n";
    let ledger = parse(text);
    assert_eq!(ledger.problems, Vec::new());
    let finding = &ledger.findings[0];
    assert_eq!(finding.status, FindingStatus::Dismissed("dup".into()));
    assert_eq!(finding.text, "The text.");
    let changed = set_status(text, "F-1", &FindingStatus::Open).unwrap();
    assert_eq!(changed.matches("Status:").count(), 1);
    assert!(changed.contains("\nStatus: open\n"), "{changed}");
}

#[test]
fn text_that_looks_like_metadata_stays_text() {
    let status = FindingStatus::Open;
    for text in [
        "Status: dismissed: no",
        "Status: dismissed: sneaky",
        "About: elsewhere",
        "Source: a",
    ] {
        let parsed = parse(&ledger_of(&[entry("F-1", "T", status.clone(), text)]));
        assert_eq!(parsed.problems, Vec::new());
        assert_eq!(parsed.findings[0].text, text);
        assert_eq!(parsed.findings[0].status, status);
    }
    let late = "# Findings\n\n## F-1 T\n\nStatus: open\n\nProse.\nStatus: dismissed: late\n";
    let parsed = parse(late);
    assert_eq!(parsed.findings[0].status, FindingStatus::Open);
    assert!(parsed.findings[0].text.contains("Status: dismissed: late"));
}

#[test]
fn entry_text_round_trips_text_that_needs_an_escape() {
    let texts = [
        "plain text",
        "# not a heading",
        "```rust",
        "~~~",
        "Status: dismissed: sneaky",
        "About: elsewhere",
        "Source: a",
        "\\`--force\\` is ignored",
        "\\plain backslash",
        "\\\\ two backslashes",
        "\\# hash after a backslash",
        "ends with a backslash \\",
    ];
    for text in texts {
        let parsed = parse(&ledger_of(&[entry("F-1", "T", FindingStatus::Open, text)]));
        assert_eq!(parsed.problems, Vec::new(), "{text}");
        assert_eq!(parsed.findings[0].text, text);
        assert_eq!(parsed.findings[0].status, FindingStatus::Open);
    }
}

#[test]
fn a_title_ending_in_a_hash_reads_back_the_same() {
    let heading = title("Port the parser to C#");
    let parsed = parse(&ledger_of(&[entry(
        "F-1",
        &heading,
        FindingStatus::Open,
        "x",
    )]));
    assert_eq!(parsed.findings[0].title, "Port the parser to C#");
}
