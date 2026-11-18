use super::{render_excerpt_block, render_knowledge_item};
use crate::context::schema::{
    Channel, ChunkId, Confidence, ContextItem, ItemKind, LifecycleState, SelectionReason,
    SourcePointer,
};
use std::path::PathBuf;

#[test]
fn excerpt_neutralizes_terminal_controls_and_sizes_fence() {
    let excerpt = "source\tline\n```\n\u{1b}]0;title\u{7}\u{202e}end";
    assert_eq!(
        render_excerpt_block(excerpt),
        "````text\nsource\tline\n```\n\u{fffd}]0;title\u{fffd} end\n````\n"
    );
    assert!(excerpt.contains('\u{1b}'));
}

#[test]
fn knowledge_item_preserves_identity_while_sanitizing_excerpt() {
    let item = ContextItem {
        id: ChunkId::from("knowledge:sample"),
        kind: ItemKind::KnowledgeChunk,
        pointer: SourcePointer {
            path: PathBuf::from("doc/source.md"),
            anchor: "section".into(),
            line_start: Some(3),
            line_end: Some(7),
        },
        summary: String::new(),
        source: Channel::Knowledge,
        token_count: 0,
        score: 1.0,
        reasons: vec![SelectionReason::Lexical],
        confidence: Confidence::High,
        state: LifecycleState::Active,
        content_hash: "sha256:original".into(),
        excerpt: Some("tab\there\n```\n\u{1b}[31m\u{1b}]0;title\u{7}\u{202e}end".into()),
        truncated: false,
        matched_term_count: 0,
        explanation: None,
        caveat: None,
        window: None,
    };
    let original = item.clone();
    let rendered = render_knowledge_item(&item);

    assert!(rendered.contains("`doc/source.md:3-7#section`"));
    assert!(rendered
        .contains("````text\ntab\there\n```\n\u{fffd}[31m\u{fffd}]0;title\u{fffd} end\n````"));
    assert!(!rendered.contains('\u{1b}'));
    assert!(!rendered.contains('\u{202e}'));
    assert_eq!(item, original);
}
