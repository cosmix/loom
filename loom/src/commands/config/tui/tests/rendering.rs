//! Coverage for how the screen degrades: which columns and chrome survive a
//! narrow or short frame, and what the table and the inspector actually draw.

use ratatui::{backend::TestBackend, Terminal};

use super::super::{render, state::ConfigState};
use super::{focus_by_name, scratch, state};
use crate::user_config::keys::KEYS;

fn screen(state: &ConfigState) -> Vec<String> {
    screen_sized(state, 160, 30)
}

/// Render at an exact terminal size, so the responsive forms are testable.
fn screen_sized(state: &ConfigState, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
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
fn bool_cell_renders_its_glyph_and_an_enum_cell_renders_guillemets() {
    let (_scratch, mut state) = state();
    let rows = screen(&state);
    assert!(contains(&rows, "◉ on"), "{rows:?}");
    // The KEY cell carries the bare field name; the section heading above it
    // supplies `update.`, and the inspector's title the full dotted key.
    assert!(
        rows.iter()
            .any(|row| row.contains("check_interval_hours") && row.contains("24")),
        "{rows:?}"
    );
    assert!(!contains(&rows, "update.check_interval_hours"), "{rows:?}");
    assert!(contains(&rows, "── update"), "{rows:?}");
    assert!(
        contains(
            &rows,
            "↑↓ move · ←→ cycle · ⏎ edit · x clear · ⇥ scope · s save · q quit"
        ),
        "{rows:?}"
    );

    state.cycle(1);
    let rows = screen(&state);
    assert!(contains(&rows, "◯ off"), "{rows:?}");
    focus_by_name(&mut state, "terminal.backend");
    let rows = screen(&state);
    assert!(contains(&rows, "‹ native ›"), "{rows:?}");
}

/// The screen names both files and marks the one the keyboard writes, which
/// is the whole point of the scope band.
#[test]
fn the_scope_band_names_both_tiers() {
    let (_scratch, mut state) = state();
    let rows = screen(&state);
    assert!(contains(&rows, "USER · global"), "{rows:?}");
    assert!(contains(&rows, "PROJECT · unavailable"), "{rows:?}");
    assert!(contains(&rows, "configuration"), "{rows:?}");

    state.toggle_scope();
    let rows = screen(&state);
    assert!(contains(&rows, "no .loom/work"), "{rows:?}");
}

/// The scope tabs' own narrow-width arithmetic, carried through columns the
/// existing chrome coverage never varies (it only varies height, at a fixed
/// width of 160). Sound already: both tab labels stay intact — only each
/// path's tail gives way, the same truncation the write segment elsewhere on
/// this screen uses, and never the whole tab.
#[test]
fn the_scope_tabs_survive_narrow_widths() {
    let scratch = scratch();
    let state = scratch.state();

    for width in [80, 64, 56] {
        let rows = screen_sized(&state, width, 24);
        assert!(contains(&rows, "USER"), "width {width}: {rows:?}");
        assert!(contains(&rows, "PROJECT"), "width {width}: {rows:?}");
        assert!(
            contains(&rows, "toml"),
            "width {width}: the project path's tail should still name the file: {rows:?}"
        );
    }
}

/// The inspector changes form rather than disappearing: at 80x24 the same
/// facts sit under the table instead of beside it, and the table keeps its
/// key and its value.
#[test]
fn the_inspector_survives_a_narrow_frame_as_a_strip() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "terminal.backend");
    let rows = screen_sized(&state, 80, 24);

    // The strip's own two lines: the help text, then the tiers and the file.
    assert!(contains(&rows, "Terminal backend loom run"), "{rows:?}");
    assert!(contains(&rows, "built-in"), "{rows:?}");
    assert!(contains(&rows, "s →"), "{rows:?}");
    // The strip's block is titled with the full dotted key.
    assert!(contains(&rows, "terminal.backend"), "{rows:?}");
    // The table still shows the key and its value.
    assert!(contains(&rows, "‹ native ›"), "{rows:?}");
    assert!(contains(&rows, "── terminal"), "{rows:?}");
    // 80 columns still carries every column of the table.
    assert!(contains(&rows, "SOURCE"), "{rows:?}");
    assert!(contains(&rows, "OTHER"), "{rows:?}");
}

