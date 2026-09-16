use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};

use super::super::{
    dispatch_key, render,
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

fn screen(state: &ConfigState) -> Vec<String> {
    let width = 160;
    let mut terminal = Terminal::new(TestBackend::new(width, 30)).unwrap();
    terminal.draw(|frame| render::draw(frame, state)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .chunks(usize::from(width))
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end_matches(' ')
                .to_owned()
        })
        .collect()
}

fn contains(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|row| row.contains(needle))
}

#[test]
fn cycles_an_enum_forward_and_wraps() {
    let (_temp, _guard, mut state) = state();
    focus_by_name(&mut state, "terminal.backend");

    state.cycle(1);
    assert_eq!(state.selected_row().displayed_value(), "tmux");
    state.cycle(1);
    assert_eq!(state.selected_row().displayed_value(), "native");
}

#[test]
fn cycles_an_enum_backward_and_wraps() {
    let (_temp, _guard, mut state) = state();
    focus_by_name(&mut state, "terminal.backend");

    state.cycle(-1);

    assert_eq!(state.selected_row().displayed_value(), "tmux");
}

#[test]
fn toggles_a_bool_with_space() {
    let (_temp, _guard, mut state) = state();

    press(&mut state, KeyCode::Char(' '));

    assert_eq!(state.selected_row().displayed_value(), "false");
    assert!(state.selected_row().is_modified());
}

#[test]
fn refuses_to_cycle_a_number_key() {
    let (_temp, _guard, mut state) = state();
    focus_by_name(&mut state, "context.ceiling_tokens");
    let before = state.selected_row().displayed_value();

    state.cycle(1);

    assert!(!state.selected_row().is_modified());
    assert_eq!(state.selected_row().displayed_value(), before);
    assert!(state.status_is_error());
    assert_eq!(
        state.status(),
        "context.ceiling_tokens has no variants to cycle; press Enter to edit."
    );
}

#[test]
fn saves_a_cycled_enum_to_disk() {
    let (temp, _guard, mut state) = state();
    focus_by_name(&mut state, "terminal.backend");
    state.cycle(1);

    state.save();

    let saved = std::fs::read_to_string(temp.path().join("config.toml")).unwrap();
    assert!(saved.contains("backend = \"tmux\""), "{saved}");
}

#[test]
fn saves_a_toggled_bool_to_disk() {
    let (temp, _guard, mut state) = state();
    press(&mut state, KeyCode::Char(' '));

    state.save();

    let saved = std::fs::read_to_string(temp.path().join("config.toml")).unwrap();
    assert!(saved.contains("check = false"), "{saved}");
    assert!(!saved.contains("\"false\""), "{saved}");
}

#[test]
fn enter_edits_only_the_free_form_kinds() {
    {
        let (_temp, _guard, mut state) = state();
        press(&mut state, KeyCode::Enter);
        assert_eq!(state.selected_row().displayed_value(), "false");
        assert!(state.selected_row().is_modified());
        assert!(!state.is_editing());
    }
    {
        let (_temp, _guard, mut state) = state();
        focus_by_name(&mut state, "terminal.backend");
        press(&mut state, KeyCode::Enter);
        assert_eq!(state.selected_row().displayed_value(), "tmux");
        assert!(state.selected_row().is_modified());
        assert!(!state.is_editing());
    }
    {
        let (_temp, _guard, mut state) = state();
        focus_by_name(&mut state, "context.ceiling_tokens");
        let before = state.selected_row().displayed_value();
        press(&mut state, KeyCode::Enter);
        assert!(state.is_editing());
        assert!(!state.selected_row().is_modified());
        assert_eq!(state.selected_row().displayed_value(), before);
    }

    assert!(opens_editor(&ValueKind::Number));
    assert!(opens_editor(&ValueKind::String));
    assert!(!opens_editor(&ValueKind::Bool));
    assert!(!opens_editor(&ValueKind::Enum(&["a"])));
}

#[test]
fn cycle_keys_dispatch_through_the_key_handler() {
    {
        let (_temp, _guard, mut state) = state();
        focus_by_name(&mut state, "terminal.backend");
        press(&mut state, KeyCode::Left);
        assert_eq!(state.selected_row().displayed_value(), "tmux");
        press(&mut state, KeyCode::Right);
        assert_eq!(state.selected_row().displayed_value(), "native");
        press(&mut state, KeyCode::Char('h'));
        assert_eq!(state.selected_row().displayed_value(), "tmux");
        press(&mut state, KeyCode::Char('l'));
        assert_eq!(state.selected_row().displayed_value(), "native");
    }
    {
        let (_temp, _guard, mut state) = state();
        press(&mut state, KeyCode::Char(' '));
        assert_eq!(state.selected_row().displayed_value(), "false");
        focus_by_name(&mut state, "context.ceiling_tokens");
        let before = state.selected_row().displayed_value();
        press(&mut state, KeyCode::Enter);
        press(&mut state, KeyCode::Char('h'));
        press(&mut state, KeyCode::Char('l'));
        press(&mut state, KeyCode::Char(' '));
        let expected = format!("{before}hl ");
        assert_eq!(state.edit_buffer(), Some(expected.as_str()));
    }
}

#[test]
fn bool_cell_renders_checkbox_and_enum_cell_renders_guillemets() {
    let (_temp, _guard, mut state) = state();
    let rows = screen(&state);
    assert!(contains(&rows, "[x] on"), "{rows:?}");
    assert!(
        rows.iter()
            .any(|row| row.contains("update.check_interval_hours") && row.contains("24")),
        "{rows:?}"
    );
    assert!(
        contains(
            &rows,
            "↑↓/k/j move  ←/→/space cycle  Enter edit or cycle  s save  Esc/q quit  * pending"
        ),
        "{rows:?}"
    );

    state.cycle(1);
    let rows = screen(&state);
    assert!(contains(&rows, "[ ] off"), "{rows:?}");
    focus_by_name(&mut state, "terminal.backend");
    let rows = screen(&state);
    assert!(contains(&rows, "‹ native ›"), "{rows:?}");
}
