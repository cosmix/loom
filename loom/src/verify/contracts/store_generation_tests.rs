//! The re-freeze transaction: an injected failure at each step, readers during
//! replacement, legacy records and orphan collection.

use super::faults::{arm, Action, Point};
use super::*;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicBool, Ordering};
use tempfile::TempDir;

const POINTS: [Point; 3] = [
    Point::AfterCopy,
    Point::BeforeGenerationRename,
    Point::BeforeRecord,
];

type Files = Vec<(String, Vec<u8>)>;

fn files(tag: &str) -> Files {
    ["a.rs", "nested/b.rs"]
        .into_iter()
        .map(|path| (path.to_string(), format!("{tag} {path}").into_bytes()))
        .collect()
}

fn frozen(files: &Files) -> FreezeRecord {
    FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: "s1".to_string(),
        session_id: "session-1".to_string(),
        frozen_at: Utc::now(),
        base: "a".repeat(40),
        files: files
            .iter()
            .map(|(path, bytes)| FrozenFile {
                path: path.clone(),
                sha256: sha256_hex(bytes),
            })
            .collect(),
        contracts: Vec::new(),
    }
}

/// A stage frozen at `old`: a version 1 record, or a generation when `refrozen`.
fn committed(refrozen: bool) -> (TempDir, FreezeRecord, Files) {
    let tmp = TempDir::new().unwrap();
    let old = files("old");
    write_freeze(tmp.path(), &frozen(&old), &old).unwrap();
    if refrozen {
        refreeze_set(tmp.path(), "s1", &old, &[]).unwrap();
    }
    let record = load_freeze(tmp.path(), "s1").unwrap().unwrap();
    assert_eq!(record.version == GENERATION_RECORD_VERSION, refrozen);
    (tmp, record, old)
}

/// `record` names exactly `expected`, every copy on disk with those bytes.
fn assert_whole(work: &Path, record: &FreezeRecord, expected: &Files) {
    assert_eq!(record.files.len(), expected.len());
    for ((path, bytes), file) in expected.iter().zip(&record.files) {
        assert_eq!(&file.path, path);
        assert_eq!(file.sha256, sha256_hex(bytes));
        let copy = frozen_file_path_for_record(work, record, path);
        assert_eq!(&std::fs::read(copy).unwrap(), bytes, "{path}");
    }
}

fn staged_entries(work: &Path) -> usize {
    match std::fs::read_dir(stage_dir(work, "s1").join("staging")) {
        Ok(entries) => entries.count(),
        Err(_) => 0,
    }
}

#[test]
fn each_failed_step_keeps_the_committed_record_whole_and_retries_cleanly() {
    for refrozen in [false, true] {
        for point in POINTS {
            let (tmp, old_record, old) = committed(refrozen);
            let new = files("new");

            arm(point, Action::Fail);
            let error = refreeze_set(tmp.path(), "s1", &new, &[]).unwrap_err();
            assert!(error.to_string().contains("injected failure"), "{error:#}");

            let current = load_freeze(tmp.path(), "s1").unwrap().unwrap();
            assert_eq!(current, old_record, "{point:?}");
            assert_whole(tmp.path(), &current, &old);
            // Nothing is collected before the record names the replacement.
            let expected_staged = usize::from(point != Point::BeforeRecord);
            assert_eq!(staged_entries(tmp.path()), expected_staged, "{point:?}");

            refreeze_set(tmp.path(), "s1", &new, &[]).unwrap();
            let published = load_freeze(tmp.path(), "s1").unwrap().unwrap();
            assert_eq!(published.version, GENERATION_RECORD_VERSION);
            assert_whole(tmp.path(), &published, &new);
            assert_eq!(staged_entries(tmp.path()), 0, "{point:?}");
            // A reader still holding the old record keeps its whole generation.
            assert_whole(tmp.path(), &old_record, &old);
        }
    }
}

#[test]
fn readers_see_all_of_one_generation_during_replacement() {
    let (tmp, _, _) = committed(false);
    let work = tmp.path();
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| {
            let mut reads = 0_u32;
            while !done.load(Ordering::Acquire) || reads == 0 {
                let record = load_freeze(work, "s1").unwrap().unwrap();
                let contents: Vec<String> = record
                    .files
                    .iter()
                    .map(|file| {
                        let copy = frozen_file_path_for_record(work, &record, &file.path);
                        let bytes = std::fs::read(copy).unwrap();
                        assert_eq!(sha256_hex(&bytes), file.sha256);
                        String::from_utf8(bytes).unwrap()
                    })
                    .collect();
                let tag = contents[0].split(' ').next().unwrap().to_string();
                assert!(contents.iter().all(|text| text.starts_with(&tag)));
                reads += 1;
            }
        });
        for round in 0..25 {
            refreeze_set(work, "s1", &files(&format!("round-{round}")), &[]).unwrap();
        }
        done.store(true, Ordering::Release);
        reader.join().unwrap();
    });
}

