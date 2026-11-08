//! The daemon span at the right of the header's top line: what the orchestrator
//! loop is doing, from `busy::loop_state`.

use ratatui::text::Span;

use super::text::spans_width;
use crate::commands::status::ui::theme::Theme;
use crate::orchestrator::busy::{Busy, LoopState};
use crate::orchestrator::tick::STALL_THRESHOLD_SECS;
use crate::utils::format_elapsed;

/// The span for `state`, at most `max_width` cells when a shorter form exists.
///
/// A busy operation falls back to `● busy <op> <elapsed>` when the full text does
/// not fit: the elapsed time is never the part that gets cut.
pub(super) fn daemon_spans(state: &LoopState, max_width: usize) -> Vec<Span<'static>> {
    match state {
        LoopState::Unknown => running(" · tick unknown".to_owned()),
        LoopState::Ticking { age_secs } => running(format!(" · tick {age_secs}s ago")),
        LoopState::Busy {
            busy,
            elapsed_secs,
            over_ceiling,
        } => busy_spans(busy, *elapsed_secs, *over_ceiling, max_width),
        LoopState::Stalled {
            age_secs,
            elapsed_secs,
            ..
        } => stalled(*age_secs, *elapsed_secs),
    }
}

fn running(detail: String) -> Vec<Span<'static>> {
    vec![
        Span::styled("● daemon running", Theme::status_completed()),
        Span::styled(detail, Theme::dimmed()),
    ]
}

fn busy_spans(
    busy: &Busy,
    elapsed_secs: i64,
    over_ceiling: bool,
    max_width: usize,
) -> Vec<Span<'static>> {
    let elapsed = format_elapsed(elapsed_secs);
    let what = busy.op.describe(&busy.stage_id);
    let mut spans = vec![
        Span::styled("● daemon running", Theme::status_completed()),
        Span::styled(format!(" · busy {what} {elapsed}"), Theme::dimmed()),
    ];
    if spans_width(&spans) > max_width {
        spans = vec![
            Span::styled("●", Theme::status_completed()),
            Span::styled(
                format!(" busy {} {elapsed}", busy.op.as_str()),
                Theme::dimmed(),
            ),
        ];
    }
    if over_ceiling {
        warn_whole(spans)
    } else {
        spans
    }
}

/// Over the ceiling the whole span takes the warning style, marker included.
fn warn_whole(spans: Vec<Span<'static>>) -> Vec<Span<'static>> {
    let text: String = spans.iter().map(|span| span.content.as_ref()).collect();
    vec![Span::styled(text, Theme::status_warning())]
}

/// A stale tick shows the tick's age; a loop stalled only by the global ceiling
/// shows how long the operation has run.
fn stalled(age_secs: i64, elapsed_secs: Option<i64>) -> Vec<Span<'static>> {
    let text = match elapsed_secs {
        Some(elapsed) if age_secs < STALL_THRESHOLD_SECS => {
            format!("● loop stalled {}", format_elapsed(elapsed))
        }
        _ => format!("● loop stalled {age_secs}s"),
    };
    vec![Span::styled(text, Theme::status_warning())]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::busy::BusyOp;
    use chrono::Utc;
    use ratatui::style::Style;

    fn busy(op: BusyOp) -> Busy {
        Busy {
            op,
            stage_id: "criteria-authoring".to_owned(),
            started_at: Utc::now(),
        }
    }

    fn busy_state(op: BusyOp, elapsed_secs: i64, over_ceiling: bool) -> LoopState {
        LoopState::Busy {
            busy: busy(op),
            elapsed_secs,
            over_ceiling,
        }
    }

    fn text(spans: &[Span<'_>]) -> String {
        spans.iter().map(|span| span.content.as_ref()).collect()
    }

    fn styles(spans: &[Span<'_>]) -> Vec<Style> {
        spans.iter().map(|span| span.style).collect()
    }

    #[test]
    fn ticking_and_unknown_keep_the_tick_wording() {
        let ticking = daemon_spans(&LoopState::Ticking { age_secs: 4 }, 100);
        assert_eq!(text(&ticking), "● daemon running · tick 4s ago");
        let unknown = daemon_spans(&LoopState::Unknown, 100);
        assert_eq!(text(&unknown), "● daemon running · tick unknown");
    }

    #[test]
    fn busy_under_the_ceiling_is_dimmed_after_a_green_marker() {
        let spans = daemon_spans(&busy_state(BusyOp::BriefGeneration, 90, false), 100);
        assert_eq!(
            text(&spans),
            "● daemon running · busy generating the brief for criteria-authoring 1m30s"
        );
        assert_eq!(
            styles(&spans),
            vec![Theme::status_completed(), Theme::dimmed()]
        );
    }

    #[test]
    fn busy_over_the_ceiling_is_one_warning_span() {
        let spans = daemon_spans(&busy_state(BusyOp::BriefGeneration, 400, true), 100);
        assert_eq!(
            text(&spans),
            "● daemon running · busy generating the brief for criteria-authoring 6m40s"
        );
        assert_eq!(styles(&spans), vec![Theme::status_warning()]);
    }

    #[test]
    fn global_ceiling_stall_prints_the_operations_elapsed_time() {
        let state = LoopState::Stalled {
            age_secs: 2,
            busy: Some(busy(BusyOp::Provisioning)),
            elapsed_secs: Some(1900),
        };
        let spans = daemon_spans(&state, 100);
        assert_eq!(text(&spans), "● loop stalled 31m40s");
        assert_eq!(styles(&spans), vec![Theme::status_warning()]);
    }

    #[test]
    fn stale_tick_while_busy_prints_the_tick_age() {
        let state = LoopState::Stalled {
            age_secs: 90,
            busy: Some(busy(BusyOp::Provisioning)),
            elapsed_secs: Some(100),
        };
        assert_eq!(text(&daemon_spans(&state, 100)), "● loop stalled 90s");
        let idle = LoopState::Stalled {
            age_secs: 75,
            busy: None,
            elapsed_secs: None,
        };
        assert_eq!(text(&daemon_spans(&idle, 100)), "● loop stalled 75s");
    }

    #[test]
    fn a_narrow_header_falls_back_but_keeps_the_elapsed_time() {
        let narrow = 64 - 19 - 12;
        let state = busy_state(BusyOp::MergeVerification, 187, false);
        assert_eq!(
            text(&daemon_spans(&state, narrow)),
            "● busy merge-verification 3m7s"
        );
        let over = daemon_spans(&busy_state(BusyOp::MergeVerification, 1300, true), narrow);
        assert_eq!(text(&over), "● busy merge-verification 21m40s");
        assert_eq!(styles(&over), vec![Theme::status_warning()]);
    }
}
