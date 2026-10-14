//! Live token flow pane and compact, cell-safe text helpers.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use super::text::{cut_line, padded, spans_width};
use crate::commands::status::ui::theme::Theme;
use crate::tokens::{AgentFlow, AgentKind, QuotaLeft, StageFlow, TokenFlow};

pub(super) const PANE_MAX_ROWS: u16 = 16;
const SERIES_CELLS: usize = 48;

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

pub(super) fn tokens_pane_lines(flow: &TokenFlow, width: u16, max_rows: u16) -> Vec<Line<'static>> {
    if max_rows == 0 {
        return Vec::new();
    }
    let mut lines = vec![title_line(width)];
    for stage in &flow.stages {
        if !stage.agents.iter().any(|agent| agent.active) {
            continue;
        }
        if lines.len() >= usize::from(max_rows) {
            break;
        }
        lines.push(stage_line(stage, width));
        let max = tail(&stage.series, SERIES_CELLS)
            .iter()
            .copied()
            .max()
            .unwrap_or(0);
        for agent in &stage.agents {
            if lines.len() >= usize::from(max_rows) {
                break;
            }
            lines.push(agent_line(agent, max, width));
        }
    }
    lines
}

fn title_line(width: u16) -> Line<'static> {
    let spans = vec![
        Span::styled(
            " TOKENS",
            Theme::status_pending().add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · fresh per 30 s cell · ░▒▓█ low to high", Theme::dimmed()),
    ];
    right_aligned(spans, Span::styled("t close", Theme::dimmed()), width)
}

fn stage_line(stage: &StageFlow, width: u16) -> Line<'static> {
    cut_line(
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                padded(&stage.stage_id, 30),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" {:>5}/m  ", compact_tokens(stage.rate.fresh()))),
            Span::raw(spark(&stage.series, SERIES_CELLS)),
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

fn agent_line(agent: &AgentFlow, max: u64, width: u16) -> Line<'static> {
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
    let values = tail(&agent.series, SERIES_CELLS);
    let strip = format!(
        "{}{}",
        " ".repeat(SERIES_CELLS - values.len()),
        shade_strip(values, max)
    );
    let spans = vec![
        Span::raw("  "),
        Span::raw(padded(&agent_label(agent), 30)),
        Span::raw(" "),
        Span::styled(
            padded(agent.model.as_deref().unwrap_or("—"), 24),
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