#[test]
fn a_dropped_path_leaves_the_new_generation_but_not_the_old_one() {
    let (tmp, old_record, old) = committed(true);
    let kept = vec![("a.rs".to_string(), b"new a".to_vec())];

    let dropped = refreeze_set(tmp.path(), "s1", &kept, &[]).unwrap();

    assert_eq!(dropped, vec!["nested/b.rs".to_string()]);
    let published = load_freeze(tmp.path(), "s1").unwrap().unwrap();
    assert_whole(tmp.path(), &published, &kept);
    assert!(!frozen_file_path_for_record(tmp.path(), &published, "nested/b.rs").exists());
    assert_whole(tmp.path(), &old_record, &old);
}

#[test]
fn a_refreeze_back_to_a_committed_set_reuses_its_generation() {
    let (tmp, first, old) = committed(true);
    refreeze_set(tmp.path(), "s1", &files("new"), &[]).unwrap();
    refreeze_set(tmp.path(), "s1", &old, &[]).unwrap();
    let again = load_freeze(tmp.path(), "s1").unwrap().unwrap();
    assert_eq!(again.files, first.files);
    assert_whole(tmp.path(), &again, &old);
}

#[test]
fn a_tampered_generation_is_refused_and_the_record_stays() {
    let (tmp, old_record, old) = committed(false);
    let new = files("new");
    arm(Point::BeforeRecord, Action::Fail);
    refreeze_set(tmp.path(), "s1", &new, &[]).unwrap_err();
    let mut unpublished = old_record.clone();
    unpublished.version = GENERATION_RECORD_VERSION;
    unpublished.files = frozen(&new).files;
    let copy = frozen_file_path_for_record(tmp.path(), &unpublished, "a.rs");
    std::fs::write(copy, "tampered").unwrap();

    let error = refreeze_set(tmp.path(), "s1", &new, &[]).unwrap_err();

    assert!(format!("{error:#}").contains("hash mismatch"), "{error:#}");
    let current = load_freeze(tmp.path(), "s1").unwrap().unwrap();
    assert_eq!(current, old_record);
    assert_whole(tmp.path(), &current, &old);
}

#[test]
fn orphans_are_collected_after_publication_and_a_failure_to_collect_changes_nothing() {
    let (tmp, _, _) = committed(false);
    let orphan = stage_dir(tmp.path(), "s1").join("staging/orphan");
    std::fs::create_dir_all(&orphan).unwrap();
    std::fs::write(orphan.join("left-behind"), "partial").unwrap();
    arm(Point::BeforeRecord, Action::Fail);
    refreeze_set(tmp.path(), "s1", &files("failed"), &[]).unwrap_err();
    assert!(orphan.join("left-behind").exists());

    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } != 0 {
        std::fs::set_permissions(&orphan, std::fs::Permissions::from_mode(0o500)).unwrap();
        let locked_out = files("locked-out");
        let collected = refreeze_set(tmp.path(), "s1", &locked_out, &[]);
        std::fs::set_permissions(&orphan, std::fs::Permissions::from_mode(0o700)).unwrap();
        collected.unwrap();
        let published = load_freeze(tmp.path(), "s1").unwrap().unwrap();
        assert_whole(tmp.path(), &published, &locked_out);
    }

    refreeze_set(tmp.path(), "s1", &files("collected"), &[]).unwrap();
    assert_eq!(staged_entries(tmp.path()), 0);
}

/// Every directory the first freeze wrote into reaches the disk before the
/// record that says its copies exist.
#[test]
fn the_first_freeze_syncs_each_directory_it_wrote_into() {
    let tmp = TempDir::new().unwrap();
    let old = files("old");
    super::faults::take_synced();
    write_freeze(tmp.path(), &frozen(&old), &old).unwrap();
    let synced = super::faults::take_synced();
    for directory in ["", "files", "files/nested"] {
        assert!(
            synced.contains(&PathBuf::from(directory)),
            "'{directory}' was not synced: {synced:?}"
        );
    }
}
