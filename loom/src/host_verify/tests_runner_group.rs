//! Criterion group retirement after command exit and through real confinement.

use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use chrono::Utc;

use super::runner::recover_after_restart;
use super::store::{job_dir, save_current, start_guarded_group};
#[cfg(target_os = "linux")]
use super::tests_runner::queue;
use super::tests_runner::{guarded_criterion, state_root, Criterion, STAGE};
use super::tests_runner_recovery::running_record;
#[cfg(target_os = "linux")]
use crate::models::stage::CommandConfinement;
use crate::verify::criteria::GroupAnchor;
#[cfg(target_os = "linux")]
use crate::verify::criteria::{set_spawn_observer, spawn_confined, CommandSpec, SpawnObserver};

#[test]
fn recovery_stops_descendants_after_the_criterion_command_exits() {
    let (_dir, work) = state_root();
    let record = running_record(STAGE, None);
    save_current(&work, &record).unwrap();
    let (leader, address) = start_guarded_group(&work, STAGE, &record.id).unwrap();
    let anchor = GroupAnchor::new(leader, Box::new(move || address.signal()));
    let pid_file = work.join("descendant.pid");
    let status = Command::new("/bin/sh")
        .args(["-c", "sleep 600 & echo $! > \"$1\"", "sh"])
        .arg(&pid_file)
        .process_group(i32::try_from(anchor.pgid()).unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let pid: u32 = std::fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(crate::process::is_process_alive(pid));

    recover_after_restart(&work).unwrap();

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if !crate::process::is_process_alive(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("criterion descendant survived restart recovery");
}

#[test]
fn guardian_retires_its_group_when_the_host_child_dies() {
    let (_dir, work) = state_root();
    let record = running_record(STAGE, None);
    save_current(&work, &record).unwrap();
    let (leader, address) = start_guarded_group(&work, STAGE, &record.id).unwrap();
    let mut guardian = Criterion(leader);
    let child = Command::new("sleep")
        .arg("600")
        .process_group(i32::try_from(guardian.0.id()).unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut criterion = Criterion(child);

    drop(address);

    assert!(
        criterion.killed(),
        "criterion survived host-child descriptor close"
    );
    assert!(
        guardian.killed(),
        "group guardian survived host-child descriptor close"
    );
}

#[test]
fn recovery_refuses_a_guardian_whose_recorded_start_changed() {
    let (_dir, work) = state_root();
    let record = running_record(STAGE, None);
    save_current(&work, &record).unwrap();
    let (_anchor, mut criterion) = guarded_criterion(&work, &record);
    let groups = job_dir(&work, STAGE, &record.id).unwrap().join("groups");
    let line = std::fs::read_to_string(&groups).unwrap();
    let mut fields: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
    fields[1] = fields[1]
        .parse::<u64>()
        .unwrap()
        .wrapping_add(1)
        .to_string();
    std::fs::write(&groups, format!("{}\n", fields.join(" "))).unwrap();

    recover_after_restart(&work).unwrap();

    assert!(
        criterion.0.try_wait().unwrap().is_none(),
        "recovery signalled a stale guardian"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn guarded_retirement_stops_a_bubblewrap_session() {
    const CHILD_ENV: &str = "LOOM_GROUP_CONFINEMENT_TEST_CHILD";
    if std::env::var_os(CHILD_ENV).is_none() {
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "host_verify::tests_runner_group::guarded_retirement_stops_a_bubblewrap_session",
                "--nocapture",
            ])
            .env(CHILD_ENV, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated confinement test failed");
        return;
    }
    if crate::process::sandbox_probe::skip_unless(
        crate::process::sandbox_probe::confined_commands_runnable(),
        "host_verify::tests_runner_group::guarded_retirement_stops_a_bubblewrap_session",
        "bubblewrap confinement is unavailable",
    ) {
        return;
    }
    run_confined_heartbeat();
}

#[cfg(target_os = "linux")]
fn run_confined_heartbeat() {
    let (_dir, work) = state_root();
    let record = queue(&work, STAGE, Utc::now());
    let group_work = work.clone();
    let job_id = record.id.clone();
    let observer = set_spawn_observer(SpawnObserver(Box::new(move || {
        let (leader, address) = start_guarded_group(&group_work, STAGE, &job_id)?;
        Ok(GroupAnchor::new(leader, Box::new(move || address.signal())))
    })));
    let heartbeat = work.join("confined-heartbeat");
    let child = spawn_confined(
        &CommandSpec::shell(
            "(while :; do echo x >> confined-heartbeat; sleep 0.1; done) & sleep 600",
        ),
        Some(&work),
        CommandConfinement::Confined,
    )
    .unwrap();
    drop(observer);
    let start = Instant::now();
    while !heartbeat.exists() && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(heartbeat.exists(), "confined descendant did not start");
    let first = std::fs::metadata(&heartbeat).unwrap().len();
    std::thread::sleep(Duration::from_millis(250));
    assert!(
        std::fs::metadata(&heartbeat).unwrap().len() > first,
        "confined descendant did not remain active"
    );

    drop(child);

    let stopped = std::fs::metadata(&heartbeat).unwrap().len();
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(
        std::fs::metadata(&heartbeat).unwrap().len(),
        stopped,
        "bubblewrap descendant survived guarded retirement"
    );
}
