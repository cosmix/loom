//! Contracts for the slug stage parser and the sidecar and parent-spawn
//! requested-model fallbacks beyond the frozen attribution contracts.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use serde_json::json;

use super::{next_line, MAX_LINE_BYTES};
use crate::commands::usage::discovery::{stage_from_project_slug, DiscoveredFile};
use crate::commands::usage::transcript::{Scope, Transcript};
use crate::commands::usage::{forward_join, parse_all, time_range};
use crate::provider_data::resolve::project_slug;

const REPO_SLUG: &str = "-home-u-src-app";

#[test]
fn slug_names_the_stage_with_and_without_the_repo_slug() {
    for repo in [Some(REPO_SLUG), None] {
        let slug = "-home-u-src-app--worktrees-stage-b";
        assert_eq!(stage_from_project_slug(slug, repo), Some("stage-b"));
        let slug = "-home-u-src-app--worktrees-Stage_B";
        assert_eq!(stage_from_project_slug(slug, repo), Some("Stage_B"));
    }
}

#[test]
fn slug_with_a_nested_delimiter_is_ambiguous_without_the_repo_slug() {
    let slug = "-home-u-src-app--worktrees-foo--worktrees-bar";
    assert_eq!(
        stage_from_project_slug(slug, Some(REPO_SLUG)),
        Some("foo--worktrees-bar")
    );
    assert_eq!(stage_from_project_slug(slug, None), None);
}

#[test]
fn slug_of_another_project_or_no_worktree_names_no_stage() {
    let other = "-home-u-src-other--worktrees-stage-b";
    assert_eq!(stage_from_project_slug(other, Some(REPO_SLUG)), None);
    assert_eq!(stage_from_project_slug(REPO_SLUG, Some(REPO_SLUG)), None);
    assert_eq!(stage_from_project_slug(REPO_SLUG, None), None);
}

#[test]
fn slug_with_an_empty_or_invalid_remainder_names_no_stage() {
    for repo in [Some("-x"), None] {
        assert_eq!(stage_from_project_slug("-x--worktrees-", repo), None);
        assert_eq!(stage_from_project_slug("-x--worktrees-a/b", repo), None);
        assert_eq!(stage_from_project_slug("-x--worktrees-a b", repo), None);
    }
}

fn range() -> time_range::TimeRange {
    time_range::TimeRange {
        since: Utc.with_ymd_and_hms(2026, 8, 29, 0, 0, 0).single().unwrap(),
        until: None,
    }
}

fn write_transcript(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let row = json!({
        "type": "assistant",
        "timestamp": "2026-08-30T00:01:00Z",
        "message": {
            "id": "m1",
            "model": "claude-sonnet-5",
            "content": [{"type": "text", "text": "done"}],
            "usage": {"input_tokens": 1}
        }
    });
    fs::write(path, format!("{row}\n")).unwrap();
}

fn parse_one(file: DiscoveredFile, work_dir: Option<&Path>) -> Transcript {
    let mut parsed = parse_all(
        &[file],
        &range(),
        work_dir,
        &forward_join::ForwardJoin::load(None),
    );
    assert_eq!(parsed.len(), 1, "the transcript must parse");
    parsed.remove(0)
}

fn main_file(project: &Path, slug: &str) -> DiscoveredFile {
    let path = project.join("session-1.jsonl");
    write_transcript(&path);
    DiscoveredFile {
        path,
        project_slug: slug.to_owned(),
        scope: Scope::Main,
        session_id: "session-1".to_owned(),
        agent_id: None,
    }
}

/// `<project>/session-1/subagents/agent-1.jsonl` plus its sidecar.
fn subagent_file(project: &Path, sidecar: serde_json::Value) -> DiscoveredFile {
    let path = project.join("session-1/subagents/agent-1.jsonl");
    write_transcript(&path);
    fs::write(
        path.with_file_name("agent-1.meta.json"),
        sidecar.to_string(),
    )
    .unwrap();
    DiscoveredFile {
        path,
        project_slug: "project".to_owned(),
        scope: Scope::Subagent,
        session_id: "session-1".to_owned(),
        agent_id: Some("agent-1".to_owned()),
    }
}

