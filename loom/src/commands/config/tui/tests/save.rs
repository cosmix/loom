//! Coverage for `save`'s two independent failure modes: a write that fails,
//! and a reload that fails after the writes it is reloading for succeeded.
//!
//! The two are deliberately kept apart in these fixtures — corrupting the
//! FILE a staged edit targets forces a write failure, corrupting the OTHER
//! tier's file forces a reload failure while that same write still succeeds
//! — because `refresh_after_save` has to tell them apart in its status line.

use super::{focus_by_name, retype_focused_row, scratch, state};
use crate::user_config::{ConfigValue, UserConfig};

/// A write that fails must not silently drop the edit the operator staged.
#[test]
fn failed_write_reports_the_key_and_leaves_the_edit_staged() {
    let (scratch, mut state) = state();
    focus_by_name(&mut state, "update.check_interval_hours");
    retype_focused_row(&mut state, "6");
    state.commit_edit();
    assert!(state.is_modified());

    // `[update]` already exists as a non-table value, so `set_in` errors
    // with "is not a table" instead of writing `check_interval_hours`.
    std::fs::write(scratch.user_config(), "update = \"not-a-table\"\n").unwrap();

    state.save();

    assert!(state.status_is_error());
    assert!(state.status().contains("update.check_interval_hours"));
    assert!(state.status().contains("0 keys written"));
    assert!(state.is_modified());
}

/// A reload failure after a successful write must not leave the write's own
/// key looking unsaved, and must not swallow a write failure that happened
/// alongside it.
#[test]
fn a_reload_failure_clears_pending_for_the_written_key_and_says_the_screen_is_stale() {
    let scratch = scratch();
    let mut state = scratch.state();
    focus_by_name(&mut state, "update.check_interval_hours");
    retype_focused_row(&mut state, "6");
    state.commit_edit();
    assert!(state.is_modified());

    // The staged edit writes the USER file. Corrupting the PROJECT file
    // instead means that write still succeeds, and only the reload after it
    // fails — the two failures this fix must tell apart.
    std::fs::write(scratch.project_config(), "not = valid = toml\n").unwrap();

    state.save();

    assert!(state.status_is_error());
    assert!(state.status().contains("1 key written"));
    assert!(state.status().contains("stale"));
    assert!(
        !state.is_modified(),
        "the write landed on disk; the pending dot must clear"
    );
    let fresh = UserConfig::load_strict().unwrap();
    let key = crate::user_config::keys::spec("update.check_interval_hours").unwrap();
    assert_eq!(fresh.value_of(key).0, ConfigValue::Number(6));
}

/// The same reload failure, but with a write failure staged alongside the
/// successful one: both problems must survive into the one status line.
#[test]
fn a_reload_failure_still_names_a_write_failure_staged_alongside_it() {
    let scratch = scratch();
    let mut state = scratch.state();
    focus_by_name(&mut state, "update.check_interval_hours");
    retype_focused_row(&mut state, "6");
    state.commit_edit();
    state.toggle_scope();
    // `focus_by_name` moves down from wherever the selection already is, so
    // the focus from the first key has to be dropped before it can land on
    // the second one.
    state.move_to_first();
    focus_by_name(&mut state, "terminal.backend");
    state.cycle(1);
    state.toggle_scope();

    // Syntactically invalid TOML fails BOTH the project write (its own
    // read-modify-write cannot parse the document to edit it) and the reload
    // that follows the user write which does succeed — the one case where
    // this fix's two failures happen together.
    std::fs::write(scratch.project_config(), "not = valid = toml").unwrap();

    state.save();

    assert!(state.status_is_error());
    assert!(state.status().contains("1 key written"));
    assert!(state.status().contains("terminal.backend"));
    assert!(state.status().contains("stale"));
}