/// The scope selector and the status line keep their content and lose their
/// boxes on a short frame, and the Tab hint moves onto the line with them.
#[test]
fn a_short_frame_drops_the_chrome_boxes_not_the_chrome() {
    let (_scratch, state) = state();
    let rows = screen_sized(&state, 160, 24);

    assert!(contains(&rows, "USER · global"), "{rows:?}");
    assert!(contains(&rows, "Ready. Edit a value"), "{rows:?}");
    // The hint the bottom border carried rides on the line instead.
    assert!(contains(&rows, "Tab switches scope"), "{rows:?}");
    // The boxes themselves are gone. Matched on the border glyph, since the
    // footer legitimately contains the word "scope".
    assert!(!contains(&rows, "╭ scope"), "{rows:?}");
    assert!(!contains(&rows, "╭ status"), "{rows:?}");
    // The table keeps its box: it is the one thing on screen with an edge
    // worth drawing.
    assert!(contains(&rows, "╭ keys"), "{rows:?}");
}

/// The longest key in the registry renders in full at 80 columns.
///
/// This is the defect dropping the section prefix was meant to fix, so it is
/// worth pinning against the registry rather than against a name: the key that
/// is longest changes as keys are added, and a truncated key is silent.
#[test]
fn the_longest_key_is_not_truncated_at_eighty_columns() {
    let (_scratch, mut state) = state();
    let longest = KEYS
        .iter()
        .max_by_key(|spec| spec.field.len())
        .expect("the registry is never empty");
    focus_by_name(&mut state, longest.name);
    let rows = screen_sized(&state, 80, 24);

    // Matched on the selected TABLE row specifically: the inspector's title
    // carries the field name too, and would mask a truncated cell.
    assert!(
        rows.iter()
            .any(|row| row.starts_with("│▸") && row.contains(longest.field)),
        "{longest:?} truncated in {rows:?}"
    );
}

/// A cramped frame drops columns in order — OTHER first, then SOURCE — and
/// never the key or its value.
#[test]
fn a_cramped_frame_drops_columns_rather_than_information() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "terminal.backend");

    // Inner width 54: past OTHER's floor, still above SOURCE's.
    let rows = screen_sized(&state, 56, 16);
    assert!(contains(&rows, "backend"), "{rows:?}");
    assert!(contains(&rows, "‹ native ›"), "{rows:?}");
    assert!(contains(&rows, "SOURCE"), "{rows:?}");
    assert!(!contains(&rows, "OTHER"), "{rows:?}");

    // Inner width 46: SOURCE goes too. KEY and VALUE never do.
    let rows = screen_sized(&state, 48, 16);
    assert!(contains(&rows, "backend"), "{rows:?}");
    assert!(contains(&rows, "‹ native ›"), "{rows:?}");
    assert!(!contains(&rows, "SOURCE"), "{rows:?}");
    // The logo has collapsed by this height, but the screen still names itself.
    assert!(contains(&rows, "configuration"), "{rows:?}");
}

/// The strip's own last four rows at height 16 (below `INSPECTOR_HEIGHT`'s
/// border, help line, facts line, border) — sliced off so a check against the
/// facts line cannot be satisfied by `table::render`'s OTHER column, which
/// prints the same `user`/`project` words at some of these widths too.
fn strip_rows(state: &ConfigState, width: u16) -> Vec<String> {
    let rows = screen_sized(state, width, 16);
    rows[rows.len() - 4..].to_vec()
}

/// The write segment — `s → <file>` — is the one fact on the strip that must
/// never drop, because it is what makes pressing `s` not a guess. It has to
/// survive at every width the table's own cramped-frame coverage uses, and
/// at the narrower point right around where the fixed facts alone fill the
/// line.
#[test]
fn the_inspector_strip_never_drops_the_write_segment() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "terminal.backend");

    for width in [80, 67, 63, 56, 48] {
        let rows = strip_rows(&state, width);
        assert!(contains(&rows, "s →"), "width {width}: {rows:?}");
    }
}

/// When the strip cannot hold everything, built-in gives way before either
/// tier does — dropping a tier first would cost the operator a fact `s →`
/// exists specifically to keep them from having to open the file to check.
#[test]
fn built_in_gives_way_before_a_tier_does() {
    let (_scratch, mut state) = state();
    focus_by_name(&mut state, "terminal.backend");

    let widest_without_built_in = (40..=100)
        .rev()
        .find(|&width| !contains(&strip_rows(&state, width), "built-in"));
    let widest_without_a_tier = (40..=100)
        .rev()
        .find(|&width| !contains(&strip_rows(&state, width), "user"));

    if let (Some(built_in_gone), Some(tier_gone)) = (widest_without_built_in, widest_without_a_tier)
    {
        assert!(
            built_in_gone >= tier_gone,
            "the user tier (gone by width {tier_gone}) gave way before built-in (gone by width {built_in_gone})"
        );
    }
}