fn spawn_row(input: serde_json::Value) -> String {
    let row = json!({
        "type": "assistant",
        "message": {"content": [
            {"type": "text", "text": "tool_use in prose"},
            {"type": "tool_use", "id": "toolu_1", "name": "Agent", "input": input}
        ]}
    });
    format!("{row}\n")
}

#[test]
fn repo_slug_decides_which_worktree_slugs_name_a_stage() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join("app/.loom/work");
    fs::create_dir_all(work_dir.join("stages")).unwrap();
    fs::write(work_dir.join("stages/foo--worktrees-bar.md"), "").unwrap();
    let repo = project_slug(&fs::canonicalize(temp.path().join("app")).unwrap());

    let nested = main_file(
        temp.path(),
        &format!("{repo}--worktrees-foo--worktrees-bar"),
    );
    let transcript = parse_one(nested, Some(&work_dir));
    assert_eq!(transcript.stage_id.as_deref(), Some("foo--worktrees-bar"));

    let other = main_file(temp.path(), "-home-u-src-other--worktrees-x");
    assert_eq!(parse_one(other, Some(&work_dir)).stage_id, None);
}

#[test]
fn slug_remainder_must_be_a_recorded_stage() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join("app/.loom/work");
    fs::create_dir_all(work_dir.join("stages")).unwrap();
    fs::write(work_dir.join("stages/01-foo.md"), "").unwrap();
    let repo = project_slug(&fs::canonicalize(temp.path().join("app")).unwrap());

    // A session started in `.worktrees/foo/loom` is a subdirectory of stage `foo`.
    let subdirectory = main_file(temp.path(), &format!("{repo}--worktrees-foo-loom"));
    assert_eq!(parse_one(subdirectory, Some(&work_dir)).stage_id, None);

    let known = main_file(temp.path(), &format!("{repo}--worktrees-foo"));
    assert_eq!(
        parse_one(known, Some(&work_dir)).stage_id.as_deref(),
        Some("foo")
    );
}

#[test]
fn symlinked_work_dir_resolves_to_the_main_repository() {
    let temp = tempfile::tempdir().unwrap();
    let app = fs::canonicalize(temp.path()).unwrap().join("app");
    fs::create_dir_all(app.join(".loom/work/stages")).unwrap();
    fs::write(app.join(".loom/work/stages/stage-b.md"), "").unwrap();
    let worktree_loom = app.join(".worktrees/s/.loom");
    fs::create_dir_all(&worktree_loom).unwrap();
    std::os::unix::fs::symlink(app.join(".loom/work"), worktree_loom.join("work")).unwrap();
    let slug = format!("{}--worktrees-stage-b", project_slug(&app));

    let file = main_file(temp.path(), &slug);
    let transcript = parse_one(file, Some(&worktree_loom.join("work")));

    assert_eq!(transcript.stage_id.as_deref(), Some("stage-b"));
}

#[test]
fn sidecar_model_is_the_requested_model() {
    let temp = tempfile::tempdir().unwrap();
    let file = subagent_file(temp.path(), json!({"agentType": "a", "model": "opus"}));
    assert_eq!(
        parse_one(file, None).requested_model.as_deref(),
        Some("opus")
    );
}

#[test]
fn sidecar_model_outranks_the_parent_spawn() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("session-1.jsonl"),
        spawn_row(json!({"model": "haiku"})),
    )
    .unwrap();
    let sidecar = json!({"toolUseId": "toolu_1", "model": "opus"});
    let file = subagent_file(temp.path(), sidecar);
    assert_eq!(
        parse_one(file, None).requested_model.as_deref(),
        Some("opus")
    );
}

#[test]
fn spawn_without_a_model_is_the_default_model() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("session-1.jsonl");
    fs::write(&parent, spawn_row(json!({"subagent_type": "a"}))).unwrap();
    let file = subagent_file(temp.path(), json!({"toolUseId": "toolu_1"}));
    assert_eq!(
        parse_one(file, None).requested_model.as_deref(),
        Some("default")
    );
}

