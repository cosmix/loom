//! The files: archive, current copy, run identity and the finish sequence.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::record::PlanStatsRecord;
use super::tests_fixture::*;
use super::{
    file_name, finish_plan, loom_home, publish, read_current, remove_current, stamp_new_run,
    write_current, Published, CURRENT_FILE,
};
use crate::models::stage::{Stage, StageStatus};
use crate::user_config::redirect_user_config;
use crate::verify::transitions::update_stage;

fn record_with(plan_name: &str) -> PlanStatsRecord {
    let mut record = super::build::assemble(
        &run(),
        plan_name.to_string(),
        vec![finished("a", 0, 60, 60)],
        &two_lane_tokens(),
    )
    .unwrap();
    record.plan_id = PLAN_ID.to_string();
    record
}

fn finished_state(root: &Path) -> PathBuf {
    work_dir(
        root,
        PLAN_TABLE,
        &[finished("a", 0, 60, 60), stage("b", StageStatus::Skipped)],
    )
}

fn write_text(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
}

#[test]
fn the_file_name_is_the_plan_id_made_safe_and_bounded() {
    assert_eq!(file_name(&record_with("x")), format!("{PLAN_ID}.json"));
    let mut record = record_with("x");
    record.plan_id = "../../etc/pass wd\u{202e}".to_string();
    assert_eq!(file_name(&record), "------etc-pass-wd-.json");
    record.plan_id = "a".repeat(1_000);
    assert_eq!(file_name(&record).len(), 128 + ".json".len());
    record.plan_id = String::new();
    assert_eq!(file_name(&record), "plan.json");
}

#[test]
fn publish_creates_a_private_archive_and_keeps_the_first_record() {
    use std::os::unix::fs::PermissionsExt;
    let home = TempDir::new().unwrap();
    let first = record_with("First");
    let Published::Written(path) = publish(&first, home.path()).unwrap() else {
        panic!("the first publish writes");
    };
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(path.parent().unwrap()), 0o700);
    let second = record_with("Second");
    assert_eq!(
        publish(&second, home.path()).unwrap(),
        Published::AlreadyPublished(path.clone())
    );
    let kept = PlanStatsRecord::from_json(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(kept.plan_name, "First");
}

