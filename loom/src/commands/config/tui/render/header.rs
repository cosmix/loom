//! The logo band and the scope selector.
//!
//! The selector is the answer to "it is not clear how to set the global and
//! the project-scoped settings": both files are named, on screen, all the
//! time, with the active one marked — a settings screen that hides which file
//! it writes is a settings screen that gets ignored.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::super::state::{ConfigState, Scope, SCOPES};
use super::super::theme;
use crate::commands::status::ui::tui::ledger::text::{cut_line, spans_width, text_width};
use crate::user_config::keys::KEYS;

/// The column the logo occupies, matching the status ledger's banner so the
/// two screens line up when an operator flips between them.
const LOGO_COLUMN: usize = 19;
/// The gap that keeps the two scope tabs from reading as one run of text.
const TAB_GAP: usize = 4;
/// The hint that names the key, for the unbordered selector that has no bottom
/// border to carry it.
const SCOPE_HINT: &str = "Tab switches scope ";
/// What the project tab says in a tree that has no workspace, in place of a
/// path.
const NO_WORKSPACE_DETAIL: &str = "no .loom/work — run loom init";

/// Render the logo band, collapsed to one line when the frame is short.
pub(super) fn render_logo(frame: &mut Frame, area: Rect, tall: bool) {
    let lines = if tall {
        band(area.width)
    } else {
        vec![compact(area.width)]
    };
    frame.render_widget(Paragraph::new(lines), area);
}

/// The four logo rows plus the warp rule that closes the band.
fn band(width: u16) -> Vec<Line<'static>> {
    let logos: Vec<&str> = crate::LOGO.lines().collect();
    // Never indexed: a banner edited down to three lines must degrade to a
    // blank row, not panic on the next render.
    let logo = |n: usize| logos.get(n).copied().unwrap_or("");
    vec![
        logo_line(logo(0), Vec::new(), width),
        title_line(logo(1), width),
        logo_line(
            logo(2),
            vec![Span::styled(counts(), theme::secondary())],
            width,
        ),
        logo_line(logo(3), Vec::new(), width),
        warp_rule(width),
    ]
}

/// The screen's name, with the version right-aligned to the frame edge.
fn title_line(logo: &str, width: u16) -> Line<'static> {
    let mut spans = logo_spans(logo);
    spans.push(Span::styled("configuration", theme::emphasis(theme::MIST)));
    let label = Span::styled(crate::version::LABEL, theme::secondary());
    let room = usize::from(width);
    let used = spans_width(&spans) + label.width();
    if used < room {
        spans.push(Span::raw(" ".repeat(room - used)));
        spans.push(label);
    }
    cut_line(Line::from(spans), width)
}

/// One band row: the logo column, then whatever the row carries beside it.
fn logo_line(logo: &str, mut rest: Vec<Span<'static>>, width: u16) -> Line<'static> {
    let mut spans = logo_spans(logo);
    spans.append(&mut rest);
    cut_line(Line::from(spans), width)
}

/// The logo itself, padded out to its column by display width rather than by
/// character count — the banner is box-drawing glyphs, not ASCII.
fn logo_spans(logo: &str) -> Vec<Span<'static>> {
    let padding = LOGO_COLUMN.saturating_sub(text_width(logo));
    vec![
        Span::styled(logo.to_owned(), theme::emphasis(theme::WARP)),
        Span::raw(" ".repeat(padding)),
    ]
}

/// Both counts read off the registry at runtime: a hardcoded pair would be
/// wrong the first time a key is added, and nothing would catch it.
fn counts() -> String {
    let mut sections: Vec<&str> = KEYS.iter().map(|spec| spec.section).collect();
    sections.sort_unstable();
    sections.dedup();
    format!("{} keys · {} sections", KEYS.len(), sections.len())
}

/// The rule closing the band: warp threads crossing it at every sixth column.
fn warp_rule(width: u16) -> Line<'static> {
    let mut spans = Vec::new();
    let mut run = String::new();
    for column in 0..usize::from(width) {
        if column > 0 && column % 6 == 0 {
            if !run.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut run), theme::rule()));
            }
            spans.push(Span::styled("╪", theme::plain(theme::WARP)));
        } else {
            run.push('─');
        }
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, theme::rule()));
    }
    cut_line(Line::from(spans), width)
}

