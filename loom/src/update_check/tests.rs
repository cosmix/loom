use super::*;
use chrono::Duration;

#[test]
fn disabled_check_yields_no_notice_and_no_refresh() {
    let now = Utc::now();
    let current = Version::parse("0.2.0").unwrap();
    let state = UpdateState {
        last_checked: None,
        latest_version: Some("9.9.9".to_string()),
    };

    let action = decide(Some(&state), false, 24, now, &current);

    assert!(action.notice.is_none());
    assert!(!action.refresh);
}

#[test]
fn newer_latest_version_produces_one_notice_naming_both_versions() {
    let now = Utc::now();
    let current = Version::parse("0.2.0").unwrap();
    let state = UpdateState {
        last_checked: Some(now),
        latest_version: Some("0.3.0".to_string()),
    };

    let action = decide(Some(&state), true, 24, now, &current);

    let notice = action.notice.expect("a newer release should notice");
    assert!(notice.contains("0.2.0"), "{notice}");
    assert!(notice.contains("0.3.0"), "{notice}");
}

#[test]
fn dev_build_is_never_notified_and_never_refreshes() {
    let now = Utc::now();
    let current = Version::parse("0.2.1-dev.5+abc1234").unwrap();

    let newer = UpdateState {
        last_checked: Some(now),
        latest_version: Some("0.2.1".to_string()),
    };
    let action = decide(Some(&newer), true, 24, now, &current);
    assert!(
        action.notice.is_none(),
        "a dev build must never be notified, even of a newer release"
    );
    assert!(!action.refresh);

    let older = UpdateState {
        last_checked: Some(now),
        latest_version: Some("0.2.0".to_string()),
    };
    let action = decide(Some(&older), true, 24, now, &current);
    assert!(
        action.notice.is_none(),
        "a dev build must not be told it is behind an older release"
    );
    assert!(!action.refresh);

    let never_checked = UpdateState {
        last_checked: None,
        latest_version: Some("0.2.1".to_string()),
    };
    let action = decide(Some(&never_checked), true, 24, now, &current);
    assert!(action.notice.is_none());
    assert!(
        !action.refresh,
        "a dev build must not schedule a refresh even when last_checked is None"
    );

    let zero = Version::parse("0.0.0-dev+abc1234").unwrap();
    let ahead = UpdateState {
        last_checked: None,
        latest_version: Some("0.2.1".to_string()),
    };
    let action = decide(Some(&ahead), true, 24, now, &zero);
    assert!(action.notice.is_none());
    assert!(!action.refresh);
}

/// `notice_for` is the formatter `notify_and_maybe_refresh` prints straight
/// to stderr (never stdout — see the module doc). Covers the reachable part
/// of that print without capturing real process stderr or spawning anything.
#[test]
fn notice_for_names_both_versions_and_update_only_when_newer() {
    let current = Version::parse("0.2.0").unwrap();

    let newer = UpdateState {
        last_checked: None,
        latest_version: Some("0.3.0".to_string()),
    };
    let notice = notice_for(Some(&newer), &current).expect("a newer release should notice");
    assert!(notice.contains("0.2.0"), "{notice}");
    assert!(notice.contains("0.3.0"), "{notice}");
    assert!(notice.contains("loom update"), "{notice}");

    let older = UpdateState {
        last_checked: None,
        latest_version: Some("0.1.0".to_string()),
    };
    assert!(notice_for(Some(&older), &current).is_none());

    let equal = UpdateState {
        last_checked: None,
        latest_version: Some("0.2.0".to_string()),
    };
    assert!(notice_for(Some(&equal), &current).is_none());

    let unparseable = UpdateState {
        last_checked: None,
        latest_version: Some("not-a-version".to_string()),
    };
    assert!(notice_for(Some(&unparseable), &current).is_none());

    assert!(notice_for(None, &current).is_none());
}

#[test]
fn absent_or_corrupt_state_both_schedule_a_refresh_silently() {
    let dir = tempfile::tempdir().unwrap();
    let now = Utc::now();
    let current = Version::parse("0.2.0").unwrap();

    assert!(read_state(dir.path()).is_none(), "no state file yet");
    let action = decide(None, true, 24, now, &current);
    assert!(action.refresh);
    assert!(action.notice.is_none());

    std::fs::create_dir_all(dir.path()).unwrap();
    std::fs::write(state_path(dir.path()), "not valid json").unwrap();
    assert!(
        read_state(dir.path()).is_none(),
        "a torn state file must read as absent, not error"
    );
}

