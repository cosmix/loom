use super::*;

const SIBLING: &str = "inv-ffffffffffffffffffffffffffffffff";

fn unverified(dir: &std::path::Path) {
    store::EventLog::open(dir)
        .unwrap()
        .append(NativeEvent::OwnershipUnverified {
            detail: "lost".into(),
        })
        .unwrap();
}

fn corrupt_sibling(h: &Harness, own: &std::path::Path) -> PathBuf {
    let mut manifest = store::read_manifest(own).unwrap();
    manifest.invocation_id = SIBLING.into();
    let dir = store::create_invocation(&h.root(), &manifest, b"task\n").unwrap();
    fs::write(dir.join("events.jsonl"), b"broken\n").unwrap();
    dir
}

#[test]
fn release_ignores_a_corrupt_sibling_record() {
    let mut h = Harness::new();
    h.session = false;
    let dir = h.record();
    unverified(&dir);
    corrupt_sibling(&h, &dir);
    assert_eq!(h.run(release()), 0);
    assert!(matches!(
        &store::read_events(&dir).unwrap().last().unwrap().event,
        NativeEvent::OwnershipReleased { by } if by == "operator"
    ));
}

#[test]
fn release_of_a_corrupt_record_names_the_path_and_manual_remedy() {
    let mut h = Harness::new();
    h.session = false;
    let dir = h.record();
    fs::write(dir.join("events.jsonl"), b"broken\n").unwrap();
    assert_eq!(h.run(release()), 3);
    let err = String::from_utf8_lossy(&h.err);
    assert!(err.contains(&dir.display().to_string()), "{err}");
    assert!(
        err.contains("confirm no process of this worker still runs"),
        "{err}"
    );
    assert!(err.contains("remove that directory"), "{err}");
    assert_eq!(fs::read(dir.join("events.jsonl")).unwrap(), b"broken\n");
}