#[test]
fn tool_use_id_missing_from_the_parent_leaves_the_model_unknown() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("session-1.jsonl"),
        spawn_row(json!({"model": "haiku"})),
    )
    .unwrap();
    let file = subagent_file(temp.path(), json!({"toolUseId": "toolu_other"}));
    assert_eq!(parse_one(file, None).requested_model, None);
    let file = subagent_file(temp.path(), json!({"agentType": "a"}));
    assert_eq!(parse_one(file, None).requested_model, None);
}

#[test]
fn symlinked_parent_transcript_is_not_followed() {
    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real-parent.jsonl");
    fs::write(&real, spawn_row(json!({"model": "haiku"}))).unwrap();
    std::os::unix::fs::symlink(&real, temp.path().join("session-1.jsonl")).unwrap();
    let file = subagent_file(temp.path(), json!({"toolUseId": "toolu_1"}));
    assert_eq!(parse_one(file, None).requested_model, None);
}

#[test]
fn ledger_rows_outrank_the_slug_and_the_sidecar() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".loom/work");
    let stage_dir = work_dir.join("subagents/stage-a");
    fs::create_dir_all(&stage_dir).unwrap();
    let start = json!({
        "ts": "2026-08-30T00:00:01Z", "agent_id": "agent-1", "agent_type": "loom-x",
        "parent_session_id": "session-1", "stage_id": "stage-a", "loom_session_id": "loom-1"
    });
    let spawn = json!({
        "ts": "2026-08-30T00:00:00Z", "session_id": "loom-1",
        "agent_type": "loom-x", "model": "sonnet"
    });
    fs::write(stage_dir.join("starts.jsonl"), format!("{start}\n")).unwrap();
    fs::write(stage_dir.join("spawns.jsonl"), format!("{spawn}\n")).unwrap();
    let mut file = subagent_file(temp.path(), json!({"model": "opus"}));
    file.project_slug = "-x--worktrees-other".to_owned();

    let transcript = parse_one(file, Some(&work_dir));

    assert_eq!(transcript.stage_id.as_deref(), Some("stage-a"));
    assert_eq!(transcript.requested_model.as_deref(), Some("sonnet"));
}

#[test]
fn oversized_lines_are_skipped_without_losing_the_next_line() {
    let mut input = vec![b'x'; MAX_LINE_BYTES + 10];
    input.extend_from_slice(b"\nshort\n");
    let mut reader = std::io::BufReader::new(input.as_slice());
    let mut line = Vec::new();

    let cut = |read: Option<(u64, bool)>| read.map(|(_, cut)| cut);
    let read = next_line(&mut reader, &mut line, MAX_LINE_BYTES).unwrap();
    assert_eq!(cut(read), Some(true));
    let read = next_line(&mut reader, &mut line, MAX_LINE_BYTES).unwrap();
    assert_eq!(cut(read), Some(false));
    assert_eq!(line, b"short");
    assert_eq!(
        next_line(&mut reader, &mut line, MAX_LINE_BYTES).unwrap(),
        None
    );
}

#[test]
fn parent_is_read_once_for_many_subagents() {
    let temp = tempfile::tempdir().unwrap();
    let parent: PathBuf = temp.path().join("session-1.jsonl");
    fs::write(&parent, spawn_row(json!({"model": "haiku"}))).unwrap();
    let mut spawns = super::ParentSpawns::new(None);
    for _ in 0..2 {
        let file = subagent_file(temp.path(), json!({"toolUseId": "toolu_1"}));
        let mut transcript = parse_one(file.clone(), None);
        transcript.requested_model = None;
        spawns.fill_fallbacks(&mut transcript, &file);
        assert_eq!(transcript.requested_model.as_deref(), Some("haiku"));
        // The second pass can only answer from the index, not the file.
        fs::remove_file(&parent).ok();
    }
}
