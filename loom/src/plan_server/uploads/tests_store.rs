//! The upload store: content addressing, permissions, the cap, `touch` and the expiry sweep.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use sha2::{Digest, Sha256};
use tempfile::TempDir;

use super::store::{put, put_within, sweep, touch, touch_at, StoreError, Swept, KEEP_SECONDS};
use crate::plan_session::attachments::stored::read_sidecar;

const PNG: [u8; 12] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4];

fn id_of(body: &[u8]) -> String {
    hex::encode(Sha256::digest(body))[..16].to_owned()
}

fn runtime() -> TempDir {
    TempDir::new().unwrap()
}

fn store_dir(runtime: &TempDir) -> std::path::PathBuf {
    runtime.path().join("attachments")
}

fn stored(result: Result<crate::plan_session::attachments::FileMeta, StoreError>) -> String {
    match result {
        Ok(meta) => meta.id,
        Err(StoreError::Full) => panic!("the store is full"),
        Err(StoreError::Io(error)) => panic!("{error:#}"),
    }
}

fn stamp(runtime: &TempDir, id: &str) -> u64 {
    read_sidecar(&store_dir(runtime), id)
        .expect("a sidecar")
        .stored_at
}

#[test]
fn a_file_is_stored_by_content_with_private_modes_and_a_sidecar_without_a_path() {
    let rt = runtime();
    let id = stored(put(rt.path(), "sketch.png", "image/png", &PNG, 100));
    assert_eq!(id, id_of(&PNG));
    let dir = store_dir(&rt);
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&dir.join(format!("{id}.png"))), 0o600);
    assert_eq!(mode(&dir.join(format!("{id}.json"))), 0o600);
    assert_eq!(fs::read(dir.join(format!("{id}.png"))).unwrap(), PNG);
    let raw = fs::read_to_string(dir.join(format!("{id}.json"))).unwrap();
    let root = rt.path().to_str().unwrap();
    assert!(
        !raw.contains(root) && !raw.contains("path"),
        "a sidecar holds no path: {raw}"
    );
    let names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 2, "no temp file is left: {names:?}");
}

#[test]
fn a_re_upload_reuses_the_file_and_refreshes_the_sidecar() {
    let rt = runtime();
    let id = stored(put(rt.path(), "first.png", "image/png", &PNG, 100));
    let data = store_dir(&rt).join(format!("{id}.png"));
    let before = fs::metadata(&data).unwrap().modified().unwrap();
    let again = stored(put(rt.path(), "second.png", "image/png", &PNG, 900));
    assert_eq!(again, id);
    assert_eq!(fs::metadata(&data).unwrap().modified().unwrap(), before);
    let sidecar = read_sidecar(&store_dir(&rt), &id).unwrap();
    assert_eq!(
        (sidecar.name.as_str(), sidecar.stored_at),
        ("second.png", 900)
    );
}

#[test]
fn the_extension_comes_from_the_media_type_never_from_the_name() {
    let rt = runtime();
    let id = stored(put(rt.path(), "run.sh", "text/plain", b"echo hi\n", 1));
    assert!(store_dir(&rt).join(format!("{id}.txt")).is_file());
    let id = stored(put(
        rt.path(),
        "../../escape.png",
        "text/markdown",
        b"# t\n",
        1,
    ));
    assert!(store_dir(&rt).join(format!("{id}.md")).is_file());
    assert!(!rt.path().join("escape.png").exists());
}

#[test]
fn an_upload_past_the_cap_is_refused_but_a_stored_file_is_reused_at_the_cap() {
    let rt = runtime();
    let first = [PNG.as_slice(), &[1u8; 100]].concat();
    let second = [PNG.as_slice(), &[2u8; 100]].concat();
    let cap = first.len() as u64 + 150;
    stored(put_within(rt.path(), "a.png", "image/png", &first, 1, cap));
    // The first file's sidecar counts against the cap too, so the second file does not fit.
    let refused = put_within(rt.path(), "b.png", "image/png", &second, 1, cap);
    assert!(matches!(refused, Err(StoreError::Full)));
    assert!(!store_dir(&rt)
        .join(format!("{}.png", id_of(&second)))
        .exists());
    let reused = put_within(
        rt.path(),
        "a.png",
        "image/png",
        &first,
        2,
        first.len() as u64,
    );
    assert!(reused.is_ok(), "an existing file needs no room");
}

