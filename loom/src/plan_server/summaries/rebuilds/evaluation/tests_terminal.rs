//! The panel's terminal-claim list follows each evaluation, not only the start-up sweep.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::super::tests::Probe;
use super::super::Rebuilds;
use crate::planning::summary::claims::Claim;
use crate::planning::summary::engine::EngineUpdate;
use crate::planning::summary::evaluate::{Evaluation, SummaryEval, SummaryState};
use crate::planning::summary::store::Scope;

const MEMBER: &str = "plans/alpha";
const REMOVED: &str = "plans/removed";

/// `(id, claimant)`: `None` is no claim, `Some(None)` a claim from a terminal.
type Row = (&'static str, Option<Option<&'static str>>);

fn update(rows: &[Row]) -> EngineUpdate {
    let eval = |&(id, claim): &Row| {
        let claim = claim.map(|session| Claim {
            summary: id.to_owned(),
            session: session.map(str::to_owned),
            pid: 1,
            created_at: String::new(),
        });
        let summary = SummaryEval {
            id: id.to_owned(),
            source: None,
            state: SummaryState::Current,
            file: None,
            sentences: Vec::new(),
            claim,
            error: None,
        };
        (id.to_owned(), summary)
    };
    EngineUpdate {
        evaluation: Evaluation {
            summaries: rows.iter().map(eval).collect::<BTreeMap<_, _>>(),
            scope: Scope::default(),
            orphans: Vec::new(),
        },
        rewritten: Vec::new(),
        deleted: Vec::new(),
        invalidated: Vec::new(),
    }
}

fn facade() -> Arc<Rebuilds> {
    Rebuilds::new_for_test(Arc::new(Probe::default()), None, Arc::new(|_| {}))
}

fn listed(rebuilds: &Rebuilds) -> Vec<String> {
    rebuilds.panel().terminal_claims
}

#[test]
fn a_claim_made_from_a_terminal_while_serving_is_listed() {
    let rebuilds = facade();
    rebuilds.on_update(&update(&[(MEMBER, None)]));
    assert!(listed(&rebuilds).is_empty());

    rebuilds.on_update(&update(&[(MEMBER, Some(None))]));
    assert_eq!(listed(&rebuilds), vec![MEMBER.to_owned()]);
}

#[test]
fn a_session_claim_is_not_listed_and_a_released_claim_leaves_the_list() {
    let rebuilds = facade();
    rebuilds.on_update(&update(&[(MEMBER, Some(Some("s1")))]));
    assert!(listed(&rebuilds).is_empty());

    rebuilds.on_update(&update(&[(MEMBER, Some(None))]));
    assert_eq!(listed(&rebuilds), vec![MEMBER.to_owned()]);
    rebuilds.on_update(&update(&[(MEMBER, None)]));
    assert!(listed(&rebuilds).is_empty());
}

#[test]
fn a_listed_claim_on_an_unevaluated_summary_stays_until_released() {
    let rebuilds = facade();
    rebuilds.on_update(&update(&[(REMOVED, Some(None))]));
    rebuilds.on_update(&update(&[(MEMBER, None)]));
    assert_eq!(listed(&rebuilds), vec![REMOVED.to_owned()]);
}
