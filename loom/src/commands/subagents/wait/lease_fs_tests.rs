use std::fs;
use std::os::fd::AsRawFd;
use std::time::Duration;

use super::write_active;
use crate::commands::subagents::wait::tests::{acquire_owner, fixture};
use crate::fs::safe_fs::safe_open_dirfd;
use crate::fs::safe_replace::FAIL_DIR_SYNC;

#[test]
fn a_failed_directory_sync_after_the_rename_is_an_error() {
    let fixture = fixture();
    let mut lease = acquire_owner(&fixture, Duration::from_secs(30));
    lease.source_revision = "revision-b".into();
    let root = fixture.dir.root();
    let dirfd = safe_open_dirfd(root).unwrap();
    FAIL_DIR_SYNC.with(|flag| flag.set(true));
    let written = write_active(dirfd.as_raw_fd(), root, &lease);
    FAIL_DIR_SYNC.with(|flag| flag.set(false));
    let error = format!("{:#}", written.unwrap_err());
    assert!(
        error.contains("target was replaced but its directory sync failed"),
        "{error}"
    );
    let stored = fs::read_to_string(root.join("lease.json")).unwrap();
    assert!(
        stored.contains("revision-b"),
        "the rename happened: {stored}"
    );
}