/// The one-line band for a frame too short to hold the logo. The logo is what
/// the operator asked for, so it goes only when the terminal cannot hold it.
fn compact(width: u16) -> Line<'static> {
    cut_line(
        Line::from(vec![
            Span::styled("loom", theme::emphasis(theme::WARP)),
            Span::styled(" │ ", theme::rule()),
            Span::styled("configuration", theme::primary()),
            Span::styled(format!("  {}", counts()), theme::secondary()),
        ]),
        width,
    )
}

/// Render the scope selector: both tiers, both paths, the active one marked.
///
/// A short frame spends its borders before it spends its content, so the box
/// goes and the line stays. The hint the bottom border carried moves onto the
/// line itself, since losing the box must not lose the answer to "how do I
/// reach the other tier".
pub(super) fn render_scope(frame: &mut Frame, area: Rect, state: &ConfigState, bordered: bool) {
    if !bordered {
        frame.render_widget(Paragraph::new(scope_line(state, area.width, true)), area);
        return;
    }
    let block = theme::block(theme::chrome_title("scope")).title_bottom(
        Line::from(Span::styled(format!(" {SCOPE_HINT}"), theme::secondary())).right_aligned(),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(scope_line(state, inner.width, false)), inner);
}

/// The two tabs, sharing whatever width is left after their fixed parts.
///
/// The hint takes only leftover room and is never reserved for: the paths are
/// what the operator needs, the footer names `⇥ scope` at every width anyway,
/// and space held back for a hint that then does not fit would shorten both
/// paths for nothing.
fn scope_line(state: &ConfigState, width: u16, with_hint: bool) -> Line<'static> {
    let paths = [Some(state.user_path()), state.project_path()];
    let fixed: usize = SCOPES
        .iter()
        .map(|scope| MARKER_WIDTH + text_width(scope.tab_label()) + 2)
        .sum::<usize>()
        + TAB_GAP;
    let share = usize::from(width).saturating_sub(fixed) / 2;
    let mut spans = Vec::new();
    for (index, scope) in SCOPES.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" ".repeat(TAB_GAP)));
        }
        spans.extend(tab_spans(
            scope,
            state.scope() == scope,
            paths[index],
            share,
        ));
    }
    if with_hint {
        let hint = Span::styled(SCOPE_HINT, theme::secondary());
        let room = usize::from(width);
        let used = spans_width(&spans) + hint.width();
        if used < room {
            spans.push(Span::raw(" ".repeat(room - used)));
            spans.push(hint);
        }
    }
    cut_line(Line::from(spans), width)
}

/// The marker cell every tab starts with, active or not.
const MARKER_WIDTH: usize = 2;

/// One tab: a marker, the tier's label, and the file it writes.
fn tab_spans(scope: Scope, active: bool, path: Option<&str>, share: usize) -> Vec<Span<'static>> {
    let missing = path.is_none();
    let color = if missing {
        theme::EMBER
    } else {
        theme::scope_color(scope)
    };
    let label_style = if active {
        theme::emphasis(color)
    } else {
        theme::plain(if missing { theme::EMBER } else { theme::SHADE })
    };
    let (label, detail, detail_style) = match path {
        // A config path's TAIL is what identifies it, so the head is what goes
        // when the line cannot hold the whole thing.
        Some(path) => (
            scope.tab_label().to_owned(),
            super::tail(path, share),
            if active {
                theme::primary()
            } else {
                theme::secondary()
            },
        ),
        None => (
            "PROJECT · unavailable".to_owned(),
            // Degrades to its first half rather than to an ellipsis: cutting
            // the fix off as "no .loom/work — r…" tells the operator less
            // than naming only what is missing.
            if text_width(NO_WORKSPACE_DETAIL) <= share {
                NO_WORKSPACE_DETAIL.to_owned()
            } else {
                "no .loom/work".to_owned()
            },
            theme::secondary(),
        ),
    };
    let mut spans = vec![
        Span::styled(if active { "▌ " } else { "  " }, theme::plain(color)),
        Span::styled(label, label_style),
        Span::raw("  "),
        Span::styled(detail, detail_style),
    ];
    if active {
        for span in &mut spans {
            span.style = span.style.bg(theme::SELECT_BG);
        }
    }
    spans
}
