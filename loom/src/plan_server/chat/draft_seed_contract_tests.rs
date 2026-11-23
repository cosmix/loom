//! Contract of a continuation's draft: the confirmed document it resumes is part of the seed
//! and is stored by the draft's first write, so no second write can fail and lose it. A seed
//! field the draft ignores, left for a later update to fill in, is rejected.

use super::drafts::{self, Seed};
use super::tests::Fixture;
use super::ChatState;
use crate::plan_session::scope::KickoffSize;

const SESSION: &str = "33333333-3333-4333-8333-333333333333";
// Absent from the project, so the draft is not taken for a landed one and deleted.
const TARGET: &str = "doc/plans/PLAN-alpha.md";

#[test]
fn a_continuation_draft_holds_its_target_from_its_first_write() {
    let f = Fixture::new(false);
    let state = ChatState::default();
    let seed = Seed {
        size: KickoffSize::Feature,
        messages: Vec::new(),
        attachments: Vec::new(),
        shape: None,
        target: Some(TARGET.to_owned()),
    };
    drafts::create(&f.ctx, &state, SESSION, seed).unwrap();

    let stored = drafts::get(&f.ctx, &state, SESSION).expect("the draft was not stored");
    assert_eq!(stored.target.as_deref(), Some(TARGET), "{stored:?}");
}
