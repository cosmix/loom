//! The private scratch of a boundary whose namespace was not proven empty
//! stays on disk, marked for the sweep; a released one goes.
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::Command;

use super::launch::{launch, started_then_failed};
use super::tests::{canonical_tempdir, no_boundary, no_bwrap, sh};
use super::{keep_scratch_unless_released, spawn_payload, spawn_started, Quiescence};
use crate::verify::criteria::CargoScratch;

/// The marker file the scratch sweep reads (`confine_scratch/marker.rs`).
const MARKER: &str = ".loom-boundary";

fn marker(scratch: &Path) -> String {
    std::fs::read_to_string(scratch.join(MARKER)).expect("boundary marker")
}

fn remove_kept(path: &Path) {
    std::fs::remove_dir_all(path).expect("remove the kept scratch");
}

/// The supervisor's teardown: an unverified namespace keeps the scratch,
/// marked with that namespace; a released one removes it.
#[test]
fn an_unproven_namespace_keeps_its_scratch() {
    if no_boundary("an_unproven_namespace_keeps_its_scratch") {
        return;
    }
    let (_tmp, w) = canonical_tempdir();
    let cases = [
        (Quiescence::Unverified("members remain".into()), true),
        (Quiescence::Released, false),
    ];
    for (quiescence, kept) in cases {
        let mut boundary = spawn_payload(&w, Path::new("/bin/sh"), &sh("sleep 30")).unwrap();
        let path = boundary.scratch.path().to_path_buf();
        let recorded = marker(&path);
        let namespace = format!(" {}", boundary.pid_namespace);
        assert!(recorded.trim_end().ends_with(&namespace), "{recorded}");
        keep_scratch_unless_released(&mut boundary.scratch, &quiescence);
        boundary.process.retire();
        drop(boundary);
        assert_eq!(path.exists(), kept, "{quiescence:?}");
        if kept {
            remove_kept(&path);
        }
    }
}

/// A start that failed after bubblewrap ran keeps its scratch, marked with
/// no namespace, so only a reboot lets the sweep reclaim it.
#[test]
fn a_start_that_failed_after_spawn_keeps_its_scratch() {
    let (_tmp, w) = canonical_tempdir();
    let mut command = Command::new("true");
    command.env_clear();
    let scratch = CargoScratch::prepare_private(&command, &w).unwrap();
    scratch.mark_boundary(None).unwrap();
    let path = scratch.path().to_path_buf();
    let error = started_then_failed(scratch, "no info".into());
    assert!(spawn_started(&error));
    assert!(path.is_dir(), "the scratch was removed");
    assert_eq!(marker(&path).split_whitespace().count(), 1);
    remove_kept(&path);
}

/// Bubblewrap refusing its arguments exits before it creates a namespace: the
/// error says nothing was started and the scratch goes with it.
#[test]
fn a_bubblewrap_that_fails_before_its_namespace_removes_its_scratch() {
    if no_bwrap("a_bubblewrap_that_fails_before_its_namespace_removes_its_scratch") {
        return;
    }
    let (_tmp, w) = canonical_tempdir();
    let mut probe = Command::new("true");
    probe.env_clear();
    let scratch = CargoScratch::prepare_private(&probe, &w).unwrap();
    let path = scratch.path().to_path_buf();
    let (reader, writer) = std::io::pipe().unwrap();
    let mut command = Command::new(which::which("bwrap").unwrap());
    let info_fd = writer.as_raw_fd().to_string();
    command.args(["--info-fd", &info_fd, "--no-such-option", "--", "true"]);
    let error = launch(command, scratch, reader, writer)
        .err()
        .expect("bubblewrap failed");
    assert!(!spawn_started(&error), "{error:#}");
    assert!(!path.exists(), "{} survived", path.display());
}
