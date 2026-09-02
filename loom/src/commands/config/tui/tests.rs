//! Headless regression coverage for the config editor's state machine.

use super::state::ConfigState;
use crate::user_config::{
    keys::KEYS, redirect_user_config, Origin, UserConfig, UserConfigRedirect,
};

/// Install one temp-path redirect for the full lifetime of each editor state test.
fn state() -> (tempfile::TempDir, UserConfigRedirect, ConfigState) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let guard = redirect_user_config(path);
    let state = ConfigState::load().unwrap();
    (temp, guard, state)
}

/// Focus the row for `name` by key rather than by position, so adding or
/// reordering a registry key cannot silently point a test at the wrong row.
/// Selection starts at 0 in every freshly loaded state, so calling
/// `move_down` once per position between there and the target is enough.
fn focus_by_name(state: &mut ConfigState, name: &str) {
    let index = KEYS
        .iter()
        .position(|spec| spec.name == name)
        .unwrap_or_else(|| panic!("no registry key named {name:?}"));
    for _ in 0..index {
        state.move_down();
    }
}

/// Replace the focused row's edit buffer with `replacement`, regardless of
/// how many characters the current value holds, so a wider or narrower
/// default can't desync a hardcoded backspace count from the text it means
/// to erase. Leaves the edit open rather than committing it, so callers keep
/// asserting on commit behavior themselves.
fn retype_focused_row(state: &mut ConfigState, replacement: &str) {
    state.begin_edit();
    let current_len = state.edit_buffer().unwrap().chars().count();
    for _ in 0..current_len {
        state.backspace();
    }
    for character in replacement.chars() {
        state.append_char(character);
    }
}

/// Navigation clamps at both ends so repeated movement never changes focus unexpectedly.
#[test]
fn selection_movement_clamps_at_the_registry_ends() {
    let (_temp, _guard, mut state) = state();
    state.move_up();
    assert_eq!(state.selected(), 0);

    for _ in 0..=KEYS.len() {
        state.move_down();
    }
    assert_eq!(state.selected(), KEYS.len() - 1);
    state.move_down();
    assert_eq!(state.selected(), KEYS.len() - 1);
}

/// Escape only removes transient typing, leaving the selected row as it was.
#[test]
fn enter_seeds_the_edit_buffer_and_escape_restores_the_row() {
    let (_temp, _guard, mut state) = state();
    let original = state.selected_row().displayed_value().to_owned();

    state.begin_edit();
    assert_eq!(state.edit_buffer(), Some(original.as_str()));
    state.append_char('x');
    state.cancel_edit();

    assert!(!state.is_editing());
    assert_eq!(state.selected_row().displayed_value(), original);
    assert!(!state.selected_row().is_modified());
}

/// A registry parse failure remains editable and makes no staged disk change.
#[test]
fn invalid_edit_stays_unmodified_and_names_its_key() {
    let (_temp, _guard, mut state) = state();
    focus_by_name(&mut state, "update.check_interval_hours");
    retype_focused_row(&mut state, "x");
    state.commit_edit();

    assert!(state.is_editing());
    assert!(!state.selected_row().is_modified());
    assert!(state.status().contains("update.check_interval_hours"));
}

/// A staged valid value is written through the shared setter and then refreshed as set.
#[test]
fn valid_edit_save_round_trips_through_a_fresh_strict_load() {
    let (_temp, _guard, mut state) = state();
    focus_by_name(&mut state, "update.check_interval_hours");
    retype_focused_row(&mut state, "6");
    state.commit_edit();
    assert!(state.selected_row().is_modified());

    state.save();
    let fresh = UserConfig::load_strict().unwrap();
    let spec = crate::user_config::keys::spec("update.check_interval_hours").unwrap();
    assert_eq!(fresh.value_of(spec).0, "6");
    assert_eq!(state.selected_row().origin(), Origin::Set);
    assert!(!state.selected_row().is_modified());
}

/// A no-op save is explicit so operators know no config file write occurred.
#[test]
fn save_with_nothing_pending_reports_that_nothing_was_written() {
    let (_temp, _guard, mut state) = state();
    state.save();
    assert_eq!(state.status(), "0 keys written; nothing pending.");
}

/// A write that fails must not silently drop the edit the operator staged.
#[test]
fn failed_write_reports_the_key_and_leaves_the_edit_staged() {
    let (temp, _guard, mut state) = state();
    focus_by_name(&mut state, "update.check_interval_hours");
    retype_focused_row(&mut state, "6");
    state.commit_edit();
    assert!(state.selected_row().is_modified());

    // `[update]` already exists as a non-table value, so `set_in` errors
    // with "is not a table" instead of writing `check_interval_hours`.
    std::fs::write(
        temp.path().join("config.toml"),
        "update = \"not-a-table\"\n",
    )
    .unwrap();

    state.save();

    assert!(state.status_is_error());
    assert!(state.status().contains("update.check_interval_hours"));
    assert!(state.status().contains("0 keys written"));
    assert!(state.selected_row().is_modified());
}
