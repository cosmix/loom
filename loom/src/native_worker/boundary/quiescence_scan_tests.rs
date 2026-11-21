//! The macOS ownership scan: which census rows belong to a boundary, and when
//! a census cannot prove that none does.
use std::collections::HashSet;

use super::quiescence_scan::{
    judge, survivors, BoundaryMark, Identity, Judgment, Observed, ProcEntry, Reading,
    BOUNDARY_MARKER_VAR,
};

const MARKER: &str = "LOOM_NATIVE_BOUNDARY=inv-1";

fn mark() -> BoundaryMark {
    BoundaryMark {
        sid: 500,
        pgid: 500,
        marker: MARKER.to_string(),
    }
}

fn entry(pid: u32, ppid: u32, pgid: u32, sid: u32, marker: Option<&str>) -> ProcEntry {
    ProcEntry {
        pid,
        ppid,
        pgid,
        sid,
        marker: marker.map(str::to_string),
    }
}

/// A readable process of this user started at `pid * 10`.
fn row(pid: u32, ppid: u32, pgid: u32, sid: u32, marker: Option<&str>) -> Observed {
    Observed {
        entry: entry(pid, ppid, pgid, sid, marker),
        start: u64::from(pid) * 10,
        foreign: false,
        zombie: false,
        reading: Reading::Read,
    }
}

fn unreadable(mut row: Observed, foreign: bool) -> Observed {
    row.reading = Reading::Unreadable("denied".into());
    row.foreign = foreign;
    row
}

fn judged(rows: &[Observed], known: &[Identity], numeric: bool) -> Judgment {
    let known: HashSet<Identity> = known.iter().copied().collect();
    judge(rows, None, &mark(), &known, numeric)
}

fn pids(judgment: &Judgment) -> Vec<u32> {
    judgment.members.iter().map(|m| m.0).collect()
}

#[test]
fn the_marker_variable_is_the_one_the_launch_sets() {
    assert_eq!(BOUNDARY_MARKER_VAR, "LOOM_NATIVE_BOUNDARY");
}

#[test]
fn descendants_of_a_match_survive_and_unrelated_processes_do_not() {
    let table = [
        entry(700, 1, 700, 700, Some(MARKER)),
        entry(701, 700, 701, 701, None),
        entry(702, 701, 702, 702, None),
        entry(800, 1, 800, 800, None),
        entry(801, 800, 800, 800, Some("LOOM_NATIVE_BOUNDARY=inv-2")),
    ];
    assert_eq!(survivors(&table, &mark()), vec![700, 701, 702]);
}

#[test]
fn a_racing_census_listing_a_pid_twice_yields_it_once() {
    let table = [
        entry(501, 500, 500, 500, None),
        entry(501, 500, 500, 500, None),
        entry(502, 501, 502, 502, None),
        entry(502, 501, 502, 502, None),
    ];
    assert_eq!(survivors(&table, &mark()), vec![501, 502]);
    assert!(survivors(&[entry(9, 1, 9, 9, None)], &mark()).is_empty());
}

#[test]
fn an_empty_readable_census_proves_release() {
    let rows = [row(1, 0, 1, 1, None), row(600, 1, 600, 600, None)];
    assert!(judged(&rows, &[], true).is_empty());
}

#[test]
fn an_unreadable_process_of_this_user_makes_the_census_uncertain() {
    let rows = [
        row(1, 0, 1, 1, None),
        unreadable(row(600, 1, 600, 600, None), false),
    ];
    let judgment = judged(&rows, &[], true);
    assert!(judgment.members.is_empty());
    assert_eq!(judgment.uncertain.len(), 1, "{judgment:?}");
    assert!(judgment.uncertain[0].contains("pid 600"), "{judgment:?}");
}

#[test]
fn an_unreadable_process_of_another_user_is_ignored_unless_something_places_it() {
    let rows = [
        unreadable(row(1, 0, 1, 1, None), true),
        unreadable(row(600, 1, 600, 600, None), true),
    ];
    assert!(judged(&rows, &[], true).is_empty());
    let placed = [
        row(500, 1, 500, 500, Some(MARKER)),
        unreadable(row(510, 500, 510, 510, None), true),
    ];
    assert_eq!(pids(&judged(&placed, &[], true)), vec![500, 510]);
}

