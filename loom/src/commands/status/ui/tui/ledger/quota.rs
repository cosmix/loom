//! The provider quota line in the ledger footer: one meter per usage window with its
//! reset countdown, and a right-aligned note when a provider's data is stale or errored.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::text::{cut_line, spans_width, text_width, truncate};
use super::FULL_WIDTH;
use crate::commands::status::ui::theme::Theme;
use crate::quota::{
    age_secs, format_reset, reset_text, ProviderQuota, QuotaSnapshot, QuotaWindow, WindowKind,
    STALE_AFTER_SECS,
};

/// Narrowest width at which six-segment bars and countdowns are laid out.
pub const MEDIUM_WIDTH: u16 = 90;

/// Cells kept clear between the meters and the suffixes, and between suffixes.
const SUFFIX_GAP: usize = 2;
/// Fewest cells a suffix is shown truncated into rather than dropped.
const SUFFIX_MIN_CELLS: usize = 16;

/// How much of each window the quota line shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaLayout {
    /// Bar length in cells; `0` renders the percentage alone.
    pub bar: u16,
    /// Whether the reset countdown follows the percentage.
    pub countdown: bool,
}

const FULL: QuotaLayout = QuotaLayout {
    bar: 8,
    countdown: true,
};
const MEDIUM: QuotaLayout = QuotaLayout {
    bar: 6,
    countdown: true,
};
const COMPACT: QuotaLayout = QuotaLayout {
    bar: 0,
    countdown: true,
};
const NARROW: QuotaLayout = QuotaLayout {
    bar: 0,
    countdown: false,
};

/// Every layout the line can fall back to, richest first.
const LAYOUTS: [QuotaLayout; 4] = [FULL, MEDIUM, COMPACT, NARROW];

/// Whether either provider has cached quota data to show.
pub fn has_quota(snapshot: &QuotaSnapshot) -> bool {
    snapshot.claude.is_some() || snapshot.codex.is_some()
}

/// The richest layout a terminal of `width` cells is allowed to show.
pub fn quota_layout(width: u16) -> QuotaLayout {
    if width >= FULL_WIDTH {
        FULL
    } else if width >= MEDIUM_WIDTH {
        MEDIUM
    } else {
        NARROW
    }
}

/// Build the quota line for `width` cells: the meters in the richest layout that
/// fits the data, then the stale and error notes that fit after them.
pub fn quota_line(snapshot: &QuotaSnapshot, now: i64, width: u16) -> Line<'static> {
    let mut spans = fitting_meters(snapshot, now, width);
    append_suffixes(&mut spans, suffix_texts(snapshot, now), width);
    cut_line(Line::from(spans), width)
}

/// The meters in the richest layout at or below the width's tier whose content fits;
/// the narrowest layout when nothing does.
fn fitting_meters(snapshot: &QuotaSnapshot, now: i64, width: u16) -> Vec<Span<'static>> {
    let ceiling = quota_layout(width);
    let mut spans = Vec::new();
    for layout in LAYOUTS
        .into_iter()
        .filter(|layout| layout.bar <= ceiling.bar && (ceiling.countdown || !layout.countdown))
    {
        spans = meter_spans(snapshot, now, layout);
        if spans_width(&spans) <= usize::from(width) {
            break;
        }
    }
    spans
}

fn meter_spans(snapshot: &QuotaSnapshot, now: i64, layout: QuotaLayout) -> Vec<Span<'static>> {
    let mut spans = vec![Span::raw(" ")];
    let providers = [("claude", &snapshot.claude), ("codex", &snapshot.codex)]
        .into_iter()
        .filter_map(|(name, quota)| quota.as_ref().map(|quota| (name, quota)));
    for (index, (name, quota)) in providers.enumerate() {
        if index > 0 {
            spans.push(Span::styled(" │ ", Theme::dimmed()));
        }
        spans.extend(provider_spans(name, quota, now, layout));
    }
    spans
}

