use super::*;
use crate::tokens::tail::MAX_LINE_BYTES;
use serde_json::json;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::sync::{Arc, Barrier};

fn paths(dir: &Path) -> RuntimePaths {
    RuntimePaths {
        planning_root: dir.to_path_buf(),
        dir: dir.to_path_buf(),
        lock: dir.join("lock"),
        drafts: dir.join("drafts"),
        claims: dir.join("claims"),
        summaries_lock: dir.join("summaries.lock"),
        summary_cost: dir.join("summary-cost.jsonl"),
        sessions: dir.join("sessions"),
    }
}

fn cost_line(id: &str) -> CostLine {
    CostLine {
        time: "2026-10-28T09:00:00Z".to_owned(),
        trigger: Trigger::Background,
        summaries: vec![id.to_owned()],
        level: Level::Document,
        model: None,
        source_bytes: None,
        tokens: TokenVector::default(),
    }
}

fn transcript_line(id: &str, output: u64) -> String {
    json!({
        "type": "assistant",
        "timestamp": "2026-10-28T09:00:00Z",
        "message": {
            "id": id,
            "model": "claude-sonnet-5-5",
            "usage": {
                "input_tokens": 2,
                "cache_creation_input_tokens": 3,
                "cache_read_input_tokens": 5,
                "output_tokens": output
            }
        }
    })
    .to_string()
}

#[test]
fn absent_fields_are_omitted_from_json() {
    let line = cost_line("plans/a");
    let value = serde_json::to_value(&line).expect("serialize cost");

    assert_eq!(
        value,
        json!({
            "time": "2026-10-28T09:00:00Z",
            "trigger": "background",
            "summaries": ["plans/a"],
            "level": "document",
            "tokens": {}
        })
    );
    assert_eq!(serde_json::from_value::<CostLine>(value).unwrap(), line);
}

#[test]
fn known_zero_is_serialized() {
    let tokens = TokenVector {
        fresh_input: Some(0),
        ..TokenVector::default()
    };

    assert_eq!(
        serde_json::to_value(tokens).unwrap(),
        json!({"fresh_input": 0})
    );
}

#[test]
fn concurrent_appends_produce_two_whole_records() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    let barrier = Arc::new(Barrier::new(2));
    let workers = ["plans/a", "plans/b"].map(|id| {
        let paths = paths.clone();
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            let mut line = cost_line(id);
            line.model = Some("x".repeat(128 * 1024));
            barrier.wait();
            append(&paths, &line).unwrap();
        })
    });
    for worker in workers {
        worker.join().unwrap();
    }

    let bytes = std::fs::read(&paths.summary_cost).unwrap();
    assert!(bytes.ends_with(b"\n"));
    assert_eq!(bytes.iter().filter(|byte| **byte == b'\n').count(), 2);
    let mut ids: Vec<_> = read_log(&paths)
        .unwrap()
        .into_iter()
        .flat_map(|line| line.summaries)
        .collect();
    ids.sort();
    assert_eq!(ids, ["plans/a", "plans/b"]);
}

#[test]
fn new_log_has_private_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());

    append(&paths, &cost_line("plans/a")).unwrap();

    let mode = std::fs::metadata(&paths.summary_cost)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn read_log_skips_bad_lines_and_unfinished_final_record() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    let line = cost_line("plans/a");
    let mut bytes = serde_json::to_vec(&line).unwrap();
    bytes.extend_from_slice(b"\ninvalid json\n\xff\n");
    bytes.extend_from_slice(&serde_json::to_vec(&cost_line("plans/b")).unwrap());
    std::fs::write(&paths.summary_cost, bytes).unwrap();

    assert_eq!(read_log(&paths).unwrap(), vec![line]);
}

#[test]
fn missing_log_is_empty() {
    let dir = tempfile::tempdir().unwrap();

    assert!(read_log(&paths(dir.path())).unwrap().is_empty());
}

#[test]
fn log_reads_and_appends_reject_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    let target = dir.path().join("target");
    std::fs::write(&target, b"untouched").unwrap();
    symlink(&target, &paths.summary_cost).unwrap();

    assert!(append(&paths, &cost_line("plans/a")).is_err());
    assert!(read_log(&paths).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"untouched");
}

#[test]
fn log_over_read_limit_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    File::create(&paths.summary_cost)
        .unwrap()
        .set_len(MAX_LOG_BYTES as u64 + 1)
        .unwrap();

    assert!(read_log(&paths).is_err());
}

#[test]
fn log_over_read_limit_yields_its_tail() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    // The first record alone fills the read window, so the cut falls inside it.
    let mut filler = cost_line("plans/filler");
    filler.model = Some("x".repeat(MAX_LOG_BYTES));
    for line in [filler, cost_line("plans/a"), cost_line("plans/b")] {
        append(&paths, &line).unwrap();
    }
    let len = std::fs::metadata(&paths.summary_cost).unwrap().len();
    assert!(len > MAX_LOG_BYTES as u64);

    let ids: Vec<_> = read_log(&paths)
        .unwrap()
        .into_iter()
        .flat_map(|line| line.summaries)
        .collect();
    assert_eq!(ids, ["plans/a", "plans/b"]);
}

#[test]
fn transcript_skips_final_line_without_newline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("transcript.jsonl");
    let text = [transcript_line("a", 7), transcript_line("b", 100)].join("\n");
    std::fs::write(&path, text).unwrap();

    assert_eq!(
        transcript_tokens(&path).unwrap(),
        TokenVector {
            fresh_input: Some(2),
            cache_write: Some(3),
            cache_read: Some(5),
            output: Some(7)
        }
    );
}

#[test]
fn unreadable_transcripts_have_unknown_usage() {
    let dir = tempfile::tempdir().unwrap();

    assert_eq!(
        transcript_tokens(&dir.path().join("absent.jsonl")).unwrap(),
        TokenVector::default()
    );
    assert_eq!(
        transcript_tokens(dir.path()).unwrap(),
        TokenVector::default()
    );
}

#[test]
fn oversized_transcript_line_is_skipped_and_next_line_is_counted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("transcript.jsonl");
    let mut file = File::create(&path).unwrap();
    // Leading whitespace leaves valid JSON that would count if the bound failed.
    file.write_all(&vec![b' '; MAX_LINE_BYTES]).unwrap();
    writeln!(file, "{}", transcript_line("too-long", 100)).unwrap();
    writeln!(file, "{}", transcript_line("a", 7)).unwrap();
    drop(file);

    assert_eq!(
        transcript_tokens(&path).unwrap(),
        TokenVector {
            fresh_input: Some(2),
            cache_write: Some(3),
            cache_read: Some(5),
            output: Some(7)
        }
    );
}
