use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::text::{cut_line, spans_width, text_width};
use super::LedgerView;
use crate::commands::status::ui::theme::{StatusColors, Theme};
use crate::models::stage::StageStatus;

/// Render the four-line ledger header.
pub fn render_header(frame: &mut Frame, area: Rect, view: &LedgerView) {
    frame.render_widget(Paragraph::new(header_lines(view, area.width)), area);
}

fn header_lines(view: &LedgerView, width: u16) -> Vec<Line<'static>> {
    let data = view.data;
    let queued = count_status(&data.stages, StageStatus::Queued);
    let waiting = count_status(&data.stages, StageStatus::WaitingForDeps);
    let logos: Vec<_> = crate::LOGO.lines().collect();
    vec![
        top_line(
            logos[0],
            data.plan_name.as_deref(),
            view.tick_age_secs,
            width,
        ),
        logo_line(
            logos[1],
            progress_line(
                view.spinner,
                data.progress.completed,
                data.progress.total,
                width.saturating_sub(19),
            ),
            width,
        ),
        logo_line(
            logos[2],
            summary_line(
                data.progress.executing,
                queued,
                waiting,
                view.attention.len(),
                data.progress.completed,
            ),
            width,
        ),
        logo_line(logos[3], merge_line(data, width), width),
    ]
}

fn top_line(logo: &str, name: Option<&str>, age: Option<i64>, width: u16) -> Line<'static> {
    let mut spans = logo_spans(logo);
    let left = Span::styled(
        name.unwrap_or("(no plan name)").to_owned(),
        if name.is_some() {
            Theme::header()
        } else {
            Theme::dimmed()
        },
    );
    let right = daemon_spans(age);
    let room = width as usize;
    let left = cut_line(
        Line::from(vec![left]),
        room.saturating_sub(spans_width(&spans) + spans_width(&right)) as u16,
    );
    spans.extend(left.spans);
    let gap = room.saturating_sub(spans_width(&spans) + spans_width(&right));
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(right);
    cut_line(Line::from(spans), width)
}

fn progress_line(spinner: char, completed: usize, total: usize, width: u16) -> Line<'static> {
    let prefix = format!("{spinner} {completed} of {total} stages complete  ");
    let pct = percentage(completed, total);
    let suffix = format!("  {pct}%");
    let available = (width as usize).saturating_sub(text_width(&prefix) + text_width(&suffix));
    let bar_width = available.min(40);
    let filled = completed
        .saturating_mul(bar_width)
        .saturating_add(total / 2)
        .checked_div(total)
        .unwrap_or(0)
        .min(bar_width);
    cut_line(
        Line::from(vec![
            Span::raw(prefix),
            Span::styled(
                "━".repeat(filled),
                Style::default().fg(StatusColors::COMPLETED),
            ),
            Span::styled("╌".repeat(bar_width - filled), Theme::dimmed()),
            Span::raw(suffix),
        ]),
        width,
    )
}

fn summary_line(
    executing: usize,
    queued: usize,
    waiting: usize,
    attention: usize,
    done: usize,
) -> Line<'static> {
    let mut spans = status_count(StageStatus::Executing, executing, "executing");
    spans.push(Span::raw(" · "));
    spans.extend(status_count(StageStatus::Queued, queued, "queued"));
    spans.push(Span::raw(" · "));
    spans.extend(status_count(
        StageStatus::WaitingForDeps,
        waiting,
        "waiting",
    ));
    spans.push(Span::raw(" · "));
    let attention_style = if attention == 0 {
        Theme::dimmed()
    } else {
        Theme::status_blocked()
    };
    spans.push(Span::styled(
        format!("{attention} need attention"),
        attention_style,
    ));
    spans.push(Span::raw(" · "));
    spans.extend(status_count(StageStatus::Completed, done, "done"));
    Line::from(spans)
}

fn status_count(status: StageStatus, count: usize, label: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(status.icon().to_owned(), status.tui_style()),
        Span::raw(format!(" {count} {label}")),
    ]
}

fn merge_line(data: &crate::commands::status::data::StatusData, width: u16) -> Line<'static> {
    cut_line(
        Line::from(Span::styled(
            format!(
                "merged {} · unmerged {} · conflicts {}",
                data.merge.merged.len(),
                data.merge.pending.len(),
                data.merge.conflicts.len()
            ),
            Theme::dimmed(),
        )),
        width.saturating_sub(19),
    )
}

fn logo_line(logo: &str, line: Line<'static>, width: u16) -> Line<'static> {
    let mut spans = logo_spans(logo);
    spans.extend(line.spans);
    cut_line(Line::from(spans), width)
}

fn logo_spans(logo: &str) -> Vec<Span<'static>> {
    let padding = 19usize.saturating_sub(text_width(logo));
    vec![
        Span::styled(logo.to_owned(), Theme::header()),
        Span::raw(" ".repeat(padding)),
    ]
}

fn daemon_spans(age: Option<i64>) -> Vec<Span<'static>> {
    match age {
        Some(age) if age >= 60 => vec![Span::styled(
            format!("● loop stalled {age}s"),
            Theme::status_warning(),
        )],
        Some(age) => vec![
            Span::styled("● daemon running", Theme::status_completed()),
            Span::styled(format!(" · tick {age}s ago"), Theme::dimmed()),
        ],
        None => vec![
            Span::styled("● daemon running", Theme::status_completed()),
            Span::styled(" · tick unknown", Theme::dimmed()),
        ],
    }
}

fn count_status(
    stages: &[crate::commands::status::data::StageSummary],
    status: StageStatus,
) -> usize {
    stages.iter().filter(|stage| stage.status == status).count()
}

fn percentage(completed: usize, total: usize) -> usize {
    completed
        .saturating_mul(100)
        .saturating_add(total / 2)
        .checked_div(total)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::summary_line;

    #[test]
    fn summary_row_reports_stage_counts() {
        let line = summary_line(2, 1, 3, 2, 2);
        assert_eq!(
            line.to_string(),
            "● 2 executing · ▶ 1 queued · ○ 3 waiting · 2 need attention · ✓ 2 done"
        );
    }
}
