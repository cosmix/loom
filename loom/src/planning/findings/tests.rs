use super::*;

pub(super) fn entry(id: &str, title: &str, status: FindingStatus, text: &str) -> String {
    let about = vec!["section_01#one".to_owned(), "PLAN-a.md#stage:x".to_owned()];
    entry_text(id, title, &about, Some("review 2026-11-05"), &status, text)
}

pub(super) fn ledger_of(entries: &[String]) -> String {
    format!("# Findings\n\n{}", entries.join("\n"))
}

#[test]
fn entry_text_round_trips_through_parse() {
    let status = FindingStatus::Addressed("section_01#one".into());
    let text = ledger_of(&[entry("F-1", "Rebuild budget", status.clone(), "Body text.")]);
    let ledger = parse(&text);
    assert_eq!(ledger.problems, Vec::new());
    assert_eq!(
        ledger.findings,
        vec![Finding {
            id: "F-1".into(),
            title: "Rebuild budget".into(),
            about: vec!["section_01#one".into(), "PLAN-a.md#stage:x".into()],
            evidence: None,
            severity: None,
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
        FindingStatus::Addressed("section_01#one, PLAN-a.md".into()),
        FindingStatus::Unaddressed("the plan already ran".into()),
        FindingStatus::Dismissed("not worth it".into()),
    ] {
        let parsed = parse(&ledger_of(&[entry("F-1", "T", status.clone(), "x")]));
        assert_eq!(parsed.findings[0].status, status);
        assert_eq!(parsed.problems, Vec::new());
    }
}

#[test]
fn an_addressed_status_reads_with_a_space_or_a_colon() {
    let addressed = FindingStatus::Addressed("section_04, vision".into());
    for line in [
        "addressed section_04, vision",
        "addressed: section_04, vision",
    ] {
        let text = format!("# Findings\n\n## F-1 One\n\nStatus: {line}\n\nx\n");
        let ledger = parse(&text);
        assert_eq!(ledger.findings[0].status, addressed, "{line}");
        assert_eq!(ledger.problems, Vec::new(), "{line}");
    }
    for line in ["addressed", "addressed:", "addressedsection_04"] {
        let text = format!("# Findings\n\n## F-1 One\n\nStatus: {line}\n\nx\n");
        assert_eq!(parse(&text).problems[0].code, BAD_STATUS, "{line}");
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
    let status = FindingStatus::Addressed("section_01".into());
    let one = set_status(text, "F-1", &status).unwrap();
    assert_eq!(
        one,
        "# Findings\n\n## F-1 One\n\nAbout: section_01\nStatus: addressed section_01\n\nText.\n\n\
         ## F-2 Two\n\nText two.\n"
    );
    let two = set_status(text, "F-2", &status).unwrap();
    assert!(
        two.ends_with("## F-2 Two\n\nStatus: addressed section_01\n\nText two.\n"),
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
    assert!(set_status(&text, "F-1", &FindingStatus::Addressed("".into())).is_err());
    assert!(set_status(&text, "F-1", &FindingStatus::Unaddressed(" ".into())).is_err());
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
        Some("too late now")
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
    assert_eq!(title("a\nb\r\nc"), "a b c");
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

#[test]
fn evidence_and_severity_are_read_and_a_bad_severity_is_reported() {
    let text =
        "# Findings\n\n## F-1 One\n\nAbout: section_01\nEvidence: The budget counts plans.\n\
                Severity: high\nStatus: open\n\nx\n\n## F-2 Two\n\nSeverity: urgent\n\
                Status: open\n\ny\n";
    let ledger = parse(text);
    let first = &ledger.findings[0];
    assert_eq!(first.evidence.as_deref(), Some("The budget counts plans."));
    assert_eq!(first.severity, Some(Severity::High));
    assert_eq!(first.text, "x");
    assert_eq!(ledger.findings[1].severity, None);
    let problems: Vec<(&str, usize)> = ledger.problems.iter().map(|p| (p.code, p.line)).collect();
    assert_eq!(problems, vec![(BAD_SEVERITY, 12)]);
    assert_eq!(paragraph("Evidence: no"), "\\Evidence: no");
}
