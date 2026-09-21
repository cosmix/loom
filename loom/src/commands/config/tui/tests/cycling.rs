use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::{
    dispatch_key,
    state::{opens_editor, ConfigState},
};
use super::{focus_by_name, state};
use crate::user_config::keys::ValueKind;

fn press(state: &mut ConfigState, code: KeyCode) {
    assert!(!dispatch_key(
        state,
        KeyEvent::new(code, KeyModifiers::NONE),
    ));
}

#[test]
fn cycles_an_enum_forward_and_wraps() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "terminal.backend");

    state.cycle(1);
    assert_eq!(state.displayed_value(), "tmux");
    state.cycle(1);
    assert_eq!(state.displayed_value(), "native");
}

#[test]
fn cycles_an_enum_backward_and_wraps() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "terminal.backend");

    state.cycle(-1);

    assert_eq!(state.displayed_value(), "tmux");
}

#[test]
fn toggles_a_bool_with_space() {
    let (_scratch, mut state) = state();

    press(&mut state, KeyCode::Char(' '));

    assert_eq!(state.displayed_value(), "false");
    assert!(state.is_modified());
}

#[test]
fn refuses_to_cycle_a_number_key() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "context.ceiling_tokens");
    let before = state.displayed_value();

    state.cycle(1);

    assert!(!state.is_modified());
    assert_eq!(state.displayed_value(), before);
    assert!(state.status_is_error());
    assert_eq!(
        state.status(),
        "context.ceiling_tokens has no variants to cycle; press Enter to edit."
    );
}

#[test]
fn saves_a_cycled_enum_to_disk() {
    let (scratch, mut state) = state();
    focus_by_name(&mut state, "terminal.backend");
    state.cycle(1);

    state.save();

    let saved = std::fs::read_to_string(scratch.user_config()).unwrap();
    assert!(saved.contains("backend = \"tmux\""), "{saved}");
}

#[test]
fn saves_a_toggled_bool_to_disk() {
    let (scratch, mut state) = state();
    press(&mut state, KeyCode::Char(' '));

    state.save();

    let saved = std::fs::read_to_string(scratch.user_config()).unwrap();
    assert!(saved.contains("check = false"), "{saved}");
    assert!(!saved.contains("\"false\""), "{saved}");
}

#[test]
fn enter_edits_only_the_free_form_kinds() {
    {
        let (_scratch, mut state) = state();
        press(&mut state, KeyCode::Enter);
        assert_eq!(state.displayed_value(), "false");
        assert!(state.is_modified());
        assert!(!state.is_editing());
    }
    {
        let (_scratch, mut state) = state();
        focus_by_name(&mut state, "terminal.backend");
        press(&mut state, KeyCode::Enter);
        assert_eq!(state.displayed_value(), "tmux");
        assert!(state.is_modified());
        assert!(!state.is_editing());
    }
    {
        let (_scratch, mut state) = state();
        focus_by_name(&mut state, "context.ceiling_tokens");
        let before = state.displayed_value();
        press(&mut state, KeyCode::Enter);
        assert!(state.is_editing());
        assert!(!state.is_modified());
        assert_eq!(state.displayed_value(), before);
    }

    assert!(opens_editor(&ValueKind::Number));
    assert!(opens_editor(&ValueKind::String));
    assert!(!opens_editor(&ValueKind::Bool));
    assert!(!opens_editor(&ValueKind::Enum(&["a"])));
}

#[test]
fn cycle_keys_dispatch_through_the_key_handler() {
    {
        let (_scratch, mut state) = state();
        focus_by_name(&mut state, "terminal.backend");
        press(&mut state, KeyCode::Left);
        assert_eq!(state.displayed_value(), "tmux");
        press(&mut state, KeyCode::Right);
        assert_eq!(state.displayed_value(), "native");
        press(&mut state, KeyCode::Char('h'));
        assert_eq!(state.displayed_value(), "tmux");
        press(&mut state, KeyCode::Char('l'));
        assert_eq!(state.displayed_value(), "native");
    }
    {
        let (_scratch, mut state) = state();
        press(&mut state, KeyCode::Char(' '));
        assert_eq!(state.displayed_value(), "false");
        focus_by_name(&mut state, "context.ceiling_tokens");
        let before = state.displayed_value();
        press(&mut state, KeyCode::Enter);
        press(&mut state, KeyCode::Char('h'));
        press(&mut state, KeyCode::Char('l'));
        press(&mut state, KeyCode::Char(' '));
        let expected = format!("{before}hl ");
        assert_eq!(state.edit_buffer(), Some(expected.as_str()));
    }
}

/// `Tab` and `x` reach the scope switch and the clear from the key handler,
/// not only from the state machine's own methods.
#[test]
fn scope_and_clear_keys_dispatch_through_the_key_handler() {
    let (_scratch, mut state) = state();
    press(&mut state, KeyCode::Tab);
    assert_eq!(
        state.scope(),
        super::super::state::Scope::Project,
        "{}",
        state.status()
    );
    press(&mut state, KeyCode::BackTab);
    assert_eq!(state.scope(), super::super::state::Scope::User);

    press(&mut state, KeyCode::Char('x'));
    assert!(
        state.status().contains("nothing to clear"),
        "{}",
        state.status()
    );
}