#[test]
fn an_unreadable_or_unrelated_archive_file_is_reported_and_left_alone() {
    let home = TempDir::new().unwrap();
    let record = record_with("x");
    let Published::Written(path) = publish(&record, home.path()).unwrap() else {
        panic!("first publish writes");
    };
    for planted in ["not json", "{}"] {
        write_text(&path, planted);
        assert!(publish(&record, home.path()).is_err(), "{planted}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), planted);
    }
    let mut other = record_with("other");
    other.plan_id = "20261121T080000Z-other-run".to_string();
    write_text(&path, &String::from_utf8(other.to_json().unwrap()).unwrap());
    assert!(
        publish(&record, home.path()).is_err(),
        "another run's record"
    );
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(home.path().join("elsewhere"), &path).unwrap();
    assert!(
        publish(&record, home.path()).is_err(),
        "a symlinked archive file"
    );
    assert!(!home.path().join("elsewhere").exists());
}

#[test]
fn an_oversized_record_is_refused_before_anything_is_written() {
    let home = TempDir::new().unwrap();
    let mut record = record_with("x");
    record.plan_name = "n".repeat(super::MAX_RECORD_BYTES);
    assert!(publish(&record, home.path()).is_err());
    assert!(!home.path().join("stats").exists());
}

#[test]
fn the_current_copy_round_trips_and_is_removed_idempotently() {
    let home = TempDir::new().unwrap();
    let _redirect = redirect_user_config(home.path().join("config.toml"));
    let root = TempDir::new().unwrap();
    let work_dir = finished_state(root.path());
    let record = record_with("Demo plan");
    assert_eq!(read_current(&work_dir), None, "nothing written yet");
    write_current(&work_dir, &record).unwrap();
    assert_eq!(read_current(&work_dir), Some(record));
    remove_current(&work_dir).unwrap();
    remove_current(&work_dir).unwrap();
    assert_eq!(read_current(&work_dir), None);
}

#[test]
fn a_stale_or_foreign_current_copy_is_not_shown() {
    let root = TempDir::new().unwrap();
    let work_dir = finished_state(root.path());
    let mut record = record_with("Demo plan");
    record.plan_id = "20250101T000000Z-old-0b8f5d2e-4c1a-4a57-9f6e-2d7c1b3a9e11".to_string();
    write_current(&work_dir, &record).unwrap();
    assert_eq!(read_current(&work_dir), None, "an earlier run's record");

    write_current(&work_dir, &record_with("Demo plan")).unwrap();
    assert!(read_current(&work_dir).is_some());
    update_stage("a", &work_dir, |stage| {
        stage.status = StageStatus::Queued;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        read_current(&work_dir),
        None,
        "a reset stage reopens the plan"
    );
}

#[test]
fn a_planted_current_copy_is_bounded_checked_and_neutralized() {
    let root = TempDir::new().unwrap();
    let work_dir = finished_state(root.path());
    let path = work_dir.join(CURRENT_FILE);
    write_text(&path, &"x".repeat(super::MAX_RECORD_BYTES + 1));
    assert_eq!(read_current(&work_dir), None, "over the bound");
    write_text(&path, "{\"schema_version\":1}");
    assert_eq!(read_current(&work_dir), None, "not a record");

    let mut record = record_with("Demo\u{1b}[2J \u{202e}plan");
    record.stages[0].name = "evil\u{0}\u{202e}name".to_string();
    record.stages[0].model = Some("m\nodel".to_string());
    record.wall_secs = i64::MAX;
    record.execution_secs = -7;
    record.totals.cache_read = u64::MAX;
    record.stages[0].tokens.output = u64::MAX;
    record.models[0].tokens.input = u64::MAX;
    write_current(&work_dir, &record).unwrap();
    let shown = read_current(&work_dir).expect("a planted record of the run still reads");
    let limit = (1i64 << 53) - 1;
    assert!(!shown.plan_name.chars().any(char::is_control));
    assert!(!shown.plan_name.contains('\u{202e}'));
    assert!(!shown.stages[0].name.contains(['\u{0}', '\u{202e}']));
    assert!(!shown.stages[0].model.as_deref().unwrap().contains('\n'));
    assert_eq!((shown.wall_secs, shown.execution_secs), (limit, 0));
    assert_eq!(shown.totals.cache_read, limit as u64);
    assert_eq!(shown.stages[0].tokens.output, limit as u64);
    assert_eq!(shown.models[0].tokens.input, limit as u64);
}

#[test]
fn a_planted_current_copy_with_names_the_build_never_writes_is_not_shown() {
    let root = TempDir::new().unwrap();
    let work_dir = finished_state(root.path());
    let plant = |edit: &dyn Fn(&mut super::PlanStatsRecord)| {
        let mut record = record_with("Demo plan");
        edit(&mut record);
        write_current(&work_dir, &record).unwrap();
        read_current(&work_dir)
    };
    assert!(plant(&|_| {}).is_some(), "the unedited record reads");
    assert_eq!(plant(&|r| r.models[0].lane = "x".to_string()), None, "lane");
    assert_eq!(plant(&|r| r.stages[0].status = "bogus".to_string()), None);
    assert_eq!(
        plant(&|r| r.stages[0].status = "pending".to_string()),
        None,
        "an alias is not the serialized name"
    );
    assert_eq!(plant(&|r| r.stages[0].stage_type = "x".to_string()), None);
}

#[test]
fn a_symlinked_current_copy_is_not_followed() {
    let root = TempDir::new().unwrap();
    let work_dir = finished_state(root.path());
    let elsewhere = root.path().join("elsewhere.json");
    write_text(
        &elsewhere,
        &String::from_utf8(record_with("x").to_json().unwrap()).unwrap(),
    );
    std::os::unix::fs::symlink(&elsewhere, work_dir.join(CURRENT_FILE)).unwrap();
    assert_eq!(read_current(&work_dir), None);
}

#[test]
fn two_initializations_of_one_plan_file_archive_separately() {
    let home = TempDir::new().unwrap();
    let _redirect = redirect_user_config(home.path().join("config.toml"));
    let mut files = Vec::new();
    for _ in 0..2 {
        let root = TempDir::new().unwrap();
        let mut table = toml_edit::Table::new();
        table["source_path"] = toml_edit::value("doc/plans/IN_PROGRESS-PLAN-demo.md");
        stamp_new_run(&mut table, "doc/plans/IN_PROGRESS-PLAN-demo.md");
        let config = format!("[plan]\nplan_name = \"Demo\"\n{table}");
        let work_dir = work_dir(root.path(), &config, &[finished("a", 0, 60, 60)]);
        let Published::Written(path) = finish_plan(&work_dir).unwrap() else {
            panic!("a new run writes its own archive");
        };
        files.push(path);
    }
    assert_ne!(files[0], files[1]);
    assert_eq!(
        std::fs::read_dir(home.path().join("stats/plans"))
            .unwrap()
            .count(),
        2
    );
}

#[test]
fn the_archive_name_survives_a_plan_rename_and_a_restart() {
    let home = TempDir::new().unwrap();
    let _redirect = redirect_user_config(home.path().join("config.toml"));
    let root = TempDir::new().unwrap();
    let work_dir = finished_state(root.path());
    let Published::Written(first) = finish_plan(&work_dir).unwrap() else {
        panic!("first finish writes");
    };
    assert!(
        first.ends_with(format!("{PLAN_ID}.json")),
        "{}",
        first.display()
    );
    let renamed = PLAN_TABLE
        .replace("IN_PROGRESS-PLAN-demo", "DONE-PLAN-renamed")
        .replace("Demo plan", "Renamed plan");
    write_text(&work_dir.join("config.toml"), &renamed);
    assert_eq!(
        finish_plan(&work_dir).unwrap(),
        Published::AlreadyPublished(first)
    );
}

#[test]
fn a_retry_replaces_a_stale_current_copy_with_the_archive() {
    let home = TempDir::new().unwrap();
    let _redirect = redirect_user_config(home.path().join("config.toml"));
    let root = TempDir::new().unwrap();
    let work_dir = finished_state(root.path());
    let Published::Written(archive) = finish_plan(&work_dir).unwrap() else {
        panic!("first finish writes");
    };
    let mut stale = record_with("Stale");
    stale.plan_id = "old".to_string();
    write_current(&work_dir, &stale).unwrap();
    assert!(matches!(
        finish_plan(&work_dir).unwrap(),
        Published::AlreadyPublished(_)
    ));
    let current = read_current(&work_dir).expect("repaired");
    let archived = PlanStatsRecord::from_json(&std::fs::read(archive).unwrap()).unwrap();
    assert_eq!(current, archived);
    assert_eq!(current.plan_name, "Demo plan");
}

#[test]
fn concurrent_finishes_publish_one_archive() {
    let home = TempDir::new().unwrap();
    let _redirect = redirect_user_config(home.path().join("config.toml"));
    let root = TempDir::new().unwrap();
    let work_dir = finished_state(root.path());
    let dir = loom_home().unwrap();
    let results: Vec<Published> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let (dir, work_dir) = (dir.clone(), work_dir.clone());
                scope.spawn(move || {
                    // The redirect is per thread; each finisher archives under the same home.
                    let _redirect = redirect_user_config(dir.join("config.toml"));
                    finish_plan(&work_dir).unwrap()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let written = results
        .iter()
        .filter(|r| matches!(r, Published::Written(_)))
        .count();
    assert_eq!(written, 1, "{results:?}");
    assert_eq!(
        std::fs::read_dir(home.path().join("stats/plans"))
            .unwrap()
            .count(),
        1
    );
    assert!(read_current(&work_dir).is_some());
}

#[test]
fn an_incomplete_plan_archives_but_shows_no_overview() {
    let home = TempDir::new().unwrap();
    let _redirect = redirect_user_config(home.path().join("config.toml"));
    let root = TempDir::new().unwrap();
    let running = Stage {
        started_at: Some(at(0)),
        ..stage("c", StageStatus::Executing)
    };
    let work_dir = work_dir(
        root.path(),
        PLAN_TABLE,
        &[finished("a", 0, 60, 60), running],
    );
    assert!(matches!(
        finish_plan(&work_dir).unwrap(),
        Published::Written(_)
    ));
    assert_eq!(read_current(&work_dir), None, "a stage is still in play");
}

#[test]
fn an_all_skipped_plan_archives_with_no_duration() {
    let home = TempDir::new().unwrap();
    let _redirect = redirect_user_config(home.path().join("config.toml"));
    let root = TempDir::new().unwrap();
    let work_dir = work_dir(
        root.path(),
        PLAN_TABLE,
        &[
            stage("a", StageStatus::Skipped),
            stage("b", StageStatus::Skipped),
        ],
    );
    assert!(matches!(
        finish_plan(&work_dir).unwrap(),
        Published::Written(_)
    ));
    let record = read_current(&work_dir).expect("all stages are terminal");
    assert_eq!((record.wall_secs, record.execution_secs), (0, 0));
    assert_eq!(record.started_at, record.ended_at);
    assert_eq!(record.skipped_count(), 2);
    assert_eq!(record.completed_count(), 0);
}

#[test]
fn a_run_without_a_stats_identity_gets_one_that_a_retry_reuses() {
    let home = TempDir::new().unwrap();
    let _redirect = redirect_user_config(home.path().join("config.toml"));
    let root = TempDir::new().unwrap();
    let old_table =
        "[plan]\nsource_path = \"doc/plans/IN_PROGRESS-PLAN-old.md\"\nplan_name = \"Old\"\n";
    let work_dir = work_dir(root.path(), old_table, &[finished("a", 0, 60, 60)]);
    let Published::Written(first) = finish_plan(&work_dir).unwrap() else {
        panic!("first finish writes");
    };
    assert!(first.to_string_lossy().contains("-plan-old-"));
    std::fs::remove_file(work_dir.join(CURRENT_FILE)).unwrap();
    assert_eq!(
        finish_plan(&work_dir).unwrap(),
        Published::AlreadyPublished(first)
    );
}
