//! Bridge tests: `set_rebuilding` stores the rebuilt summary ids on the turn, so a snapshot
//! (a browser reconnect) and a later update of the same turn keep them.

use super::fake_host::*;
use crate::plan_session::kind::SessionKind::Authoring;
use crate::plan_session::model::ChatDelta;

fn listed(h: &Harness, turn_id: &str) -> Vec<String> {
    let turns = h.bridge.chat_snapshot(true).turns;
    let found = turns.into_iter().find(|turn| turn.id == turn_id);
    found.expect("the turn").summaries_rebuilding
}

fn published(h: &Harness, turn_id: &str) -> Vec<Vec<String>> {
    let deltas = h.deltas.lock().unwrap();
    let ids = deltas.iter().filter_map(|delta| match delta {
        ChatDelta::TurnUpdated {
            turn_id: id,
            summaries_rebuilding,
            ..
        } if id == turn_id => Some(summaries_rebuilding.clone()),
        _ => None,
    });
    ids.collect()
}

#[test]
fn rebuilt_summary_ids_survive_a_snapshot_and_a_later_update_of_the_turn() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let turn_id = h
        .bridge
        .chat_snapshot(true)
        .turns
        .last()
        .unwrap()
        .id
        .clone();
    h.bridge
        .set_rebuilding(Some((&s.id, &turn_id)), vec!["a".to_owned()]);
    assert_eq!(published(&h, &turn_id).last(), Some(&vec!["a".to_owned()]));
    assert_eq!(listed(&h, &turn_id), ["a"], "the snapshot lost the ids");

    let before = published(&h, &turn_id).len();
    h.record(&s, interrupted());
    wait_until("the turn update", || published(&h, &turn_id).len() > before);
    assert_eq!(published(&h, &turn_id).last(), Some(&vec!["a".to_owned()]));
    assert_eq!(listed(&h, &turn_id), ["a"]);

    h.bridge.set_rebuilding(None, vec!["b".to_owned()]);
    assert_eq!(listed(&h, &turn_id), ["b"], "None names the latest turn");
    h.bridge
        .set_rebuilding(Some((&s.id, "t999")), vec!["c".to_owned()]);
    assert_eq!(listed(&h, &turn_id), ["b"], "an unknown turn changed one");
}
