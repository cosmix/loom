//! Scenario builders and multi-step checks shared by the summary-store contracts.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use loom::planning::summary::claims::Claim;
use loom::planning::summary::config::SummaryModels;
use loom::planning::summary::format::{parse, SummaryFile};
use loom::planning::watch::{Batch, Watcher};
use tempfile::TempDir;

use super::*;

pub const ID: &str = "demo/product/section_01";
pub const PLAN_Y: &str = "doc/plans/demo/PLAN-y.md";
pub const DRAFT_X: &str = "# X\n\nX is planned. [d](#s-a1edcae5)\n\n\
                           X does x. [§](../../../plans/demo/PLAN-x.md#plan-x)\n";
pub const CLAIM_S1: &str =
    r#"{"summary":"plans/x","session":"s1","pid":1,"created_at":"2026-10-27T00:00:00Z"}"#;
pub const UUID: &str = "0b8f6c2e-4d1a-4e8b-9c3f-2a7d5e6f1b90";

/// [`project`] over owned texts.
pub fn project_of(files: &[(&str, String)]) -> (TempDir, PathBuf, PathBuf) {
    let refs: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    project(&refs)
}

/// A vision listing `numbers`, each section written with `## A` and `## B`.
pub fn sections_project(numbers: &[u32]) -> (TempDir, PathBuf, PathBuf) {
    let mut files = vec![(VISION, vision(numbers))];
    let paths = [S1, S2];
    for n in numbers {
        files.push((paths[*n as usize - 1], section(*n, "Alpha.", "Beta.")));
    }
    project_of(&files)
}

/// A feature metaplan naming `doc/plans/demo/PLAN-x.md`.
pub fn plan_x() -> (TempDir, PathBuf, PathBuf) {
    project(&[
        (META, &metaplan(&["PLAN-x.md"])),
        (PLAN_X, "# Plan: X\n\nDoes x.\n"),
    ])
}

/// [`plan_x`] in a repository the CLI can locate.
pub fn git_plan_x() -> (TempDir, PathBuf, PathBuf) {
    git_project(&[
        (META, &metaplan(&["PLAN-x.md"])),
        (PLAN_X, "# Plan: X\n\nDoes x.\n"),
    ])
}

/// [`project`] over `files` with `plans/x` sealed citing the plan's title section.
pub fn plan_fixture(files: &[(&str, &str)]) -> (TempDir, PathBuf, PathBuf) {
    let (t, root, home) = project(files);
    sealed(&root, &home, "plans/x", DRAFT_X);
    (t, root, home)
}

/// Parses summary `id`'s file text, panicking with every format error.
pub fn parsed(id: &str, text: &str) -> SummaryFile {
    parse(Path::new(&summary_rel(id)), text).unwrap_or_else(|errors| {
        let lines: Vec<String> = errors
            .iter()
            .map(|e| format!("{}: {}", e.line, e.message))
            .collect();
        panic!("parse {id} failed: {lines:?}\n{text}")
    })
}

/// A document summary of section 01 whose body holds the single sentence line `line`.
pub fn summary_with(line: &str) -> SummaryFile {
    let zero = format!("sha256:{}", "0".repeat(64));
    let a = format!("  \"doc/product/demo/section_01.md#a\": \"{zero}\"\n");
    let b = format!("  \"doc/product/demo/section_01.md#b\": \"{zero}\"\n");
    let head = format!("kind: document\nof: doc/product/demo/section_01.md\nfile: \"{zero}\"\n");
    parsed(
        ID,
        &format!("---\n{head}cites:\n{a}{b}---\n\n# S1\n\n{line}\n"),
    )
}

/// A draft of `n` distinct valid sentences of section 01.
pub fn many(n: usize) -> String {
    let texts: Vec<String> = (0..n).map(|i| format!("Fact {i} holds.")).collect();
    let lines: Vec<(&str, &str)> = texts.iter().map(|t| (t.as_str(), "a")).collect();
    doc_draft(1, &lines)
}

/// A product overview citing section 01's `Alpha holds.` and `n - 1` vision lines.
pub fn overview_of(n: usize) -> String {
    let member = member_link(ID, "Alpha holds.");
    let mut body = format!("# Product\n\nAlpha is kept. [§]({member})\n\n");
    for i in 1..n {
        body.push_str(&format!("Vision point {i}. [§]({VISION_LINK})\n\n"));
    }
    body
}

