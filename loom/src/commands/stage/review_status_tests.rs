//! `freshness_report`'s baseline selection: the delta it prints must anchor
//! on the latest well-formed round, matching the review gate's own selector
//! (DESIGN D12), even when the true latest round is malformed.

use super::*;

fn fingerprint(value: &str, files: &[(&str, &str)]) -> ChangeFingerprint {
    ChangeFingerprint {
        value: value.to_string(),
        behaviour: files
            .iter()
            .map(|(path, hash)| (path.to_string(), hash.to_string()))
            .collect(),
        base: "main".to_string(),
        files: files
            .iter()
            .map(|(path, hash)| (path.to_string(), hash.to_string()))
            .collect(),
    }
}

fn round(number: u32, at: &ChangeFingerprint, malformed: Option<&str>) -> ReviewRound {
    ReviewRound {
        version: 1,
        round: number,
        agent_id: format!("agent-{number}"),
        harvested_at: chrono::Utc::now(),
        fingerprint: at.value.clone(),
        base: at.base.clone(),
        behaviour: at.behaviour.clone(),
        files: at.files.clone(),
        malformed: malformed.map(str::to_string),
        findings: Vec::new(),
        resolved: Vec::new(),
        unresolved: Vec::new(),
        suggestion_memory_ids: Vec::new(),
    }
}

#[test]
fn freshness_reports_exempt_paths_using_the_gate_predicate() {
    let before = crate::verify::review::fingerprint::fingerprint_from(
        "base",
        &[
            ("a.rs".into(), Some(b"fn a() {}\n".to_vec())),
            ("doc/x.md".into(), Some(b"old".to_vec())),
        ],
    );
    let after = crate::verify::review::fingerprint::fingerprint_from(
        "base",
        &[
            ("a.rs".into(), Some(b"fn a() {}\n".to_vec())),
            ("doc/x.md".into(), Some(b"new".to_vec())),
        ],
    );
    let report = freshness_report(&[round(1, &before, None)], &after).join("\n");
    assert!(report.contains("current fingerprint: yes"), "{report}");
    assert!(report.contains("prose changes alone"), "{report}");
    assert!(
        !report.contains("a.rs") && report.contains("doc/x.md"),
        "{report}"
    );
    let mut changed_base = after;
    changed_base.base = "other".into();
    let report = freshness_report(&[round(1, &before, None)], &changed_base).join("\n");
    assert!(report.contains("current fingerprint: no"), "{report}");
    assert!(!report.contains("changes alone"), "{report}");
}

#[test]
fn freshness_requires_review_after_source_comment_changes() {
    let before = crate::verify::review::fingerprint::fingerprint_from(
        "base",
        &[("a.rs".into(), Some(b"fn a() {}\n".to_vec()))],
    );
    let after = crate::verify::review::fingerprint::fingerprint_from(
        "base",
        &[("a.rs".into(), Some(b"// note\nfn a() {}\n".to_vec()))],
    );

    let report = freshness_report(&[round(1, &before, None)], &after).join("\n");

    assert!(report.contains("current fingerprint: no"), "{report}");
    assert!(!report.contains("changes alone"), "{report}");
    assert!(report.contains("a.rs"), "{report}");
}

#[test]
fn delta_is_measured_from_the_last_well_formed_round_when_the_latest_round_is_malformed() {
    let f1 = fingerprint("sha256:f1", &[("a.rs", "h1")]);
    let f2 = fingerprint("sha256:f2", &[("a.rs", "h1"), ("b.rs", "h2")]);
    let well_formed = round(1, &f1, None);
    let malformed = round(2, &f2, Some("no loom-review block"));

    let report = freshness_report(&[well_formed, malformed], &f2).join("\n");

    assert!(report.contains("malformed"), "report: {report}");
    assert!(report.contains("measured from round 1"), "report: {report}");
    assert!(report.contains("b.rs"), "report: {report}");
}

#[test]
fn delta_covers_every_file_when_no_well_formed_round_is_recorded() {
    let only = fingerprint("sha256:f1", &[("a.rs", "h1")]);
    let malformed = round(1, &only, Some("no loom-review block"));

    let report = freshness_report(&[malformed], &only).join("\n");

    assert!(
        report.contains("no well-formed round is recorded"),
        "report: {report}"
    );
    assert!(report.contains("a.rs"), "report: {report}");
}

#[test]
fn delta_uses_the_latest_round_directly_when_it_is_well_formed() {
    let f1 = fingerprint("sha256:f1", &[("a.rs", "h1")]);
    let f2 = fingerprint("sha256:f2", &[("a.rs", "h1"), ("b.rs", "h2")]);
    let round1 = round(1, &f1, None);

    let report = freshness_report(&[round1], &f2).join("\n");

    assert!(!report.contains("measured from round"), "report: {report}");
    assert!(report.contains("b.rs"), "report: {report}");
}
