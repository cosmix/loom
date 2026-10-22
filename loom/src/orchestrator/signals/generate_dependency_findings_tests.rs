//! The Dependency Findings section end to end: records and journals on disk,
//! through `generate_signal` and `generate_recovery_signal`, read back from
//! the written signal file. These TempDirs are not git repositories, so no
//! dependency has a range and no headline has a `git diff` part.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{generate_signal, DEPENDENCY_FINDINGS_HEADING, MAX_DEPENDENCY_FINDINGS_BYTES};
use crate::fs::memory::{append_entry, MemoryEntry, MemoryEntryType};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::signals::tests::{
    create_test_session, create_test_stage, create_test_worktree,
};
use crate::orchestrator::signals::{generate_recovery_signal, RecoverySignalContent};
use crate::verify::transitions::save_stage;

const COMMIT: &str = "89abcdef0123456789abcdef0123456789abcdef";
const HEADING_LINE: &str = "## Dependency Findings";

fn work_dir(temp: &TempDir) -> PathBuf {
    let work_dir = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    work_dir
}

fn save(work_dir: &Path, id: &str, configure: impl FnOnce(&mut Stage)) -> Stage {
    let mut stage = create_test_stage();
    stage.id = id.to_string();
    configure(&mut stage);
    save_stage(&stage, work_dir).unwrap();
    stage
}

fn save_dependency(work_dir: &Path, id: &str, stage_type: StageType) {
    save(work_dir, id, |stage| {
        stage.status = StageStatus::Completed;
        stage.completed_commit = Some(COMMIT.to_string());
        stage.stage_type = stage_type;
    });
}

fn save_recipient(work_dir: &Path, id: &str, deps: &[&str], files: &[&str]) -> Stage {
    save(work_dir, id, |stage| {
        stage.dependencies = deps.iter().map(|d| d.to_string()).collect();
        stage.files = files.iter().map(|f| f.to_string()).collect();
    })
}

/// Append to `stage_id`'s journal; returns the entry's id8.
fn append(work_dir: &Path, stage_id: &str, entry: MemoryEntry) -> String {
    append_entry(work_dir, stage_id, &entry).unwrap();
    entry.id[..8].to_string()
}

fn decision_on(content: &str, evidence: &str) -> MemoryEntry {
    MemoryEntry::with_context(
        MemoryEntryType::Decision,
        content.to_string(),
        "why".to_string(),
    )
    .with_evidence(vec![evidence.to_string()])
}

fn note_on(content: &str, evidence: &str) -> MemoryEntry {
    MemoryEntry::new(MemoryEntryType::Note, content.to_string())
        .with_evidence(vec![evidence.to_string()])
}

fn signal(work_dir: &Path, stage: &Stage, session_id: &str) -> String {
    let mut session = create_test_session();
    session.id = session_id.to_string();
    let worktree = create_test_worktree();
    let path = generate_signal(&session, stage, &worktree, &[], None, None, work_dir).unwrap();
    std::fs::read_to_string(path).unwrap()
}

fn recovery_signal(work_dir: &Path, stage: &Stage) -> String {
    let content = RecoverySignalContent::for_crash(
        "session-recovered".into(),
        stage.id.clone(),
        "session-crashed".into(),
        None,
        1,
    );
    let path = generate_recovery_signal(&content, stage, work_dir).unwrap();
    std::fs::read_to_string(path).unwrap()
}

/// The section's lines, heading through closing fence; exactly one heading.
fn section(signal: &str) -> Vec<&str> {
    let lines: Vec<&str> = signal.lines().collect();
    let headings: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i] == HEADING_LINE)
        .collect();
    assert_eq!(headings.len(), 1, "one heading expected:\n{signal}");
    let open = (headings[0]..lines.len())
        .find(|&i| lines[i].starts_with("```") && lines[i].ends_with("text"))
        .expect("an opening fence");
    let fence = lines[open].trim_end_matches("text");
    let close = (open + 1..lines.len())
        .find(|&i| lines[i] == fence)
        .expect("a closing fence");
    lines[headings[0]..=close].to_vec()
}

/// A Completed `a-dep` whose journal holds a decision on `src/b.rs` and an
/// unrelated note; returns their id8s.
fn accepted_dependency(work_dir: &Path) -> (String, String) {
    save_dependency(work_dir, "a-dep", StageType::Standard);
    let decision = append(
        work_dir,
        "a-dep",
        decision_on("hold the lock", "src/b.rs:3"),
    );
    let note = append(work_dir, "a-dep", note_on("tidy the docs", "docs/x.md"));
    (decision, note)
}

