//! The real host's probe run: a probe boundary past its timeout.
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

use super::super::super::linux::{self, Plan};
use super::super::super::tests::{canonical_tempdir, no_boundary, sh};
use super::{finish_within, retire};
use crate::process::OwnedProcess;
use crate::verify::criteria::pid_namespace::members;

/// A probe still running at its timeout has its namespace killed and empty,
/// and its scratch removed, by the time the run returns.
#[test]
fn a_timed_out_probe_empties_its_namespace_before_its_scratch_goes() {
    let name = "a_timed_out_probe_empties_its_namespace_before_its_scratch_goes";
    if no_boundary(name) {
        return;
    }
    let (_tmp, w) = canonical_tempdir();
    let plan = Plan::payload(&w, None, Path::new("/bin/sh"), &sh("sleep 30")).unwrap();
    let host: Vec<_> = std::env::vars_os().collect();
    let boundary = linux::start(&plan, &host).unwrap();
    let namespace = boundary.pid_namespace;
    let scratch = boundary.scratch.path().to_path_buf();
    let started = Instant::now();
    let error = finish_within(boundary, Duration::from_millis(100)).unwrap_err();
    assert!(error.contains("did not exit within 100 ms"), "{error}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(members(namespace), Ok(Vec::new()));
    assert!(!scratch.exists(), "{} survived", scratch.display());
}

/// A probe reported exited while a member of its namespace still runs has
/// that namespace killed and empty, and its scratch removed, by the time the
/// run returns: the outer bubblewrap's exit proves nothing about the members.
/// The real bubblewrap is held aside, a stand-in retired in its place, so only
/// the namespace kill can empty the namespace.
#[test]
fn an_exited_probe_still_empties_its_namespace_before_its_scratch_goes() {
    let name = "an_exited_probe_still_empties_its_namespace_before_its_scratch_goes";
    if no_boundary(name) {
        return;
    }
    let (_tmp, w) = canonical_tempdir();
    let plan = Plan::payload(&w, None, Path::new("/bin/sh"), &sh("sleep 30")).unwrap();
    let host: Vec<_> = std::env::vars_os().collect();
    let mut boundary = linux::start(&plan, &host).unwrap();
    let namespace = boundary.pid_namespace;
    let scratch = boundary.scratch.path().to_path_buf();
    let stand_in = Command::new("true").process_group(0).spawn().unwrap();
    let real = std::mem::replace(&mut boundary.process, OwnedProcess::new(stand_in));
    let exited = Ok(Some(ExitStatus::from_raw(0)));
    let (status, reported, _) = retire(boundary, exited, Duration::from_secs(1)).unwrap();
    let left = members(namespace);
    drop(real);
    assert!(status.success() && reported == namespace);
    assert_eq!(left, Ok(Vec::new()));
    assert!(!scratch.exists(), "{} survived", scratch.display());
}
