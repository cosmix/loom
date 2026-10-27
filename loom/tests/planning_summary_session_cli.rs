//! Binary-level contracts for `$LOOM_PLAN_SESSION` on `loom plan summary`: the session a brief
//! records, who may release a claim, `--force` from a session against from a terminal, and
//! summary ids that have no source in the project.

#[path = "planning_summary_support/mod.rs"]
mod support;

use std::fs;

use serde_json::Value;
use support::*;

const SESSION: &str = "LOOM_PLAN_SESSION";

#[test]
fn session_cli_brief_records_the_session_and_release_is_scoped_to_it() {
    let (_t, root, home) = git_project(&[(S1, &section(1, "Alpha.", "Beta."))]);
    let claim = paths(&root, &home).claims.join("product__section_01.json");
    let out = loom_summary(&root, &home, &["brief", ID], &[(SESSION, "s1")]);
    assert!(out.status.success(), "brief failed: {}", stderr(&out));
    let text = fs::read_to_string(&claim).expect("the brief wrote no claim");
    let stored: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}"));
    assert_eq!(
        stored["session"], "s1",
        "the claim does not name the session"
    );

    let out = loom_summary(&root, &home, &["brief", ID], &[(SESSION, "s2")]);
    assert_eq!(out.status.code(), Some(1), "s2 briefed: {}", stderr(&out));
    assert!(stderr(&out).contains("claimed by s1"), "{}", stderr(&out));

    let out = loom_summary(&root, &home, &["release", ID], &[(SESSION, "s2")]);
    assert_eq!(out.status.code(), Some(1), "s2 released: {}", stderr(&out));
    assert!(stderr(&out).contains("claimed by s1"), "{}", stderr(&out));
    assert!(claim.exists(), "s2 deleted the claim of s1");

    let out = loom_summary(&root, &home, &["release", ID], &[(SESSION, "s1")]);
    assert!(out.status.success(), "s1 release: {}", stderr(&out));
    assert!(!claim.exists(), "the claimant's release kept its claim");
}

#[test]
fn session_cli_force_overrides_a_claim_only_from_a_terminal() {
    let (_t, root, home) = git_project(&[(PLAN_X, "# Plan: X\n\nDoes x.\n")]);
    let summary = root.join(summary_rel("plans/x"));
    let claim = plant_claim(&root, &home);
    let args = ["seal", "plans/x", "--force"];

    let out = loom_summary(&root, &home, &args, &[(SESSION, "s2")]);
    assert_eq!(out.status.code(), Some(1), "s2 forced: {}", stderr(&out));
    assert!(stderr(&out).contains("claimed by s1"), "{}", stderr(&out));
    assert!(
        !stderr(&out).contains("--force"),
        "a session is told to force: {}",
        stderr(&out)
    );
    assert!(claim.exists(), "s2 deleted the claim of s1");
    assert!(!summary.exists(), "s2 wrote the summary");

    let out = loom_summary(&root, &home, &args, &[]);
    assert!(out.status.success(), "terminal --force: {}", stderr(&out));
    assert!(summary.exists(), "the terminal's --force wrote nothing");
    assert!(!claim.exists(), "the forced seal left the claim");
}

#[test]
fn session_cli_ids_without_a_source_name_the_id() {
    let (_t, root, home) = git_project(&[(PLAN_X, "# Plan: X\n\nDoes x.\n")]);
    let claim = paths(&root, &home).claims.join("plans__missing.json");

    let out = loom_summary(&root, &home, &["brief", "plans/missing"], &[]);
    assert_eq!(out.status.code(), Some(2), "brief: {}", stderr(&out));
    let want = "no such summary in this project: plans/missing";
    assert!(stderr(&out).contains(want), "{}", stderr(&out));
    assert!(!claim.exists(), "an unknown summary was claimed");

    let out = loom_summary(&root, &home, &["seal", "plans/missing"], &[]);
    assert_eq!(out.status.code(), Some(1), "seal: {}", stderr(&out));
    assert!(stderr(&out).contains(want), "{}", stderr(&out));
    assert!(!root.join(summary_rel("plans/missing")).exists());
}