fn provider_spans(
    name: &str,
    quota: &ProviderQuota,
    now: i64,
    layout: QuotaLayout,
) -> Vec<Span<'static>> {
    let mut spans = vec![
        Span::styled(name.to_owned(), Theme::header()),
        Span::raw(" "),
    ];
    for (index, kind) in [WindowKind::FiveHour, WindowKind::SevenDay]
        .into_iter()
        .enumerate()
    {
        if index > 0 {
            spans.push(window_join(layout));
        }
        let window = quota.windows.iter().find(|window| window.kind == kind);
        spans.extend(window_spans(kind, window, now, layout));
    }
    spans
}

/// Bars and countdowns separate the windows on their own; bare percentages need a dot.
fn window_join(layout: QuotaLayout) -> Span<'static> {
    if layout == NARROW {
        Span::styled(" · ", Theme::dimmed())
    } else {
        Span::raw("  ")
    }
}

/// One window slot: label, then bar, percentage and countdown as the layout allows,
/// or a dash when the provider reported no such window.
fn window_spans(
    kind: WindowKind,
    window: Option<&QuotaWindow>,
    now: i64,
    layout: QuotaLayout,
) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(format!("{} ", kind.label()), Theme::dimmed())];
    let Some(window) = window else {
        spans.push(Span::styled("—", Theme::dimmed()));
        return spans;
    };
    let percent = window.used_percent.round() as u32;
    let style = Theme::context_style(percent, 100);
    if layout.bar > 0 {
        spans.extend(bar_spans(window.used_percent, layout.bar, style));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(format!("{percent}%"), style));
    if layout.countdown {
        if let Some(reset) = reset_text(window.resets_at, now) {
            spans.push(Span::styled(format!(" · {reset}"), Theme::dimmed()));
        }
    }
    spans
}

fn bar_spans(used_percent: f64, bar: u16, style: Style) -> Vec<Span<'static>> {
    let filled = ((used_percent / 100.0 * f64::from(bar)).round() as u16).min(bar);
    vec![
        Span::styled("━".repeat(usize::from(filled)), style),
        Span::styled("╌".repeat(usize::from(bar - filled)), Theme::dimmed()),
    ]
}

/// Stale and error notes, claude first, in the order they are offered to the line.
fn suffix_texts(snapshot: &QuotaSnapshot, now: i64) -> Vec<String> {
    let mut texts = Vec::new();
    for (name, quota) in [("claude", &snapshot.claude), ("codex", &snapshot.codex)] {
        let Some(quota) = quota else {
            continue;
        };
        let age = age_secs(quota.observed_at, now);
        if age >= STALE_AFTER_SECS {
            texts.push(format!("· {name} {} old", age_text(age)));
        }
        if let Some(error) = &quota.error {
            texts.push(format!("· {name}: {error}"));
        }
    }
    texts
}

fn age_text(age: i64) -> String {
    if age < 86_400 {
        format!("{}m", age / 60)
    } else {
        format_reset(age)
    }
}

/// Append the suffixes that fit after the meters, right-aligned to `width`. A suffix
/// that overflows is shown truncated when at least `SUFFIX_MIN_CELLS` remain; the
/// meters are never cut to make room.
fn append_suffixes(spans: &mut Vec<Span<'static>>, texts: Vec<String>, width: u16) {
    let mut room = usize::from(width).saturating_sub(spans_width(spans));
    let mut shown = Vec::new();
    for text in texts {
        let available = room.saturating_sub(SUFFIX_GAP);
        let needed = text_width(&text);
        if needed <= available {
            room -= SUFFIX_GAP + needed;
            shown.push(text);
        } else if available >= SUFFIX_MIN_CELLS {
            room = 0;
            shown.push(truncate(&text, available));
            break;
        } else {
            break;
        }
    }
    if shown.is_empty() {
        return;
    }
    spans.push(Span::raw(" ".repeat(room + SUFFIX_GAP)));
    spans.push(Span::styled(
        shown.join(&" ".repeat(SUFFIX_GAP)),
        Theme::dimmed(),
    ));
}

#[cfg(test)]
#[path = "quota_tests.rs"]
mod tests;
