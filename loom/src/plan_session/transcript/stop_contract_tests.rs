//! Contract: Stop in the browser leaves one notice in the turn. The server's "Stopped" notice
//! comes first, and Claude Code's interrupt record that follows it adds no "Interrupted" notice.

use std::path::PathBuf;

use super::Mapper;
use crate::plan_session::model::{Item, NoticeKind};

fn fixture_lines() -> Vec<String> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/plan_session/transcripts/interrupt.jsonl"
    );
    let text = std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{path}: {err}"));
    text.lines().map(str::to_owned).collect()
}

#[test]
fn stop_then_interrupt_shows_one_notice() {
    let lines = fixture_lines();
    let (interrupt, before) = lines.split_last().expect("fixture lines");
    assert!(
        interrupt.contains("[Request interrupted by user"),
        "the fixture's last record is the interrupt: {interrupt}"
    );
    let mut mapper = Mapper::new(PathBuf::from("/work/proj"));
    for line in before {
        mapper.ingest_main(line);
    }
    mapper.add_server_item(Item::Notice {
        id: "stopped-1".to_owned(),
        kind: NoticeKind::Stopped,
        text: "Stopped".to_owned(),
    });
    mapper.ingest_main(interrupt);

    assert_eq!(mapper.turns().len(), 1, "{:?}", mapper.turns());
    let notices: Vec<&NoticeKind> = mapper.turns()[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Notice { kind, .. } => Some(kind),
            _ => None,
        })
        .collect();
    assert_eq!(notices, [&NoticeKind::Stopped], "{:?}", mapper.turns()[0]);
}
