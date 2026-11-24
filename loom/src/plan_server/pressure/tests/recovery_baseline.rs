//! The configuration baseline on disk: saved before Codex starts, compared by the recovery of a
//! server that died while Codex ran, and removed when a run ends.

use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

use serial_test::serial;

use super::fake::Script;
use super::{wait_until, Fixture, PLAN};
use crate::plan_server::pressure::baseline;
use crate::plan_server::pressure::codex::CONFIG_CHANGED;
use crate::plan_server::pressure::config_guard::snapshot;
use crate::plan_server::pressure::recover::check_baseline;
use crate::plan_session::bridge::registry::ChildRecord;
use crate::plan_session::bridge::{RecoveredSession, RecoveryReport};
use crate::plan_session::kind::SessionKind;
use crate::process::sandbox_probe::skip_without_group_signals;
use crate::test_stand_in::StandIn;

const BASELINE: &str = "config-baseline.json";
const BLOCK: &str = "config-block.json";

fn interrupted_run() -> RecoveryReport {
    let session = RecoveredSession {
        id: "s-1".to_owned(),
        kind: SessionKind::Pressure { plan: PLAN.into() },
        children: Vec::new(),
    };
    RecoveryReport {
        torn_down: vec![session],
        ..RecoveryReport::default()
    }
}

#[test]
#[serial]
fn a_baseline_with_no_edit_leaves_no_block() {
    let f = Fixture::new();
    baseline::save(f.ctx.runtime(), &snapshot(&f.root).unwrap()).unwrap();
    let dir = f.ctx.runtime().dir.clone();
    assert!(dir.join(BASELINE).is_file());

    f.restart(interrupted_run());

    assert!(
        !dir.join(BLOCK).exists(),
        "an unchanged project was blocked"
    );
    assert!(!dir.join(BASELINE).exists(), "the baseline was kept");
}

#[test]
#[serial]
fn a_baseline_with_an_edit_blocks_and_is_removed() {
    let f = Fixture::new();
    baseline::save(f.ctx.runtime(), &snapshot(&f.root).unwrap()).unwrap();
    let settings = f.root.join(".claude/settings.local.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(&settings, "{}").unwrap();
    let dir = f.ctx.runtime().dir.clone();

    f.restart(interrupted_run());

    let block = fs::read_to_string(dir.join(BLOCK)).expect("the edit wrote no block");
    assert!(block.contains(CONFIG_CHANGED), "{block}");
    assert!(block.contains("settings.local.json"), "{block}");
    assert!(!dir.join(BASELINE).exists(), "the baseline was kept");
}

#[test]
#[serial]
fn an_unreadable_baseline_blocks() {
    let f = Fixture::new();
    let dir = f.ctx.runtime().dir.clone();
    fs::write(dir.join(BASELINE), "not json").unwrap();

    f.restart(interrupted_run());

    let block = fs::read_to_string(dir.join(BLOCK)).expect("an unreadable baseline wrote no block");
    assert!(block.contains("cannot be checked"), "{block}");
}

#[test]
#[serial]
fn a_finished_run_clears_its_baseline() {
    let f = Fixture::new();
    f.start(1);
    f.ended();
    assert!(
        !f.ctx.runtime().dir.join(BASELINE).exists(),
        "a finished run kept its baseline"
    );
}

#[test]
#[serial]
fn the_baseline_exists_while_codex_runs_and_goes_when_the_run_stops() {
    let false = skip_without_group_signals("the_baseline_exists_while_codex_runs") else {
        return;
    };
    let f = Fixture::new();
    let baseline = f.ctx.runtime().dir.join(BASELINE);
    f.codex("sleep", "30");
    f.fake.script(vec![Script::Hold]);
    f.start(1);
    wait_until("the baseline while Codex runs", || baseline.is_file());
    let (status, _) = f.call("/api/pressure/stop", &f.holder, None);
    assert_eq!(status, 202);
    f.finished();
    assert!(!baseline.exists(), "a stopped run kept its baseline");
}

#[test]
#[serial]
fn settling_acts_but_keeps_a_baseline_another_run_saved_even_when_the_snapshot_is_identical() {
    let f = Fixture::new();
    let (runtime, before) = (f.ctx.runtime(), snapshot(&f.root).unwrap());
    baseline::save(runtime, &before).unwrap();
    let seen = baseline::load(runtime).unwrap().unwrap();
    baseline::save(runtime, &before).unwrap();

    let mut acted = false;
    let settled = baseline::settle(runtime, Some(&seen), || {
        acted = true;
        Ok(())
    })
    .unwrap();

    assert_eq!(settled, baseline::Settled::Kept);
    assert!(acted, "the action was skipped for a replaced baseline");
    assert!(f.ctx.runtime().dir.join(BASELINE).is_file());
}

/// A pressure run starts while the check still waits for the recovered Codex to exit: the edit
/// the interrupted run left is still blocked, and the new run's baseline is kept.
#[test]
#[serial]
fn a_baseline_replaced_during_the_check_still_blocks_and_is_kept() {
    let f = Fixture::new();
    let runtime = f.ctx.runtime().clone();
    baseline::save(&runtime, &snapshot(&f.root).unwrap()).unwrap();
    let settings = f.root.join(".claude/settings.local.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(&settings, "{}").unwrap();

    let mut child = StandIn::exits_alone(Command::new("sleep").arg("1"));
    let mut report = interrupted_run();
    report.torn_down[0].children.push(ChildRecord {
        label: "codex".to_owned(),
        pid: child.id(),
        start_time: None,
    });
    let reaper = thread::spawn(move || child.wait_within(Duration::from_secs(10)));
    let (root, later) = (f.root.clone(), runtime.clone());
    let new_run = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        baseline::save(&later, &snapshot(&root).unwrap()).unwrap();
    });

    let line = check_baseline(&f.root, &runtime, &report, Duration::from_secs(10));

    new_run.join().unwrap();
    assert!(reaper.join().unwrap().success());
    let line = line.expect("the interrupted run's edit was not reported");
    assert!(line.contains(CONFIG_CHANGED), "{line}");
    let block = fs::read_to_string(runtime.dir.join(BLOCK)).expect("the edit wrote no block");
    assert!(block.contains("settings.local.json"), "{block}");
    assert!(
        runtime.dir.join(BASELINE).is_file(),
        "the new run's baseline was removed"
    );
}
