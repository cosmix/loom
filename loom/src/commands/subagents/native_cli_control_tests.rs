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
            mac: None,
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

fn mark_unverified(dir: &std::path::Path) {
    store::EventLog::open(dir)
        .unwrap()
        .append(NativeEvent::OwnershipUnverified {
            detail: "lost".into(),
        })
        .unwrap();
}

/// Records a macOS identity from another boot whose private root is
/// `scratch_root`.
fn write_mac_identity(dir: &std::path::Path, scratch_root: PathBuf) {
    let mac = MacIdentity {
        invocation: INV.into(),
        session: 4242,
        leader_start: "1".into(),
        boot_id: "other-boot".into(),
        scratch_root,
    };
    let identity = BoundaryIdentity {
        supervisor_pid: 1,
        supervisor_start: "1".into(),
        boundary_pid: 4242,
        pid_namespace: 0,
        boot_id: "other-boot".into(),
        mac: Some(mac),
    };
    store::write_boundary(dir, &identity).unwrap();
}

fn ownership(h: &Harness) -> Ownership {
    index::unit_state(&h.root(), "stage-a", "unit")
        .unwrap()
        .unwrap()
        .ownership
}

#[test]
fn release_after_proof_removes_the_macos_private_root() {
    let mut h = Harness::new();
    h.session = false;
    let dir = h.record();
    let cache = tempfile::tempdir().unwrap();
    let root = cache
        .path()
        .canonicalize()
        .unwrap()
        .join("native")
        .join(INV);
    fs::create_dir_all(root.join("tmp")).unwrap();
    fs::write(root.join("tmp").join("scratch"), "x").unwrap();
    write_mac_identity(&dir, root.clone());
    mark_unverified(&dir);
    assert_eq!(h.run(release()), 0);
    assert_eq!(ownership(&h), Ownership::Released);
    assert!(!root.exists(), "the private root outlived the release");
}

#[test]
fn release_keeps_the_record_unreleased_when_the_private_root_cannot_be_removed() {
    let mut h = Harness::new();
    h.session = false;
    let dir = h.record();
    let cache = tempfile::tempdir().unwrap();
    // Not a `native/<invocation>` root: the removal refuses it.
    let root = cache.path().canonicalize().unwrap().join("elsewhere");
    fs::create_dir_all(&root).unwrap();
    write_mac_identity(&dir, root.clone());
    mark_unverified(&dir);
    assert_eq!(h.run(release()), 3);
    h.refused("could not be removed");
    assert_eq!(ownership(&h), Ownership::Unverified);
    assert!(root.exists());
}

#[cfg(target_os = "macos")]
#[test]
fn release_refuses_a_begun_macos_spawn_without_an_identity() {
    let mut h = Harness::new();
    h.session = false;
    let dir = h.record();
    store::mark_spawn_begun(&dir).unwrap();
    mark_unverified(&dir);
    assert_eq!(h.run(release()), 3);
    h.refused("no boundary identity was recorded");
    assert_eq!(ownership(&h), Ownership::Unverified);
}
