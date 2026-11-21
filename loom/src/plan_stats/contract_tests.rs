//! Frozen contracts for the plan statistics archive.
//!
//! The work-dir fixture holds the `[plan]` table `loom init` writes, one
//! completed and one skipped stage, and no token checkpoint (a run that
//! tracked no usage). The table carries no stats run identity, so
//! `finish_plan` assigns it once and reuses it on the retry. The current copy
//! is `<work_dir>/plan-stats.json`; the archive is
//! `<loom home>/stats/plans/<file_name>`.

use std::path::{Path, PathBuf};
use std::sync::Barrier;

use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::Value;
use tempfile::TempDir;

use super::{file_name, finish_plan, loom_home, publish, read_current, PlanStatsRecord, Published};
use crate::models::stage::{Stage, StageStatus};
use crate::tokens::TokenUsage;
use crate::user_config::redirect_user_config;
use crate::verify::transitions::{create_stage, update_stage};

const PLAN_ID: &str = "20261121T120000Z-plan-contract-demo-0b8f5d2e-4c1a-4a57-9f6e-2d7c1b3a9e10";

const PLAN_TABLE: &str = "[plan]\n\
source_path = \"doc/plans/IN_PROGRESS-PLAN-contract-demo.md\"\n\
plan_id = \"contract-demo\"\n\
plan_name = \"Contract demo\"\n\
base_branch = \"main\"\n";

fn noon() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 21, 12, 0, 0).unwrap()
}

fn record(plan_name: &str, execution_secs: i64) -> PlanStatsRecord {
    PlanStatsRecord {
        schema_version: 1,
        plan_id: PLAN_ID.to_string(),
        plan_name: plan_name.to_string(),
        started_at: noon(),
        ended_at: noon() + Duration::seconds(600),
        wall_secs: 600,
        execution_secs,
        totals: TokenUsage {
            input: 10,
            cache_write: 20,
            cache_read: 30,
            output: 40,
        },
        stages: Vec::new(),
        models: Vec::new(),
    }
}

fn work_dir_fixture(root: &Path) -> PathBuf {
    let work_dir = root.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::write(work_dir.join("config.toml"), PLAN_TABLE).unwrap();
    let completed = Stage {
        id: "alpha".to_string(),
        name: "Alpha".to_string(),
        status: StageStatus::Completed,
        started_at: Some(noon()),
        completed_at: Some(noon() + Duration::seconds(300)),
        execution_secs: Some(240),
        ..Stage::default()
    };
    let skipped = Stage {
        id: "beta".to_string(),
        name: "Beta".to_string(),
        status: StageStatus::Skipped,
        ..Stage::default()
    };
    create_stage(&completed, &work_dir).unwrap();
    create_stage(&skipped, &work_dir).unwrap();
    work_dir
}

fn plans_dir(home: &Path) -> PathBuf {
    home.join("stats").join("plans")
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
}

/// The archive directory's entries, canonicalized.
fn listed(home: &Path) -> Vec<PathBuf> {
    entries(&plans_dir(home))
        .iter()
        .map(|path| canon(path))
        .collect()
}

fn canon(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap()
}

