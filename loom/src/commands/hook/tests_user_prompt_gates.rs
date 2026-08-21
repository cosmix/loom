//! Tests for the emit floor and the dedupe-omitted count.
//!
//! Split out of `tests_user_prompt.rs` itself so that file stays under the
//! maintainability line limit — same idiom `tests_user_prompt.rs` uses for its
//! own `tests_user_prompt_e2e.rs` child: no repeated `#[cfg(test)]`, since this
//! whole file is already gated by it.

use super::super::compose::compose;
use super::ContextItem;
use super::{default_config, delivered, item, pack_of};
use crate::context::schema::{ItemKind, SelectionReason};
use std::collections::BTreeSet;

/// A source-graph unit scored only by loose lexical overlap: no exact-rung
/// reason, and (being a source node rather than a knowledge chunk) no
/// knowledge-term count to lean on either. This is the shape the emit floor
/// exists to silence.
fn weak_source_item(id: &str) -> ContextItem {
    let mut unit = item(id, "sha256:aa", Some("some excerpt"));
    unit.kind = ItemKind::SourceNode;
    unit.reasons = vec![SelectionReason::Lexical];
    unit.matched_term_count = 0;
    unit
}

#[test]
fn a_weak_lexical_source_item_alone_clears_no_floor() {
    let pack = pack_of(vec![weak_source_item("src#zorble#0")]);
    assert!(
        compose("stage-a", &pack, &BTreeSet::new(), &default_config()).is_none(),
        "one weak lexical match on a source node says nothing worth surfacing"
    );
}

#[test]
fn an_exact_rung_item_clears_the_floor_on_its_own() {
    let mut unit = weak_source_item("src#zorble#0");
    unit.reasons = vec![SelectionReason::ExactSymbol];
    let pack = pack_of(vec![unit]);
    assert!(
        compose("stage-a", &pack, &BTreeSet::new(), &default_config()).is_some(),
        "an exact-rung reason clears the floor regardless of term count"
    );
}

#[test]
fn a_knowledge_item_at_exactly_the_term_floor_is_emitted() {
    let config = default_config();
    let mut unit = item("arch#loop#0", "sha256:aa", Some("The loop polls."));
    unit.matched_term_count = config.min_knowledge_terms;
    let pack = pack_of(vec![unit]);

    assert!(
        compose("stage-a", &pack, &BTreeSet::new(), &config).is_some(),
        "the boundary: exactly min_knowledge_terms clears the floor"
    );
}

#[test]
fn a_knowledge_item_one_below_the_term_floor_is_silent() {
    let config = default_config();
    let mut unit = item("arch#loop#0", "sha256:aa", Some("The loop polls."));
    unit.matched_term_count = config.min_knowledge_terms - 1;
    let pack = pack_of(vec![unit]);

    assert!(
        compose("stage-a", &pack, &BTreeSet::new(), &config).is_none(),
        "one below the floor is exactly the case it exists to silence"
    );
}

#[test]
fn a_dedupe_drop_is_folded_into_the_omitted_count() {
    let pack = pack_of(vec![
        item("arch#loop#0", "sha256:aa", Some("The loop polls.")),
        item("arch#merge#0", "sha256:bb", Some("Merging is verified.")),
        item(
            "arch#verify#0",
            "sha256:cc",
            Some("Verification runs after."),
        ),
    ]);
    let two_already_delivered =
        delivered(&[("arch#loop#0", "sha256:aa"), ("arch#merge#0", "sha256:bb")]);

    let (_, handed_over) = compose("stage-a", &pack, &two_already_delivered, &default_config())
        .expect("one undelivered unit survives");

    assert_eq!(
        handed_over.omitted.omitted,
        pack.omitted.omitted + 2,
        "the two deduped units must count toward what the footer reports"
    );
}
