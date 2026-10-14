//! Live token flow pane and compact, cell-safe text helpers.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use super::text::{cut_line, padded, spans_width};
use crate::commands::status::ui::theme::Theme;
use crate::tokens::{AgentFlow, AgentKind, QuotaLeft, StageFlow, TokenFlow};

pub(super) const PANE_MAX_ROWS: u16 = 16;
/// Preferred and smallest cell widths of the series strip, the agent label and the
/// model column.
const SERIES_CELLS: u16 = 48;
const SERIES_MIN: u16 = 12;
const LABEL_CELLS: u16 = 30;
const LABEL_MIN: u16 = 12;
const MODEL_CELLS: u16 = 24;
const MODEL_MIN: u16 = 8;
/// Cells an agent row spends outside those three columns and its total: the two-space
/// indent, the two gaps, the 5-cell state and the gap before the total.
const AGENT_FIXED: u16 = 2 + 1 + 1 + 5 + 1;
/// Cells a stage line spends outside its id, sparkline and rate: the indent and the
/// ` {rate}/m  ` text around the rate.
const STAGE_FIXED: u16 = 1 + 1 + 2 + 2;
/// The agent total and stage rate slots are at least this many cells wide.
const SLOT_MIN: u16 = 5;

pub(super) fn compact_tokens(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=99_999 => format!("{}.{:01}k", n / 1_000, (n % 1_000) / 100),
        100_000..=999_999 => format!("{}k", n / 1_000),
        _ => format!("{}.{:02}M", n / 1_000_000, (n % 1_000_000) / 10_000),
    }
}

pub(super) fn spark(values: &[u64], cells: usize) -> String {
    let values = tail(values, cells);
    let max = values.iter().copied().max().unwrap_or(0);
    let mut result = " ".repeat(cells - values.len());
    for &value in values {
        let level = intensity(value, max, 8);
        result.push(if level == 0 {
            ' '
        } else {
            ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'][level - 1]
        });
    }
    result
}

pub(super) fn shade_strip(values: &[u64], max: u64) -> String {
    values
        .iter()
        .map(|&value| [' ', '░', '▒', '▓', '█'][intensity(value, max, 4)])
        .collect()
}

fn intensity(value: u64, max: u64, levels: usize) -> usize {
    if value == 0 || max == 0 {
        return 0;
    }
    ((u128::from(value.min(max)) * levels as u128).div_ceil(u128::from(max))) as usize
}

fn tail(values: &[u64], cells: usize) -> &[u64] {
    &values[values.len().saturating_sub(cells)..]
}

pub(super) fn quota_left_text(left: &QuotaLeft) -> Option<(String, Style)> {
    if left.lasts_to_reset {
        return Some(("lasts to reset".into(), Theme::dimmed()));
    }
    let minutes = left.minutes_left?;
    let text = if minutes >= 60 {
        format!("~{}h{:02}m left", minutes / 60, minutes % 60)
    } else {
        format!("~{minutes}m left")
    };
    let color = if minutes < 30 {
        Color::Red
    } else {
        Color::Yellow
    };
    Some((text, Style::default().fg(color)))
}

/// Cell widths of the pane's columns at one pane width.
struct Columns {
    stage_id: u16,
    rate: u16,
    spark: u16,
    label: u16,
    model: u16,
    strip: u16,
}

impl Columns {
    /// The columns for the rows of `stages`, with the slots the widest agent total and
    /// stage rate among them need.
    fn for_stages(width: u16, stages: &[&StageFlow]) -> Self {
        let totals = stages.iter().flat_map(|stage| &stage.agents);
        Self::fit(
            width,
            slot_cells(totals.map(|agent| agent.total.fresh())),
            slot_cells(stages.iter().map(|stage| stage.rate.fresh())),
        )
    }

    /// Every row of the pane uses these widths, so the strips line up. `total` and `rate`
    /// are the cells the widest agent total and stage rate need, so a long number never
    /// pushes the newest cell of a strip off the row. The model column gives way first,
    /// then the strip, then the label; each stops at its minimum.
    fn fit(width: u16, total: u16, rate: u16) -> Self {
        let [model, strip, label] = shrink(
            width.saturating_sub(AGENT_FIXED + total),
            [
                (MODEL_CELLS, MODEL_MIN),
                (SERIES_CELLS, SERIES_MIN),
                (LABEL_CELLS, LABEL_MIN),
            ],
        );
        let [spark, stage_id] = shrink(
            width.saturating_sub(STAGE_FIXED + rate),
            [(SERIES_CELLS, SERIES_MIN), (LABEL_CELLS, LABEL_MIN)],
        );
        Self {
            stage_id,
            rate,
            spark,
            label,
            model,
            strip,
        }
    }
}

/// Cells the widest of `values` needs once compacted, at least [`SLOT_MIN`].
fn slot_cells(values: impl Iterator<Item = u64>) -> u16 {
    // `compact_tokens` is ASCII and at most 18 cells, so the cast is exact.
    let widest = values.map(|n| compact_tokens(n).len() as u16).max();
    widest.unwrap_or(0).max(SLOT_MIN)
}

/// Fit `(preferred, minimum)` columns into `room` cells. They shrink in array order,
/// each no further than its minimum.
fn shrink<const N: usize>(room: u16, columns: [(u16, u16); N]) -> [u16; N] {
    let wanted: u16 = columns.iter().map(|&(preferred, _)| preferred).sum();
    let mut excess = wanted.saturating_sub(room);
    columns.map(|(preferred, minimum)| {
        let cut = excess.min(preferred - minimum);
        excess -= cut;
        preferred - cut
    })
}

