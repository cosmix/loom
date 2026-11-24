//! Contract: a rerun of a replacement trial follows the request it already relayed, even after
//! the daemon's own overlay of the replacement bytes changed the worktree digest.

use std::{cell::Cell, path::PathBuf, time::Duration};

use tempfile::TempDir;

use crate::host_verify::wait::{wait_or_request, WaitOutcome, WaitSite};
use crate::host_verify::JobKind;
use crate::models::dispute::FileReplacement;
use crate::relay::HostJobRequest;
use crate::test_git::{git, TestRepo};

const JOB: &str = "33333333333333333333333333333333";

struct Fx {
    _tmp: TempDir,
    checkout: PathBuf,
    work_dir: PathBuf,
    scratch: PathBuf,
}

fn fixture() -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let checkout = tmp.path().join("checkout");
    let work_dir = tmp.path().join(".loom").join("work");
    let scratch = tmp.path().join("scratch");
    for dir in [&checkout, &work_dir, &scratch] {
        std::fs::create_dir_all(dir).unwrap();
    }
    TestRepo::builder().no_seed().init_in(&checkout);
    std::fs::write(checkout.join("a.txt"), "one\n").unwrap();
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-q", "-m", "base"]);
    Fx {
        _tmp: tmp,
        checkout,
        work_dir,
        scratch,
    }
}

impl Fx {
    fn site<'a>(&'a self, contracts: &'a [FileReplacement]) -> WaitSite<'a> {
        WaitSite {
            work_dir: &self.work_dir,
            stage_id: "s1",
            session_id: "session-1",
            checkout: &self.checkout,
            knowledge: false,
            kind: JobKind::Check,
            no_cache: false,
            criteria: None,
            contracts: Some(contracts),
            at_base: false,
            scratch_dir: &self.scratch,
            budget: Duration::ZERO,
        }
    }
}

#[test]
fn a_trial_rerun_after_the_overlay_relays_nothing() {
    let fx = fixture();
    let trial = [FileReplacement {
        file: "a.txt".to_string(),
        content: "two\n".to_string(),
        base_sha256: None,
    }];
    let calls = Cell::new(0_u32);
    let mut relay = |_: &HostJobRequest| -> anyhow::Result<String> {
        calls.set(calls.get() + 1);
        Ok(JOB.to_string())
    };

    let mut out = Vec::new();
    let first = wait_or_request(&fx.site(&trial), &mut relay, &mut out).unwrap();
    assert_eq!(
        first,
        WaitOutcome::Requested {
            request_id: JOB.into()
        }
    );

    // What the daemon's trial overlay does while the trial runs.
    std::fs::write(fx.checkout.join("a.txt"), "two\n").unwrap();

    let second = wait_or_request(&fx.site(&trial), &mut relay, &mut out).unwrap();
    assert_eq!(
        calls.get(),
        1,
        "the rerun relayed a second request, superseding the running trial; second outcome: {second:?}"
    );
    assert!(
        !matches!(second, WaitOutcome::Refused(_)),
        "the rerun was refused: {second:?}"
    );
}
