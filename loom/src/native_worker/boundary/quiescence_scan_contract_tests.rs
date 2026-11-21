//! Contract: the macOS ownership scan finds every process the boundary
//! started, whether it kept the boundary's session, its process group, the
//! exact marker in its environment, or only a matched ancestor.
use super::quiescence_scan::{survivors, BoundaryMark, ProcEntry};

const MARKER: &str = "LOOM_NATIVE_BOUNDARY=inv-1";

fn entry(pid: u32, ppid: u32, pgid: u32, sid: u32, marker: Option<&str>) -> ProcEntry {
    ProcEntry {
        pid,
        ppid,
        pgid,
        sid,
        marker: marker.map(str::to_string),
    }
}

#[test]
fn a_member_that_left_the_process_group_is_still_a_survivor() {
    let mark = BoundaryMark {
        sid: 500,
        pgid: 500,
        marker: MARKER.to_string(),
    };
    // Listed out of order, so a single pass over the table cannot reach the
    // deepest descendant; pid 501 appears twice, as a racing census may list it.
    let table = [
        entry(504, 503, 503, 503, None),
        entry(1, 0, 1, 1, None),
        entry(503, 502, 503, 503, None),
        entry(600, 1, 600, 600, Some("LOOM_NATIVE_BOUNDARY=inv-10")),
        entry(601, 600, 601, 601, None),
        entry(602, 1, 602, 602, Some("HOME=/Users/someone")),
        entry(501, 500, 501, 500, None),
        entry(502, 1, 502, 502, Some(MARKER)),
        entry(500, 1, 500, 500, Some(MARKER)),
        entry(501, 500, 501, 500, None),
        entry(510, 500, 500, 500, None),
    ];
    // 500: the session and group leader. 510: still in the group. 501: left
    // the group with setpgid but kept the session. 502: started its own
    // session but keeps the exact marker. 503: only its parent 502 matched.
    // 504: only its grandparent 502 matched. 600 carries a marker that merely
    // shares a prefix, 601 is its child, 602 and launchd share nothing.
    assert_eq!(
        survivors(&table, &mark),
        vec![500, 501, 502, 503, 504, 510],
        "survivors are the session, group and marker matches plus their \
         transitive descendants, sorted and deduplicated"
    );
}