#[test]
fn refresh_only_fires_once_the_interval_has_elapsed() {
    let now = Utc::now();
    let current = Version::parse("0.2.0").unwrap();

    let fresh = UpdateState {
        last_checked: Some(now - Duration::hours(1)),
        latest_version: None,
    };
    assert!(!decide(Some(&fresh), true, 24, now, &current).refresh);

    let stale = UpdateState {
        last_checked: Some(now - Duration::hours(25)),
        latest_version: None,
    };
    assert!(decide(Some(&stale), true, 24, now, &current).refresh);

    let future = UpdateState {
        last_checked: Some(now + Duration::hours(10)),
        latest_version: None,
    };
    assert!(
        decide(Some(&future), true, 24, now, &current).refresh,
        "a future last_checked (clock skew) must schedule a refresh rather than \
         wedging the check forever: the writer always stamps its own `now`, so \
         one refresh self-heals the record"
    );
}

#[test]
fn worker_lock_excludes_other_workers_until_owner_exits() {
    let dir = tempfile::tempdir().unwrap();
    let lock = acquire_refresh_lock(dir.path()).unwrap();
    assert!(acquire_refresh_lock(dir.path()).is_none());
    drop(lock);
    assert!(acquire_refresh_lock(dir.path()).is_some());
}

#[test]
fn refresh_lock_survives_launcher_exit() {
    use std::process::{Command, Stdio};
    use std::time::{Duration as Wait, Instant};

    const MODE: &str = "LOOM_REFRESH_LOCK_FIXTURE_MODE";
    const DIR: &str = "LOOM_REFRESH_LOCK_FIXTURE_DIR";
    let spawn = |mode: &str, dir: &Path| {
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "update_check::tests::refresh_lock_survives_launcher_exit",
            ])
            .env(MODE, mode)
            .env(DIR, dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };
    if let Ok(mode) = std::env::var(MODE) {
        refresh_lock_fixture(
            &mode,
            &PathBuf::from(std::env::var_os(DIR).unwrap()),
            &spawn,
        );
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    assert!(spawn("launcher", dir.path()).wait().unwrap().success());
    let deadline = Instant::now() + Wait::from_secs(5);
    while !dir.path().join("ready").exists() && Instant::now() < deadline {
        std::thread::sleep(Wait::from_millis(10));
    }
    assert!(dir.path().join("ready").exists());
    let excluded = acquire_refresh_lock(dir.path()).is_none();
    std::fs::write(dir.path().join("release"), "release").unwrap();
    let deadline = Instant::now() + Wait::from_secs(5);
    while !dir.path().join("done").exists() && Instant::now() < deadline {
        std::thread::sleep(Wait::from_millis(10));
    }
    assert!(dir.path().join("done").exists());
    assert!(acquire_refresh_lock(dir.path()).is_some());
    assert!(
        excluded,
        "worker must retain the lock after its launcher exits"
    );
}

fn refresh_lock_fixture(
    mode: &str,
    dir: &Path,
    spawn: &impl Fn(&str, &Path) -> std::process::Child,
) {
    use std::time::{Duration as Wait, Instant};
    if mode == "launcher" {
        // The launcher exits while the worker still holds the lock. Reap it
        // if the launcher remains alive long enough for the worker to exit.
        let mut worker = spawn("worker", dir);
        std::thread::spawn(move || {
            let _ = worker.wait();
        });
        return;
    }
    let lock = acquire_refresh_lock(dir).unwrap();
    std::fs::write(dir.join("ready"), "ready").unwrap();
    let deadline = Instant::now() + Wait::from_secs(10);
    while !dir.join("release").exists() && Instant::now() < deadline {
        std::thread::sleep(Wait::from_millis(10));
    }
    drop(lock);
    std::fs::write(dir.join("done"), "done").unwrap();
}

#[test]
fn perform_refresh_stamps_the_attempt_even_when_the_fetch_fails() {
    let dir = tempfile::tempdir().unwrap();
    let now = Utc::now();

    let prior = UpdateState {
        last_checked: Some(now - Duration::hours(48)),
        latest_version: Some("0.1.0".to_string()),
    };
    std::fs::create_dir_all(dir.path()).unwrap();
    std::fs::write(
        state_path(dir.path()),
        serde_json::to_string(&prior).unwrap(),
    )
    .unwrap();

    perform_refresh(dir.path(), now, || {
        Err(anyhow::anyhow!("network unreachable"))
    });

    let state = read_state(dir.path()).expect("perform_refresh always writes a state file");
    assert_eq!(state.last_checked, Some(now));
    assert_eq!(
        state.latest_version.as_deref(),
        Some("0.1.0"),
        "a failed fetch must preserve the previous latest_version"
    );

    perform_refresh(dir.path(), now, || {
        Version::parse("0.4.0").map_err(Into::into)
    });

    let state = read_state(dir.path()).expect("perform_refresh always writes a state file");
    assert_eq!(state.latest_version.as_deref(), Some("0.4.0"));
}
