//! A running criterion's log in the job directory: written as output arrives,
//! capped, and replaced by the captured output when the criterion ends.

use std::io::Write;
use std::time::Duration;

use super::child_progress::{acceptance_label, Progress};
use super::store::{job_dir, open_live_log, save_criterion_log, save_current};
use super::tests_runner::{state_root, STAGE};
use super::tests_runner_recovery::running_record;
use crate::fs::safe_fs::MAX_LOG_BYTES;
use crate::verify::criteria::CriterionResult;

#[test]
fn a_running_criterions_output_is_in_its_log_until_the_result_replaces_it() {
    let (_dir, work) = state_root();
    let record = running_record(STAGE, None);
    save_current(&work, &record).unwrap();
    let progress = Progress::new(&work, STAGE, &record.id);
    let live = progress.live_output(acceptance_label);
    let mut writer = (live.0)(2).expect("the live log opens");
    writer
        .write_all(b"test slow has been running for over 60 seconds\n")
        .unwrap();
    let log = job_dir(&work, STAGE, &record.id)
        .unwrap()
        .join("acceptance-2.log");
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        "test slow has been running for over 60 seconds\n"
    );

    let result = CriterionResult::new(
        "cargo test".into(),
        false,
        "captured out\n".into(),
        "captured err\n".into(),
        None,
        Duration::from_secs(1),
        true,
    );
    progress.acceptance(2, &result, &[]);
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        "captured out\ncaptured err\n"
    );
    assert_eq!(
        progress.take_outcomes()[0].log.as_deref(),
        Some("acceptance-2.log")
    );
}

#[test]
fn a_live_log_keeps_its_head_and_latest_output_and_refuses_a_bad_label() {
    let (_dir, work) = state_root();
    let record = running_record(STAGE, None);
    save_current(&work, &record).unwrap();
    assert!(open_live_log(&work, STAGE, &record.id, "../escape").is_err());
    assert!(open_live_log(&work, STAGE, &record.id, "child").is_err());
    let mut log = open_live_log(&work, STAGE, &record.id, "base:0").unwrap();
    let path = job_dir(&work, STAGE, &record.id)
        .unwrap()
        .join("base:0.log");
    let mut output = Vec::new();
    for round in 0..9u8 {
        let chunk = vec![b'a' + round; 1024 * 1024];
        log.write_all(&chunk).unwrap();
        output.extend_from_slice(&chunk);
        let written = std::fs::read(&path).unwrap();
        assert!(written.len() <= MAX_LOG_BYTES, "round {round}");
        assert!(
            output.ends_with(&written[written.len() - 1024..]),
            "round {round}"
        );
    }
    log.write_all(b"the last line\n").unwrap();
    let written = std::fs::read(&path).unwrap();
    let head = MAX_LOG_BYTES / 4;
    assert_eq!(&written[..head], &output[..head]);
    let line = b"\n[truncated at 4 MiB]\n";
    assert_eq!(&written[head..head + line.len()], line);
    assert!(written.ends_with(b"iiiithe last line\n"));
    assert!(written.len() <= MAX_LOG_BYTES);

    // The criterion's end still replaces it with the head of the capture.
    output.extend_from_slice(b"the last line\n");
    save_criterion_log(&work, STAGE, &record.id, "base:0", &output).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert_eq!(saved.len(), MAX_LOG_BYTES);
    assert_eq!(
        &saved[..MAX_LOG_BYTES - line.len()],
        &output[..MAX_LOG_BYTES - line.len()]
    );
    assert!(saved.ends_with(line));
}

/// A name planted in the job directory as a hard link or a symlink to
/// another file is replaced: the file it named is never written.
#[test]
fn a_live_log_never_writes_through_a_planted_entry() {
    let (_dir, work) = state_root();
    let record = running_record(STAGE, None);
    save_current(&work, &record).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let dir = job_dir(&work, STAGE, &record.id).unwrap();
    type Plant = fn(&std::path::Path, &std::path::Path) -> std::io::Result<()>;
    let plants: [(&str, Plant); 2] = [
        ("acceptance-0", |target, link| {
            std::fs::hard_link(target, link)
        }),
        ("acceptance-1", |target, link| {
            std::os::unix::fs::symlink(target, link)
        }),
    ];
    for (label, plant) in plants {
        let operator = outside.path().join(format!("{label}.operator"));
        std::fs::write(&operator, b"operator data").unwrap();
        let path = dir.join(format!("{label}.log"));
        plant(&operator, &path).unwrap();

        let mut log = open_live_log(&work, STAGE, &record.id, label).unwrap();
        log.write_all(b"criterion output").unwrap();

        assert_eq!(
            std::fs::read(&operator).unwrap(),
            b"operator data",
            "{label}"
        );
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        assert!(metadata.is_file(), "{label}");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"criterion output",
            "{label}"
        );
    }
}