#[test]
fn touch_refreshes_a_present_id_and_reports_a_missing_or_malformed_one() {
    let rt = runtime();
    let id = stored(put(rt.path(), "a.png", "image/png", &PNG, 100));
    assert!(touch_at(rt.path(), &id, 5000).unwrap());
    assert_eq!(stamp(&rt, &id), 5000);
    assert!(touch(rt.path(), &id).unwrap());
    assert!(stamp(&rt, &id) > 5000, "touch stamps the current time");
    for missing in [
        "ffffffffffffffff",
        "FFFFFFFFFFFFFFFF",
        "../x",
        "",
        &id[..15],
    ] {
        assert!(!touch(rt.path(), missing).unwrap(), "{missing:?}");
    }
    fs::remove_file(store_dir(&rt).join(format!("{id}.png"))).unwrap();
    assert!(
        !touch(rt.path(), &id).unwrap(),
        "a sidecar alone is not a stored file"
    );
}

#[test]
fn touch_on_a_runtime_with_no_store_is_false() {
    let rt = runtime();
    assert!(!touch(rt.path(), "0123456789abcdef").unwrap());
}

#[test]
fn the_sweep_removes_files_older_than_thirty_days_by_a_fixed_clock() {
    let rt = runtime();
    let old = stored(put(rt.path(), "old.png", "image/png", &PNG, 1000));
    let fresh_body = [PNG.as_slice(), &[9u8; 4]].concat();
    let fresh = stored(put(
        rt.path(),
        "new.png",
        "image/png",
        &fresh_body,
        1000 + KEEP_SECONDS,
    ));
    let dir = store_dir(&rt);
    let now = 1000 + KEEP_SECONDS + 1;
    assert_eq!(
        sweep(&dir, now - 1),
        Swept::default(),
        "not older than 30 days yet"
    );
    assert_eq!(
        sweep(&dir, now),
        Swept {
            removed: 2,
            failed: 0
        }
    );
    assert!(!dir.join(format!("{old}.png")).exists());
    assert!(!dir.join(format!("{old}.json")).exists());
    assert!(dir.join(format!("{fresh}.png")).is_file());
    assert!(dir.join(format!("{fresh}.json")).is_file());
}

#[test]
fn a_touch_keeps_a_file_through_the_sweep() {
    let rt = runtime();
    let id = stored(put(rt.path(), "a.png", "image/png", &PNG, 1000));
    let later = 1000 + KEEP_SECONDS;
    assert!(touch_at(rt.path(), &id, later).unwrap());
    assert_eq!(sweep(&store_dir(&rt), later + 5), Swept::default());
    assert!(store_dir(&rt).join(format!("{id}.png")).is_file());
}

#[test]
fn the_sweep_ages_a_file_with_no_sidecar_by_mtime_and_ignores_strangers() {
    let rt = runtime();
    let dir = store_dir(&rt);
    fs::create_dir_all(&dir).unwrap();
    let orphan = dir.join("0123456789abcdef.png");
    fs::write(&orphan, PNG).unwrap();
    fs::write(dir.join("notes.txt"), "mine").unwrap();
    // The orphan was just written, so by the real clock it is fresh.
    assert_eq!(sweep(&dir, super::store::now()), Swept::default());
    let far = super::store::now() + KEEP_SECONDS + 10;
    assert_eq!(
        sweep(&dir, far),
        Swept {
            removed: 1,
            failed: 0
        }
    );
    assert!(!orphan.exists());
    assert!(
        dir.join("notes.txt").exists(),
        "a file the store did not write is left alone"
    );
}

#[test]
fn the_sweep_of_a_missing_directory_is_empty() {
    let rt = runtime();
    assert_eq!(sweep(&store_dir(&rt), u64::MAX), Swept::default());
}