pub(super) fn tokens_pane_lines(flow: &TokenFlow, width: u16, max_rows: u16) -> Vec<Line<'static>> {
    if max_rows == 0 {
        return Vec::new();
    }
    let active: Vec<&StageFlow> = flow
        .stages
        .iter()
        .filter(|stage| stage.agents.iter().any(|agent| agent.active))
        .collect();
    if active.is_empty() {
        return vec![title_line(width, Some("no active agent"))];
    }
    let columns = Columns::for_stages(width, &active);
    let mut lines = vec![title_line(width, None)];
    for stage in active {
        if lines.len() >= usize::from(max_rows) {
            break;
        }
        lines.push(stage_line(stage, &columns, width));
        // Shade against the peak of the cells the strips show.
        let max = tail(&stage.series, usize::from(columns.strip))
            .iter()
            .copied()
            .max()
            .unwrap_or(0);
        for agent in &stage.agents {
            if lines.len() >= usize::from(max_rows) {
                break;
            }
            lines.push(agent_line(agent, max, &columns, width));
        }
    }
    lines
}

/// The pane while the toggle is on but no token flow has been read yet.
pub(super) fn no_data_lines(width: u16, max_rows: u16) -> Vec<Line<'static>> {
    placeholder_lines("no token data yet", width, max_rows)
}

/// The title row and one dim line saying why the pane has no rows.
fn placeholder_lines(message: &str, width: u16, max_rows: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if max_rows > 0 {
        lines.push(title_line(width, None));
    }
    if max_rows > 1 {
        let note = Line::from(Span::styled(format!("  {message}"), Theme::dimmed()));
        lines.push(cut_line(note, width));
    }
    lines
}

/// The pane's title row. `note` is placed dimmed before the legend when the pane has no
/// rows to show, so `right_aligned` cuts the legend first on a narrow pane.
fn title_line(width: u16, note: Option<&str>) -> Line<'static> {
    let mut spans = vec![Span::styled(
        " TOKENS",
        Theme::status_pending().add_modifier(Modifier::BOLD),
    )];
    if let Some(note) = note {
        spans.push(Span::styled(format!(" · {note}"), Theme::dimmed()));
    }
    spans.push(Span::styled(
        " · fresh per 30 s cell · ░▒▓█ low to high",
        Theme::dimmed(),
    ));
    right_aligned(spans, Span::styled("t close", Theme::dimmed()), width)
}

fn stage_line(stage: &StageFlow, columns: &Columns, width: u16) -> Line<'static> {
    cut_line(
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                padded(&stage.stage_id, columns.stage_id),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                " {:>rate$}/m  ",
                compact_tokens(stage.rate.fresh()),
                rate = usize::from(columns.rate)
            )),
            Span::raw(spark(&stage.series, usize::from(columns.spark))),
        ]),
        width,
    )
}

fn agent_label(agent: &AgentFlow) -> String {
    match agent.kind {
        AgentKind::Main => "main".into(),
        AgentKind::Subagent => {
            let kind = agent.agent_type.as_deref().unwrap_or("subagent");
            let kind = kind.strip_prefix("loom-").unwrap_or(kind);
            format!("{} {}", kind, agent.id.chars().take(4).collect::<String>())
        }
        AgentKind::Codex => {
            let id = agent.id.strip_prefix("codex-").unwrap_or(&agent.id);
            format!("codex {}", id.chars().take(4).collect::<String>())
        }
    }
}

fn agent_line(agent: &AgentFlow, max: u64, columns: &Columns, width: u16) -> Line<'static> {
    let model_style = if agent.kind == AgentKind::Codex {
        Theme::dimmed().fg(Color::Magenta)
    } else {
        Theme::dimmed()
    };
    let (state, state_style) = if agent.active {
        ("live ", Style::default().fg(Color::Blue))
    } else {
        ("idle ", Theme::dimmed())
    };
    // The newest cells always: an older series is cut from its left end.
    let values = tail(&agent.series, usize::from(columns.strip));
    let strip = format!(
        "{}{}",
        " ".repeat(usize::from(columns.strip) - values.len()),
        shade_strip(values, max)
    );
    let spans = vec![
        Span::raw("  "),
        Span::raw(padded(&agent_label(agent), columns.label)),
        Span::raw(" "),
        Span::styled(
            padded(agent.model.as_deref().unwrap_or("—"), columns.model),
            model_style,
        ),
        Span::raw(" "),
        Span::styled(state, state_style),
        Span::raw(strip),
    ];
    right_aligned(spans, Span::raw(compact_tokens(agent.total.fresh())), width)
}

fn right_aligned(spans: Vec<Span<'static>>, right: Span<'static>, width: u16) -> Line<'static> {
    let left_width = usize::from(width).saturating_sub(right.width() + 1);
    let mut line = cut_line(Line::from(spans), left_width as u16);
    let padding = usize::from(width).saturating_sub(spans_width(&line.spans) + right.width());
    line.spans.push(Span::raw(" ".repeat(padding)));
    line.spans.push(right);
    cut_line(line, width)
}

#[cfg(test)]
#[path = "tokens_pane_tests.rs"]
mod tests;
