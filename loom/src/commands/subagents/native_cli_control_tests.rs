use super::*;

#[test]
fn in_session_cancel_relays_and_refuses_stage_flag() {
    let mut h = Harness::new();
    assert_eq!(h.run(cancel(Some("stage-a"))), 3);
    h.refused("--stage");
    assert_eq!(h.run(cancel(None)), 0);
    assert!(
        matches!(h.payload(), WorkerControlPayload::Cancel { reason, .. } if reason == "stop now")
    );
}
#[test]
fn operator_cancel_writes_cancel_json_and_refuses_inside_a_session() {
    let mut h = Harness::new();
    let dir = h.record();
    assert_eq!(h.run(cancel(Some("stage-a"))), 3);
    assert!(store::read_cancel(&dir).unwrap().is_none());
    h.session = false;
    assert_eq!(h.run(cancel(Some("stage-a"))), 0);
    let request = store::read_cancel(&dir).unwrap().unwrap();
    assert_eq!(
        (request.by.as_str(), request.reason.as_str()),
        ("operator", "stop now")
    );
    assert!(String::from_utf8_lossy(&h.out).contains(INV));
}
#[test]
fn release_refuses_held_ownership() {
    let mut h = Harness::new();
    h.session = false;
    h.record();
    assert_eq!(h.run(release()), 3);
    h.refused("ownership is held");
}
#[test]
fn release_appends_operator_release_when_the_boot_id_changed() {
    let mut h = Harness::new();
    h.session = false;
    let dir = h.record();
    store::write_boundary(
        &dir,
        &BoundaryIdentity {
            supervisor_pid: 1,
            supervisor_start: "1".into(),
            boundary_pid: 1,
            pid_namespace: 1,
            boot_id: "other-boot".into(),
        },
    )
    .unwrap();
    store::EventLog::open(&dir)
        .unwrap()
        .append(NativeEvent::OwnershipUnverified {
            detail: "lost".into(),
        })
        .unwrap();
    assert_eq!(h.run(release()), 0);
    let state = index::unit_state(&h.root(), "stage-a", "unit")
        .unwrap()
        .unwrap();
    assert_eq!(state.ownership, Ownership::Released);
    assert!(
        matches!(&store::read_events(&dir).unwrap().last().unwrap().event,
        NativeEvent::OwnershipReleased { by } if by == "operator")
    );
}

#[test]
fn cancel_refuses_invalid_reason_or_invocation_before_relay() {
    for (reason, invocation, flag) in [
        (" ".into(), INV, "--reason"),
        ("x".repeat(1025), INV, "--reason"),
        ("stop".into(), "bad", "--invocation"),
    ] {
        let mut h = Harness::new();
        let args = CancelArgs {
            worker: "codex:unit".into(),
            stage: None,
            invocation: Some(invocation.into()),
            reason,
        };
        assert_eq!(h.run(SubagentsCommand::Cancel(args)), 3);
        h.refused(flag);
    }
}

#[test]
fn release_refuses_inside_a_session() {
    let mut h = Harness::new();
    assert_eq!(h.run(release()), 3);
    h.refused("release is an operator command");
}
