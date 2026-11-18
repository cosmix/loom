//! Live token flow pane and compact, cell-safe text helpers.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use chrono::{DateTime, Utc};

use super::text::{cut_line, padded, spans_width};
use super::tokens_columns::Columns;
use super::tokens_jobs::{has_open_job, job_of, jobs_of, model_text, state_text, unmatched};
use super::tokens_models::{model_lines, MODEL_BLOCK_MAX_ROWS};
use crate::commands::status::data::{CodexJob, StageSummary};
use crate::commands::status::ui::theme::Theme;
use crate::tokens::{AgentFlow, AgentKind, QuotaLeft, StageFlow, TokenFlow};

/// Most rows the live part (title, stage and agent lines) uses.
const LIVE_MAX_ROWS: usize = 16;
/// The live rows, a blank line and the by-model block.
pub(super) const PANE_MAX_ROWS: u16 = (LIVE_MAX_ROWS + 1 + MODEL_BLOCK_MAX_ROWS) as u16;
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

/// The live rows (at most [`LIVE_MAX_ROWS`]), a blank line and the by-model block,
/// cut to `max_rows` from the bottom.
pub(super) fn tokens_pane_lines(
    flow: &TokenFlow,
    stages: &[StageSummary],
    width: u16,
    max_rows: u16,
) -> Vec<Line<'static>> {
    if max_rows == 0 {
        return Vec::new();
    }
    let mut lines = live_lines(flow, stages, width);
    let models = model_lines(&flow.models, width);
    if !models.is_empty() {
        lines.push(Line::default());
        lines.extend(models);
    }
    lines.truncate(usize::from(max_rows));
    lines
}

/// The title and the stage and agent rows of the stages with a live agent or a Codex job
/// still running that no agent row reads: the part of the pane that stays when the by-model block does not
/// fit. `stages` holds the Codex jobs.
pub(super) fn live_lines(
    flow: &TokenFlow,
    stages: &[StageSummary],
    width: u16,
) -> Vec<Line<'static>> {
    let now = Utc::now();
    let active: Vec<&StageFlow> = flow
        .stages
        .iter()
        .filter(|stage| {
            stage.agents.iter().any(|agent| agent.active)
                || has_open_job(stage, jobs_of(stages, &stage.stage_id))
        })
        .collect();
    if active.is_empty() {
        return vec![title_line(width, Some("no active agent"))];
    }
    let columns = Columns::for_stages(width, &active, stages, now);
    let mut lines = vec![title_line(width, None)];
    for stage in active {
        if lines.len() >= LIVE_MAX_ROWS {
            break;
        }
        lines.push(stage_line(stage, &columns, width));
        // Shade against the peak of the cells the strips show.
        let max = tail(&stage.series, usize::from(columns.strip))
            .iter()
            .copied()
            .max()
            .unwrap_or(0);
        let jobs = jobs_of(stages, &stage.stage_id);
        for agent in &stage.agents {
            if lines.len() >= LIVE_MAX_ROWS {
                break;
            }
            let job = job_of(agent, jobs);
            lines.push(agent_line(agent, job, now, max, &columns, width));
        }
        for job in unmatched(stage, jobs) {
            if lines.len() >= LIVE_MAX_ROWS {
                break;
            }
            lines.push(job_line(job, now, &columns, width));
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
        AgentKind::Codex => match agent.unit_id.as_deref() {
            Some(unit) => format!("codex {unit}"),
            None => {
                let id = agent.id.strip_prefix("codex-").unwrap_or(&agent.id);
                format!("codex {}", id.chars().take(4).collect::<String>())
            }
        },
    }
}

fn agent_line(
    agent: &AgentFlow,
    job: Option<&CodexJob>,
    now: DateTime<Utc>,
    max: u64,
    columns: &Columns,
    width: u16,
) -> Line<'static> {
    let model_style = if agent.kind == AgentKind::Codex {
        Theme::dimmed().fg(Color::Magenta)
    } else {
        Theme::dimmed()
    };
    let (state, state_style) = match (job, agent.active) {
        (Some(job), _) => state_text(job, now),
        (None, true) => ("live".into(), Style::default().fg(Color::Blue)),
        (None, false) => ("idle".into(), Theme::dimmed()),
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
            padded(
                &model_text(agent.model.as_deref(), agent.effort.as_deref()),
                columns.model,
            ),
            model_style,
        ),
        Span::raw(" "),
        Span::styled(padded(&state, columns.state), state_style),
        Span::raw(strip),
    ];
    right_aligned(spans, Span::raw(compact_tokens(agent.total.fresh())), width)
}

/// A Codex job no agent row reads, with no tokens: the unit that failed before its thread
/// started, or whose rollout has not been found.
fn job_line(job: &CodexJob, now: DateTime<Utc>, columns: &Columns, width: u16) -> Line<'static> {
    let (state, state_style) = state_text(job, now);
    let spans = vec![
        Span::raw("  "),
        Span::raw(padded(&format!("codex {}", job.unit_id), columns.label)),
        Span::raw(" "),
        Span::styled(
            padded(
                &model_text(Some(&job.model), Some(&job.effort)),
                columns.model,
            ),
            Theme::dimmed().fg(Color::Magenta),
        ),
        Span::raw(" "),
        Span::styled(padded(&state, columns.state), state_style),
        Span::raw(" ".repeat(usize::from(columns.strip))),
    ];
    right_aligned(spans, Span::raw("—"), width)
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

#[cfg(test)]
#[path = "tokens_pane_models_tests.rs"]
mod models_tests;