fn json_file(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn json(record: &PlanStatsRecord) -> Value {
    serde_json::to_value(record).unwrap()
}

/// Both records publish at once; each result is the path and whether that
/// publisher wrote it.
fn race(home: &Path, records: [&PlanStatsRecord; 2]) -> Vec<(PathBuf, bool)> {
    let barrier = Barrier::new(2);
    std::thread::scope(|scope| {
        let handles: Vec<_> = records
            .into_iter()
            .map(|record| {
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    publish(record, home).expect("publish")
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| match handle.join().unwrap() {
                Published::Written(path) => (path, true),
                Published::AlreadyPublished(path) => (path, false),
            })
            .collect()
    })
}

fn finish_repairs_the_current_copy_from_the_archive() {
    let home = TempDir::new().unwrap();
    let _redirect = redirect_user_config(home.path().join("config.toml"));
    let root = TempDir::new().unwrap();
    let work_dir = work_dir_fixture(root.path());
    let Published::Written(archive) = finish_plan(&work_dir).expect("first finish") else {
        panic!("the first finish of a run must write its archive");
    };
    let archived = std::fs::read(&archive).unwrap();
    update_stage("alpha", &work_dir, |stage| {
        stage.name = "Alpha renamed".to_string();
        stage.execution_secs = Some(9_999);
        Ok(())
    })
    .unwrap();
    let current = work_dir.join("plan-stats.json");
    assert!(current.is_file(), "finish exposes the current copy");
    std::fs::remove_file(&current).unwrap();

    match finish_plan(&work_dir).expect("retried finish") {
        Published::AlreadyPublished(path) => assert_eq!(canon(&path), canon(&archive)),
        Published::Written(path) => panic!("a retried finish wrote {}", path.display()),
    }
    assert_eq!(
        std::fs::read(&archive).unwrap(),
        archived,
        "archive rewritten"
    );
    let repaired = read_current(&work_dir).expect("the retry repairs the current copy");
    let first: Value = serde_json::from_slice(&archived).unwrap();
    assert_eq!(
        json(&repaired),
        first,
        "current copy differs from the archive"
    );
    assert_eq!(
        listed(home.path()),
        vec![canon(&archive)],
        "duplicate archive"
    );
}

#[test]
fn a_second_publish_keeps_the_first_record() {
    let first = record("First publisher", 120);
    let second = record("Second publisher", 999);
    assert_eq!(file_name(&first), file_name(&second), "same run, same file");
    for _ in 0..16 {
        let home = TempDir::new().unwrap();
        let outcomes = race(home.path(), [&first, &second]);
        let target = canon(&plans_dir(home.path())).join(file_name(&first));
        for (path, _) in &outcomes {
            assert_eq!(canon(path), target, "both publishers name the one archive");
        }
        let winners: Vec<usize> = (0..2).filter(|&index| outcomes[index].1).collect();
        assert_eq!(winners.len(), 1, "exactly one publisher writes the archive");
        let winner = if winners[0] == 0 { &first } else { &second };
        assert_eq!(
            json_file(&target),
            json(winner),
            "archive is not the winner's record"
        );
        assert_eq!(
            listed(home.path()),
            vec![target],
            "a duplicate or temporary file remains"
        );
    }
    finish_repairs_the_current_copy_from_the_archive();
}

fn refuses_linked_directory(record: &PlanStatsRecord, linked: &str) {
    let home = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let stats = home.path().join("stats");
    if linked == "stats" {
        std::os::unix::fs::symlink(outside.path(), &stats).unwrap();
    } else {
        std::fs::create_dir(&stats).unwrap();
        std::os::unix::fs::symlink(outside.path(), stats.join("plans")).unwrap();
    }
    assert!(
        publish(record, home.path()).is_err(),
        "publish followed a linked {linked} directory"
    );
    assert!(
        entries(outside.path()).is_empty(),
        "publish wrote through the linked {linked} directory"
    );
}

#[test]
fn the_loom_home_follows_the_user_config_location() {
    let home = TempDir::new().unwrap();
    let _redirect = redirect_user_config(home.path().join("config.toml"));
    assert_eq!(canon(&loom_home().expect("loom home")), canon(home.path()));
    let root = TempDir::new().unwrap();
    let work_dir = work_dir_fixture(root.path());

    let Published::Written(path) = finish_plan(&work_dir).expect("finish") else {
        panic!("the first finish of a run must write its archive");
    };
    let saved = read_current(&work_dir).expect("finish exposes the current copy");
    let expected = canon(&plans_dir(home.path())).join(file_name(&saved));
    assert_eq!(
        canon(&path),
        expected,
        "archive outside the configured home"
    );
    assert_eq!(json_file(&path), json(&saved));

    refuses_linked_directory(&saved, "stats");
    refuses_linked_directory(&saved, "plans");
}
