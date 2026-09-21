//! Coverage for the quit-warning guard: which key confirms a discard, which
//! one only dismisses it, and what re-arms it after a refusal.
//!
//! Two tabs mean a discard can cost edits the operator is not even looking
//! at, so the guard — and the tests pinning it — count staged edits across
//! both scopes rather than just the one on screen.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::dispatch_key;
use super::{focus_by_name, retype_focused_row, state};

/// With nothing staged there is nothing to lose, so `q` quits at once.
#[test]
fn quitting_with_nothing_staged_is_immediate() {
    let (_scratch, mut state) = state();
    assert!(state.request_quit());
}

/// A staged edit on either tab makes the first `q` a warning, not a discard.
/// The second one quits.
#[test]
fn quitting_is_refused_once_while_edits_are_staged() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "update.check_interval_hours");
    retype_focused_row(&mut state, "6");
    state.commit_edit();

    assert!(!state.request_quit());
    assert!(state.status_is_error());
    assert_eq!(
        state.status(),
        "1 edit staged · 1 user — press q again to discard, or s to save."
    );
    assert!(state.is_modified(), "the warning must not discard anything");

    assert!(state.request_quit());
}

/// Anything the operator does between the two presses re-arms the warning:
/// they are still working, and the next `q` must not be a silent discard.
#[test]
fn an_intervening_key_re_arms_the_quit_warning() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "terminal.backend");
    state.cycle(1);

    assert!(!dispatch_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)
    ));
    assert!(!dispatch_key(
        &mut state,
        KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)
    ));
    assert!(
        !dispatch_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)
        ),
        "the arrow key re-armed the warning"
    );
    assert!(dispatch_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)
    ));
}

/// `Esc` cancels an armed quit warning rather than confirming it: only a
/// second `q` may confirm, because `Esc` means cancel everywhere else in this
/// editor. Dismissing it must not discard the staged edit, and the next `q`
/// has to warn again rather than treat the dismissal as the second press.
#[test]
fn escape_dismisses_an_armed_quit_warning_instead_of_confirming_it() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "terminal.backend");
    state.cycle(1);

    assert!(!dispatch_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)
    ));
    assert!(state.status_is_error(), "the warning is armed");

    assert!(
        !dispatch_key(&mut state, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        "Esc must not quit while the warning is armed"
    );
    assert!(!state.status_is_error(), "the dismissal is not an error");
    assert!(state.is_modified(), "Esc must not discard the staged edit");

    assert!(
        !dispatch_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)
        ),
        "the dismissal must not count as the second press"
    );
    assert!(state.status_is_error());
    assert!(dispatch_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)
    ));
}

/// With no warning armed, `Esc` falls through to the same rule `q` uses:
/// immediate when nothing is staged, a warning otherwise.
#[test]
fn escape_behaves_like_q_when_nothing_is_armed() {
    {
        let (_scratch, mut state) = state();
        assert!(dispatch_key(
            &mut state,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
        ));
    }
    {
        let (_scratch, mut state) = state();
        focus_by_name(&mut state, "terminal.backend");
        state.cycle(1);
        assert!(!dispatch_key(
            &mut state,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
        ));
        assert!(state.status_is_error());
        assert!(state.is_modified());
    }
}

/// Saving clears what the warning was protecting, so `q` stops warning.
#[test]
fn a_save_stops_the_quit_warning() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "terminal.backend");
    state.cycle(1);
    state.save();

    assert!(state.request_quit());
}

/// Ctrl+C is the operator's escape hatch and never warns.
#[test]
fn ctrl_c_quits_even_with_edits_staged() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "terminal.backend");
    state.cycle(1);

    assert!(dispatch_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
    ));
}
