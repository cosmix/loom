//! Contract: a local command's output (the `/context` table) becomes one clean notice: no ANSI
//! escapes, no controls but newline and tab, no surrounding whitespace.

use std::path::PathBuf;

use super::Mapper;
use crate::plan_session::model::{Item, NoticeKind};

const USAGE_CONTEXT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/plan_session/transcripts/p2-usage-context.jsonl"
));

/// Every info notice of the mapped transcript, in order.
fn info_notices(mapper: &Mapper) -> Vec<String> {
    let items = mapper.turns().iter().flat_map(|turn| turn.items.iter());
    items
        .filter_map(|item| match item {
            Item::Notice {
                kind: NoticeKind::Info,
                text,
                ..
            } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn local_command_output_becomes_a_clean_notice() {
    assert!(
        USAGE_CONTEXT.contains("<local-command-stdout>") && USAGE_CONTEXT.contains("\\u001b["),
        "the fixture holds the /context stdout record with its ANSI escapes"
    );
    let mut mapper = Mapper::new(PathBuf::from("/work/proj"));
    for line in USAGE_CONTEXT.lines() {
        mapper.ingest_main(line);
    }

    let notices = info_notices(&mapper);
    let context = notices
        .iter()
        .find(|text| text.contains("Context Usage"))
        .unwrap_or_else(|| panic!("no notice holds the /context output: {notices:?}"));
    assert!(
        !context.contains('\u{1b}'),
        "ANSI escapes kept: {context:?}"
    );
    let control = context
        .chars()
        .find(|c| c.is_control() && *c != '\n' && *c != '\t');
    assert_eq!(control, None, "a control character kept: {context:?}");
    assert_eq!(
        context.trim(),
        context.as_str(),
        "the notice keeps surrounding whitespace"
    );
    assert!(
        !context.contains("<local-command-stdout>"),
        "the record's tag reached the notice: {context:?}"
    );
}
