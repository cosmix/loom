use super::*;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::verify::transitions::serialize_stage_to_markdown;
use std::fs;
use tempfile::TempDir;

const PLAN: &str = "doc/plans/IN_PROGRESS-PLAN-strip.md";

fn fixture() -> (TempDir, WorkDir) {
    let temp = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp.path()).unwrap();
    work_dir.initialize().unwrap();
    fs::write(
        work_dir.config_path(),
        format!("[plan]\nsource_path = \"{PLAN}\"\nbase_branch = \"main\"\n"),
    )
    .unwrap();
    // Knowledge stages legitimately have no merge commit or branch.
    write_stage(&work_dir, "first", StageStatus::Completed, true);
    write_stage(&work_dir, "second", StageStatus::Completed, true);
    write_stage(&work_dir, "third", StageStatus::WaitingForInput, false);
    (temp, work_dir)
}

fn write_stage(work_dir: &WorkDir, id: &str, status: StageStatus, merged: bool) {
    let stage = Stage {
        id: id.into(),
        name: id.into(),
        status,
        stage_type: StageType::Knowledge,
        merged,
        ..Stage::default()
    };
    let content = serialize_stage_to_markdown(&stage).unwrap();
    fs::write(work_dir.stages_dir().join(format!("01-{id}.md")), content).unwrap();
}

#[test]
fn build_strip_counts_merged_stages_and_attention() {
    let (_temp, work_dir) = fixture();

    let strip = build_strip(&work_dir, None).unwrap();

    assert_eq!(
        strip,
        RunStrip {
            plan: Some(PLAN.into()),
            daemon_running: false,
            merged: 2,
            total: 3,
            attention: 1,
            dashboard_url: None,
        }
    );
}

#[test]
fn build_strip_removes_tokens_and_preserves_other_query_parameters() {
    let (_temp, work_dir) = fixture();
    let cases = [
        ("http://127.0.0.1:7373/?token=abc", "http://127.0.0.1:7373/"),
        (
            "http://127.0.0.1:7373/?tab=stages&token=abc&view=compact",
            "http://127.0.0.1:7373/?tab=stages&view=compact",
        ),
        (
            "http://127.0.0.1:7373/?token=abc&token=def",
            "http://127.0.0.1:7373/",
        ),
    ];

    for (url, expected) in cases {
        let strip = build_strip(&work_dir, Some(url)).unwrap();

        assert_eq!(strip.dashboard_url.as_deref(), Some(expected));
        let json = serde_json::to_string(&PlanMessage::RunStrip(strip)).unwrap();
        assert!(!json.contains("token="), "{json}");
    }
}

#[test]
fn absent_workspace_sends_one_empty_strip_until_it_appears() {
    let temp = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp.path()).unwrap();
    let first = build_strip(&work_dir, Some("http://127.0.0.1/?token=secret")).unwrap();
    assert_eq!(
        changed_strip(None, Ok(first.clone())).unwrap(),
        Some(first.clone())
    );
    assert_eq!(first, empty_strip());
    assert_eq!(
        changed_strip(Some(&first), build_strip(&work_dir, None)).unwrap(),
        None
    );

    work_dir.initialize().unwrap();
    write_stage(&work_dir, "appeared", StageStatus::WaitingForInput, false);
    let appeared = build_strip(&work_dir, None).unwrap();
    assert_eq!(appeared.total, 1);
    assert_eq!(
        changed_strip(Some(&first), Ok(appeared.clone())).unwrap(),
        Some(appeared)
    );
}

#[test]
fn changed_strip_suppresses_identical_snapshots_and_sends_each_changed_field() {
    let previous = empty_strip();
    let variants = [
        RunStrip {
            plan: Some(PLAN.into()),
            ..previous.clone()
        },
        RunStrip {
            daemon_running: true,
            ..previous.clone()
        },
        RunStrip {
            merged: 1,
            ..previous.clone()
        },
        RunStrip {
            total: 1,
            ..previous.clone()
        },
        RunStrip {
            attention: 1,
            ..previous.clone()
        },
        RunStrip {
            dashboard_url: Some("http://127.0.0.1:7373/".into()),
            ..previous.clone()
        },
    ];

    assert_eq!(
        changed_strip(Some(&previous), Ok(previous.clone())).unwrap(),
        None
    );
    for next in variants {
        assert_eq!(
            changed_strip(Some(&previous), Ok(next.clone())).unwrap(),
            Some(next)
        );
    }
}

#[test]
fn read_error_keeps_the_last_snapshot_for_change_detection() {
    let (_temp, work_dir) = fixture();
    let previous = build_strip(&work_dir, None).unwrap();
    fs::write(work_dir.config_path(), "[invalid TOML").unwrap();

    assert!(changed_strip(Some(&previous), build_strip(&work_dir, None)).is_err());
    assert_eq!(
        changed_strip(Some(&previous), Ok(previous.clone())).unwrap(),
        None
    );
}

#[test]
fn removed_workspace_clears_the_previous_strip_once() {
    let (_temp, work_dir) = fixture();
    let previous = build_strip(&work_dir, None).unwrap();
    fs::remove_dir_all(work_dir.root()).unwrap();

    let cleared = changed_strip(Some(&previous), build_strip(&work_dir, None))
        .unwrap()
        .unwrap();

    assert_eq!(cleared, empty_strip());
    assert_eq!(
        changed_strip(Some(&cleared), build_strip(&work_dir, None)).unwrap(),
        None
    );
}