/// A product overview citing `(member, sentence text)` pairs and the vision.
pub fn product_overview(members: &[(&str, &str)]) -> String {
    let mut body = "# Product\n\n".to_string();
    for (i, (member, text)) in members.iter().enumerate() {
        let link = member_link(member, text);
        body.push_str(&format!("Member {i} is kept. [§]({link})\n\n"));
    }
    body.push_str(&format!("The vision lists them. [§]({VISION_LINK})\n"));
    body
}

/// Restores section 01 and reseals its one-sentence summary citing `## A`.
pub fn reseal(root: &Path, home: &Path) {
    put(root, S1, section(1, "Alpha.", "Beta."));
    sealed(root, home, ID, &doc_draft(1, &[("Alpha holds.", "a")]));
}

/// Hand edits of the sealed section 01 summary: a claim, conflict markers, a broken
/// edit and a valid one.
pub fn assert_hand_edits(root: &Path) {
    let rel = summary_rel(ID);
    let sealed_text = read(root, &rel);
    let claim = Claim {
        summary: ID.to_string(),
        session: Some("s1".to_string()),
        pid: 1,
        created_at: "2026-10-27T00:00:00Z".to_string(),
    };
    assert_eq!(state(&eval(root, &[claim]), ID), "rebuilding");
    let markers = "<<<<<<< ours\n=======\n>>>>>>> theirs\n";
    put(root, &rel, format!("{sealed_text}{markers}"));
    assert_eq!(state(&eval(root, &[]), ID), "missing", "conflict markers");
    let list = format!("\n- list [§]({})\n", doc_link(1, "a"));
    put(root, &rel, format!("{sealed_text}{list}"));
    assert_eq!(
        state(&eval(root, &[]), ID),
        "missing",
        "a hand-added list line"
    );
    put(
        root,
        &rel,
        sealed_text.replace("Alpha holds.", "Alpha still holds."),
    );
    assert_eq!(
        state(&eval(root, &[]), ID),
        "current",
        "a valid hand edit is kept"
    );
}

/// A plan with stages `alpha` and `beta` described by `alpha` and `beta`.
pub fn staged_plan(alpha: &str, beta: &str) -> String {
    let stage = |id: &str, name: &str, text: &str| {
        format!("    - id: {id}\n      name: {name}\n      description: {text}\n")
    };
    format!(
        "# Plan: X\n\nProse.\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 1\n  stages:\n\
         {}{}```\n\n<!-- END loom METADATA -->\n",
        stage("alpha", "Alpha", alpha),
        stage("beta", "Beta", beta)
    )
}

/// Polls `watcher` until it delivers a batch.
pub fn next_batch(watcher: &mut Watcher) -> Batch {
    for _ in 0..10 {
        if let Some(batch) = watcher.poll().expect("poll") {
            return batch;
        }
    }
    panic!("the watcher delivered no batch");
}

/// Writes the s1 claim of `plans/x` and a valid `plans/x` draft.
pub fn plant_claim(root: &Path, home: &Path) -> PathBuf {
    let claims = paths(root, home).claims;
    fs::create_dir_all(&claims).unwrap();
    let claim = claims.join("plans__x.json");
    fs::write(&claim, CLAIM_S1).unwrap();
    write_draft(root, home, "plans/x", DRAFT_X);
    claim
}

/// Runs `f` with `LOOM_HOME` set to `home`, restoring the previous value.
pub fn with_loom_home<T>(home: &Path, f: impl FnOnce() -> T) -> T {
    let old = std::env::var_os("LOOM_HOME");
    std::env::set_var("LOOM_HOME", home);
    let out = f();
    match old {
        Some(value) => std::env::set_var("LOOM_HOME", value),
        None => std::env::remove_var("LOOM_HOME"),
    }
    out
}

/// `[document model, document effort, overview model, overview effort]`.
pub fn models(m: &SummaryModels) -> [String; 4] {
    let fields = [
        &m.document_model,
        &m.document_effort,
        &m.overview_model,
        &m.overview_effort,
    ];
    fields.map(|f| f.clone())
}

pub fn chmod(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

/// Line indexes of `text` whose trimmed start is `mark`.
pub fn marked(text: &str, mark: &str) -> Vec<usize> {
    let lines = text.lines().enumerate();
    let hits = lines.filter(|(_, l)| l.trim_start().starts_with(mark));
    hits.map(|(i, _)| i).collect()
}

/// Asserts `value` holds every key of `keys`.
pub fn assert_keys(value: &serde_json::Value, keys: &[&str]) {
    for key in keys {
        assert!(value.get(key).is_some(), "lacks {key}: {value}");
    }
}
