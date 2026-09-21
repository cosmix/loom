//! Headless regression coverage for the config editor's state machine.
//!
//! Every fixture here runs against a scratch tree AND a redirected user
//! config, so the suite can never read or write the operator's real
//! `~/.loom/config.toml` — the same discipline
//! `commands::status::web::config_api::tests` keeps, and for the same reason:
//! `loom config` is the tool that creates that file.

mod cycling;
mod quit;
mod rendering;
mod save;
mod scope;

use std::path::PathBuf;

use tempfile::TempDir;

use super::state::{ConfigState, Scope, Source};
use crate::fs::work_dir::WorkDir;
use crate::user_config::{
    keys::KEYS, redirect_user_config, ConfigValue, UserConfig, UserConfigRedirect,
};

/// A scratch tree plus a user-config redirect, both inside one `TempDir`.
struct Scratch {
    /// Held for its `Drop`: the tree disappears with it.
    _temp: TempDir,
    /// The tree whose `.loom/work` supplies the project tier.
    base: PathBuf,
    /// Held for its `Drop`: the redirect lasts exactly as long as the tree.
    _redirect: UserConfigRedirect,
}

impl Scratch {
    /// The file the user scope writes to.
    fn user_config(&self) -> PathBuf {
        self.base.join("user-config.toml")
    }

    /// The file the project scope writes to.
    fn project_config(&self) -> PathBuf {
        self.base.join(".loom").join("work").join("config.toml")
    }

    /// A freshly loaded editor pointed at this tree.
    fn state(&self) -> ConfigState {
        ConfigState::load_from(&self.base).expect("load both config tiers")
    }
}

fn scratch_tree() -> Scratch {
    let temp = tempfile::tempdir().expect("create scratch tree");
    let base = temp.path().to_path_buf();
    // The recorded incident this guards against is a scratch root that came
    // back empty and sent writes at the operator's real home.
    assert!(
        base.is_absolute() && base.components().count() > 2,
        "scratch tree resolved to {}, which is not a temporary directory",
        base.display()
    );
    let _redirect = redirect_user_config(base.join("user-config.toml"));
    Scratch {
        _temp: temp,
        base,
        _redirect,
    }
}

/// A scratch tree with an initialized workspace, so both tiers exist.
fn scratch() -> Scratch {
    let scratch = scratch_tree();
    WorkDir::new(&scratch.base)
        .expect("build work dir")
        .initialize()
        .expect("initialize work dir");
    scratch
}

/// A scratch tree with no workspace at all — the ordinary case for `loom
/// config` run outside a repository.
fn scratch_without_workspace() -> Scratch {
    scratch_tree()
}

/// Install one temp-path redirect for the full lifetime of each editor state
/// test, with no workspace: these cases are about the user tier.
fn state() -> (Scratch, ConfigState) {
    let scratch = scratch_without_workspace();
    let state = scratch.state();
    (scratch, state)
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
    let (_scratch, mut state) = state();
    state.move_up();
    assert_eq!(state.selected(), 0);

    for _ in 0..=KEYS.len() {
        state.move_down();
    }
    assert_eq!(state.selected(), KEYS.len() - 1);
    state.move_down();
    assert_eq!(state.selected(), KEYS.len() - 1);
}

/// `g` and `G` reach both ends of the registry without a key repeat.
#[test]
fn first_and_last_jump_to_the_registry_ends() {
    let (_scratch, mut state) = state();
    state.move_to_last();
    assert_eq!(state.selected(), KEYS.len() - 1);
    state.move_to_first();
    assert_eq!(state.selected(), 0);
}

/// Escape only removes transient typing, leaving the selected row as it was.
#[test]
fn enter_seeds_the_edit_buffer_and_escape_restores_the_row() {
    let (_scratch, mut state) = state();
    let original = state.displayed_value();

    state.begin_edit();
    assert_eq!(state.edit_buffer(), Some(original.as_str()));
    state.append_char('x');
    state.cancel_edit();

    assert!(!state.is_editing());
    assert_eq!(state.displayed_value(), original);
    assert!(!state.is_modified());
}

/// A registry parse failure remains editable and makes no staged disk change.
#[test]
fn invalid_edit_stays_unmodified_and_names_its_key() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "update.check_interval_hours");
    retype_focused_row(&mut state, "x");
    state.commit_edit();

    assert!(state.is_editing());
    assert!(!state.is_modified());
    assert!(state.status().contains("update.check_interval_hours"));
}

/// A staged valid value is written through the shared setter and then refreshed as set.
#[test]
fn valid_edit_save_round_trips_through_a_fresh_strict_load() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "update.check_interval_hours");
    retype_focused_row(&mut state, "6");
    state.commit_edit();
    assert!(state.is_modified());

    state.save();
    let fresh = UserConfig::load_strict().unwrap();
    let spec = crate::user_config::keys::spec("update.check_interval_hours").unwrap();
    assert_eq!(fresh.value_of(spec).0, ConfigValue::Number(6));
    assert_eq!(state.selected_row().source(Scope::User), Source::Set);
    assert!(!state.is_modified());
    assert_eq!(state.status(), "1 key written · 1 user.");
}

/// A no-op save is explicit so operators know no config file write occurred.
#[test]
fn save_with_nothing_pending_reports_that_nothing_was_written() {
    let (_scratch, mut state) = state();
    state.save();
    assert_eq!(state.status(), "0 keys written; nothing pending.");
}

/// Clearing a key the file does not set is a named no-op rather than a write.
#[test]
fn clearing_an_unset_key_stages_nothing_and_says_so() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "update.check_interval_hours");
    state.stage_clear();

    assert!(!state.is_modified());
    assert!(state.status().contains("update.check_interval_hours"));
    assert!(state.status().contains("nothing to clear"));
}

/// Clearing a set key reverts it to the built-in and removes it from the file.
#[test]
fn clearing_a_set_key_writes_the_key_out_of_the_file() {
    let (scratch, mut state) = state();
    focus_by_name(&mut state, "update.check_interval_hours");
    retype_focused_row(&mut state, "6");
    state.commit_edit();
    state.save();
    assert!(std::fs::read_to_string(scratch.user_config())
        .unwrap()
        .contains("check_interval_hours"));

    state.stage_clear();
    assert!(state.is_modified());
    state.save();

    let saved = std::fs::read_to_string(scratch.user_config()).unwrap();
    assert!(!saved.contains("check_interval_hours"), "{saved}");
    assert_eq!(state.selected_row().source(Scope::User), Source::Default);
}
