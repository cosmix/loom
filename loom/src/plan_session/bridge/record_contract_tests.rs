//! Contract of the session record: a write the filesystem refuses leaves the record dirty, so a
//! later driver tick writes the newest generation once the path accepts it. A record step that
//! clears the dirty flag before the write and never sets it again, atomic replace or not, drops
//! the generation the refused write carried.

use std::fs;

use super::fake_host::{settle, wait_until, Harness};
use super::registry::read_record;
use crate::plan_session::kind::SessionKind::Authoring;

#[test]
fn a_refused_record_write_is_retried_on_a_later_tick() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    // The driver has written whatever the opening left dirty.
    settle();
    let first = read_record(&s.dir).unwrap();
    assert_eq!(first.feature, None, "the record already names a feature");

    // A non-empty directory at the path refuses a write whatever the uid.
    let record = s.dir.record();
    fs::remove_file(&record).unwrap();
    fs::create_dir(&record).unwrap();
    fs::write(record.join("kept"), b"kept").unwrap();

    h.bridge
        .set_owner(&s.id, Some("alpha".to_owned()), None)
        .unwrap();
    // Ticks run against the refused path: each write fails.
    settle();
    assert!(record.is_dir(), "the refused write replaced the directory");

    // The path accepts a write again: the newest record must arrive on its own. The saved
    // bytes are not put back, so only a retried write can create the file.
    fs::remove_dir_all(&record).unwrap();
    wait_until("the retried record write", || {
        read_record(&s.dir).is_ok_and(|record| record.feature.as_deref() == Some("alpha"))
    });
}