#[test]
fn an_incomplete_table_is_never_proof() {
    let rows = [row(600, 1, 600, 600, None)];
    let judgment = judge(&rows, Some("table grew"), &mark(), &HashSet::new(), true);
    assert_eq!(judgment.uncertain, vec!["table grew".to_string()]);
    assert!(!judgment.is_empty());
}

#[test]
fn a_child_of_a_process_that_vanished_mid_census_cannot_be_placed() {
    let mut gone = row(600, 1, 600, 0, None);
    gone.reading = Reading::Gone;
    let rows = [gone, row(601, 600, 601, 601, None)];
    let judgment = judged(&rows, &[], true);
    assert!(judgment.members.is_empty());
    assert_eq!(judgment.uncertain.len(), 1, "{judgment:?}");
    let mut alone = row(700, 1, 700, 0, None);
    alone.reading = Reading::Gone;
    assert!(judged(&[alone], &[], true).is_empty());
}

#[test]
fn zombies_run_nothing_and_are_left_out() {
    let mut zombie = row(500, 1, 500, 500, Some(MARKER));
    zombie.zombie = true;
    assert!(judged(&[zombie], &[], true).is_empty());
}

#[test]
fn a_reused_session_number_is_uncertain_once_no_leader_pins_it() {
    // The leader's pid now names a new session leader that started later.
    let rows = [row(500, 1, 500, 500, None), row(501, 500, 500, 500, None)];
    let trusted = judged(&rows, &[], true);
    assert_eq!(pids(&trusted), vec![500, 501]);
    let untrusted = judged(&rows, &[], false);
    assert!(untrusted.members.is_empty(), "{untrusted:?}");
    assert_eq!(untrusted.uncertain.len(), 2, "{untrusted:?}");
    assert!(untrusted.detail(500).contains("pid 500"), "{untrusted:?}");
}

#[test]
fn a_known_identity_stays_a_member_after_losing_every_mark() {
    // 900 left the session, the group and the marker, and was reparented.
    let rows = [row(900, 1, 900, 900, None), row(901, 900, 900, 900, None)];
    assert_eq!(pids(&judged(&rows, &[(900, 9000)], false)), vec![900, 901]);
    // The same pid with another start time is another process.
    assert!(judged(&rows, &[(900, 1)], false).is_empty());
}

#[test]
fn the_detail_names_members_and_caps_the_lists() {
    let rows: Vec<Observed> = (600..610)
        .map(|pid| unreadable(row(pid, 1, pid, pid, None), false))
        .chain([row(500, 1, 500, 500, Some(MARKER))])
        .collect();
    let detail = judged(&rows, &[], true).detail(500);
    assert!(detail.starts_with("1 process(es) of boundary session 500 still run: pids 500"));
    assert!(detail.contains("5 more uncertain process(es)"), "{detail}");
}

#[test]
fn a_conclusive_judgment_has_exactly_the_survivors_as_members() {
    let rows = [
        row(504, 503, 503, 503, None),
        row(1, 0, 1, 1, None),
        row(503, 502, 503, 503, None),
        row(600, 1, 600, 600, Some("LOOM_NATIVE_BOUNDARY=inv-10")),
        row(501, 500, 501, 500, None),
        row(502, 1, 502, 502, Some(MARKER)),
        row(500, 1, 500, 500, Some(MARKER)),
        row(510, 500, 500, 500, None),
    ];
    let table: Vec<ProcEntry> = rows.iter().map(|o| o.entry.clone()).collect();
    let judgment = judged(&rows, &[], true);
    assert!(judgment.uncertain.is_empty(), "{judgment:?}");
    assert_eq!(pids(&judgment), survivors(&table, &mark()));
    assert!(judgment
        .members
        .iter()
        .all(|(pid, start)| *start == u64::from(*pid) * 10));
}
