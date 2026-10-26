//! Tests for planning runtime paths.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;

use serial_test::serial;
use tempfile::TempDir;

use crate::planning::runtime::{
    project_key, runtime_paths_from, runtime_paths_under, server_runtime_paths,
};

const SESSION: &str = "123e4567-e89b-12d3-a456-426614174000";

fn tmp() -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().unwrap();
    let root = temp.path().canonicalize().unwrap();
    (temp, root)
}

#[test]
fn key_is_sanitised_name_and_twelve_hex() {
    let (_temp, r) = tmp();
    let proj = r.join("my proj");
    fs::create_dir_all(&proj).unwrap();
    let key = project_key(&proj).unwrap();
    let hash = key
        .strip_prefix("my_proj-")
        .expect("space becomes underscore");
    assert_eq!(hash.len(), 12);
    assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()));
}

#[test]
fn same_name_in_two_places_gives_two_keys() {
    let (_temp, r) = tmp();
    let (one, two) = (r.join("a/proj"), r.join("b/proj"));
    fs::create_dir_all(&one).unwrap();
    fs::create_dir_all(&two).unwrap();
    assert_ne!(project_key(&one).unwrap(), project_key(&two).unwrap());
}

#[test]
fn paths_sit_under_the_project_directory() {
    let (_temp, r) = tmp();
    let (loom_dir, proj) = (r.join("loomdir"), r.join("proj"));
    fs::create_dir_all(&proj).unwrap();
    let paths = runtime_paths_under(&loom_dir, &proj).unwrap();
    assert_eq!(paths.planning_root, loom_dir.join("plan"));
    assert_eq!(
        paths.dir,
        loom_dir.join("plan").join(project_key(&proj).unwrap())
    );
    assert_eq!(paths.lock, paths.dir.join("lock"));
    assert_eq!(paths.drafts, paths.dir.join("drafts"));
    assert_eq!(paths.claims, paths.dir.join("claims"));
    assert_eq!(paths.summaries_lock, paths.dir.join("summaries.lock"));
    assert_eq!(paths.summary_cost, paths.dir.join("summary-cost.jsonl"));
    assert_eq!(paths.sessions, paths.dir.join("sessions"));
    assert!(!loom_dir.exists(), "path computation creates nothing");
}

fn set_mode(dirs: &[&Path], mode: u32) {
    for dir in dirs {
        fs::set_permissions(dir, fs::Permissions::from_mode(mode)).unwrap();
    }
}

#[test]
fn session_state_acceptance_and_refusals() {
    let (_temp, r) = tmp();
    let (root, loom_dir) = (r.join("proj"), r.join("loomdir"));
    fs::create_dir_all(&root).unwrap();
    let (key, other) = (project_key(&root).unwrap(), project_key(&r).unwrap());
    for dir in [
        format!("ro/{key}"),
        format!("ro/{other}"),
        format!("rw/{key}"),
        "lnk".into(),
    ] {
        fs::create_dir_all(r.join(dir)).unwrap();
    }
    let good = r.join("ro").join(&key);
    symlink(&good, r.join("lnk").join(&key)).unwrap();
    let guarded = [r.join("ro"), r.join("lnk")];
    let guarded: Vec<&Path> = guarded.iter().map(|p| p.as_path()).collect();
    set_mode(&guarded, 0o555);
    let resolve = |state: &Path, sid| runtime_paths_from(Some(state), sid, &loom_dir, &root);
    let sid = Some(SESSION);
    let accepted = resolve(&good, sid);
    let refused = [
        ("writable parent", resolve(&r.join("rw").join(&key), sid)),
        ("missing session", resolve(&good, None)),
        ("short session", resolve(&good, Some("abc"))),
        ("non-hex session", resolve(&good, Some("zzzzzzzzzzzz"))),
        ("relative", resolve(Path::new(&key), sid)),
        ("symlink", resolve(&r.join("lnk").join(&key), sid)),
        ("wrong key", resolve(&r.join("ro").join(&other), sid)),
    ];
    set_mode(&guarded, 0o755);
    let accepted = accepted.unwrap();
    assert_eq!(accepted.dir, good);
    assert_eq!(accepted.planning_root, r.join("ro"));
    assert_eq!(accepted.lock, good.join("lock"));
    for (case, result) in &refused {
        let err = format!("{:#}", result.as_ref().expect_err(case));
        assert!(err.contains("LOOM_PLAN_STATE"), "{case}: {err}");
    }
}

#[test]
#[serial]
fn server_ignores_the_state_variable() {
    let (_temp, r) = tmp();
    let (root, home) = (r.join("proj"), r.join("home"));
    fs::create_dir_all(&root).unwrap();
    let key = project_key(&root).unwrap();
    let state = r.join("elsewhere").join(&key);
    let (old_home, old_state) = (
        std::env::var_os("LOOM_HOME"),
        std::env::var_os("LOOM_PLAN_STATE"),
    );
    std::env::set_var("LOOM_HOME", &home);
    std::env::set_var("LOOM_PLAN_STATE", &state);
    let resolved = server_runtime_paths(&root);
    for (name, old) in [("LOOM_HOME", old_home), ("LOOM_PLAN_STATE", old_state)] {
        match old {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
    let (paths, ignored) = resolved.unwrap();
    assert_eq!(paths.dir, home.join("plan").join(&key));
    assert_eq!(ignored, Some(state.into_os_string()));
}