#[test]
fn a_successor_gets_the_relevant_decision_and_not_the_unrelated_note() {
    let temp = TempDir::new().unwrap();
    let wd = work_dir(&temp);
    let (decision, note) = accepted_dependency(&wd);
    let b = save_recipient(&wd, "b-next", &["a-dep"], &["src/b.rs"]);

    let text = section(&signal(&wd, &b, "session-b")).join("\n");
    let headline = "a-dep accepted at 89abcdef: 1 of 2 entries | loom memory list --stage a-dep";
    assert!(text.contains(headline), "{text}");
    assert!(text.len() <= MAX_DEPENDENCY_FINDINGS_BYTES, "{text}");
    assert!(text.contains(&decision), "{text}");
    assert!(!text.contains(&note), "{text}");
}

#[test]
fn an_unrelated_successor_gets_only_the_headline() {
    let temp = TempDir::new().unwrap();
    let wd = work_dir(&temp);
    accepted_dependency(&wd);
    let c = save_recipient(&wd, "c-next", &["a-dep"], &["web/c.ts"]);

    let lines = section(&signal(&wd, &c, "session-c")).join("\n");
    assert!(
        lines.contains("a-dep accepted at 89abcdef: 0 of 2 entries"),
        "{lines}"
    );
    assert!(!lines.lines().any(|l| l.starts_with("- [")), "{lines}");
}

#[test]
fn knowledge_distill_and_knowledge_dependencies_get_no_section() {
    let temp = TempDir::new().unwrap();
    let wd = work_dir(&temp);
    accepted_dependency(&wd);
    let distill = save(&wd, "distill", |stage| {
        stage.stage_type = StageType::KnowledgeDistill;
        stage.dependencies = vec!["a-dep".to_string()];
    });
    assert!(!signal(&wd, &distill, "session-distill").contains(HEADING_LINE));

    save_dependency(&wd, "k-dep", StageType::Knowledge);
    append(&wd, "k-dep", decision_on("k rule", "src/b.rs"));
    let after_knowledge = save_recipient(&wd, "after-k", &["k-dep"], &["src/b.rs"]);
    let text = signal(&wd, &after_knowledge, "session-after-k");
    assert!(!text.contains(HEADING_LINE), "{text}");
}

#[test]
fn a_journal_body_with_structure_renders_as_one_line() {
    let temp = TempDir::new().unwrap();
    let wd = work_dir(&temp);
    save_dependency(&wd, "a-dep", StageType::Standard);
    let body = "keep the order\n---\n### not a heading\n```\nend of body";
    let id8 = append(&wd, "a-dep", note_on(body, "src/b.rs"));
    let b = save_recipient(&wd, "b-next", &["a-dep"], &["src/b.rs"]);

    let text = signal(&wd, &b, "session-hostile");
    let lines = section(&text);
    let holding: Vec<&&str> = lines.iter().filter(|l| l.contains(&id8)).collect();
    assert_eq!(holding.len(), 1, "{lines:#?}");
    let flattened = "keep the order --- ### not a heading ``` end of body";
    assert!(holding[0].contains(flattened), "{lines:#?}");
}

#[test]
fn the_section_heading_is_an_expected_one_and_its_only_heading() {
    assert!(super::super::parse::EXPECTED_SECTIONS.contains(&DEPENDENCY_FINDINGS_HEADING));
    let temp = TempDir::new().unwrap();
    let wd = work_dir(&temp);
    accepted_dependency(&wd);
    let b = save_recipient(&wd, "b-next", &["a-dep"], &["src/b.rs"]);

    let text = signal(&wd, &b, "session-headings");
    let lines = section(&text);
    let headings: Vec<&&str> = lines.iter().filter(|l| l.starts_with("## ")).collect();
    assert_eq!(headings, vec![&HEADING_LINE], "{lines:#?}");
}

#[test]
fn a_relay_after_the_first_signal_reaches_the_next_signal() {
    let temp = TempDir::new().unwrap();
    let wd = work_dir(&temp);
    accepted_dependency(&wd);
    let b = save_recipient(&wd, "b-next", &["a-dep"], &["src/b.rs"]);
    let first = signal(&wd, &b, "session-first");

    let relayed = append(&wd, "a-dep", decision_on("relayed late", "src/b.rs"));
    let second = signal(&wd, &b, "session-second");
    assert!(!section(&first).join("\n").contains(&relayed), "{first}");
    assert!(section(&second).join("\n").contains(&relayed), "{second}");
}

#[test]
fn the_recovery_signal_carries_the_same_section_once() {
    let temp = TempDir::new().unwrap();
    let wd = work_dir(&temp);
    let (decision, _) = accepted_dependency(&wd);
    let b = save_recipient(&wd, "b-next", &["a-dep"], &["src/b.rs"]);

    let fresh = signal(&wd, &b, "session-fresh");
    let recovered = recovery_signal(&wd, &b);
    assert_eq!(section(&fresh), section(&recovered));
    assert!(section(&recovered).join("\n").contains(&decision));
}
