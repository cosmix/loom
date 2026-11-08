//! The busy line of the orchestrator tick as the snapshot carries it.

use super::*;

fn snapshot_json(work_dir: &Path) -> serde_json::Value {
    let snapshot = collect_snapshot(
        work_dir,
        StatusData::default(),
        SnapshotSource::Files,
        false,
    );
    serde_json::to_value(snapshot).unwrap()
}

fn write_tick(work_dir: &Path, third_line: Option<&str>) {
    let mut content = format!("{}\nevents\n", Utc::now().to_rfc3339());
    if let Some(line) = third_line {
        content.push_str(line);
        content.push('\n');
    }
    std::fs::write(work_dir.join("orchestrator.tick"), content).unwrap();
}

#[test]
fn a_busy_tick_line_reaches_the_wire_with_its_ceilings() {
    let temp = tempfile::TempDir::new().unwrap();
    write_tick(
        temp.path(),
        Some("busy merge-verification 2026-11-07T13:51:08.120Z stage-a"),
    );

    let json = snapshot_json(temp.path());

    assert_eq!(
        json["busy"],
        serde_json::json!({
            "op": "merge-verification",
            "what": "verifying the merge of stage-a",
            "stage_id": "stage-a",
            "started_at": "2026-11-07T13:51:08.120Z",
            "ceiling_secs": 1200,
            "global_ceiling_secs": 1800
        })
    );
}

#[test]
fn a_two_line_tick_serializes_busy_as_null() {
    let temp = tempfile::TempDir::new().unwrap();
    write_tick(temp.path(), None);

    let json = snapshot_json(temp.path());

    assert!(json.as_object().unwrap().contains_key("busy"));
    assert!(json["busy"].is_null());
}

#[test]
fn a_missing_tick_serializes_busy_as_null() {
    let temp = tempfile::TempDir::new().unwrap();

    assert!(snapshot_json(temp.path())["busy"].is_null());
}
