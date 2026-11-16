use super::{Correlator, Pending};

#[test]
fn ids_start_at_one_and_increase() {
    let mut correlator = Correlator::new();
    assert_eq!(correlator.register(Pending::Initialize), 1);
    assert_eq!(correlator.register(Pending::ThreadStart), 2);
}

#[test]
fn resolve_returns_the_registered_pending_once() {
    let mut correlator = Correlator::new();
    let id = correlator.register(Pending::TurnStart);
    assert_eq!(correlator.resolve(id), Some(Pending::TurnStart));
    assert_eq!(correlator.resolve(id), None);
}

#[test]
fn resolve_unknown_id_is_none() {
    assert_eq!(Correlator::new().resolve(100), None);
}

#[test]
fn resolve_after_resolve_is_none() {
    let mut correlator = Correlator::new();
    let id = correlator.register(Pending::Initialize);
    correlator.resolve(id);
    assert_eq!(correlator.resolve(id), None);
}

#[test]
fn outstanding_counts_unresolved() {
    let mut correlator = Correlator::new();
    let id = correlator.register(Pending::Initialize);
    correlator.register(Pending::TurnInterrupt);
    assert_eq!(correlator.outstanding(), 2);
    correlator.resolve(id);
    assert_eq!(correlator.outstanding(), 1);
    correlator.resolve(id);
    assert_eq!(correlator.outstanding(), 1);
}

#[test]
fn ids_are_never_reused_after_resolve() {
    let mut correlator = Correlator::new();
    let id = correlator.register(Pending::Initialize);
    correlator.resolve(id);
    assert_eq!(correlator.register(Pending::Initialize), id + 1);
}

#[test]
fn steer_pending_keeps_its_message_id() {
    let mut correlator = Correlator::new();
    let pending = Pending::TurnSteer {
        message_id: "message-1".into(),
    };
    let id = correlator.register(pending.clone());
    assert_eq!(correlator.resolve(id), Some(pending));
}
