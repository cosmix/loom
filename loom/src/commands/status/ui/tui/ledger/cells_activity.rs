//! The "Activity" column: what a stage's live session is doing right now,
//! split out of `cells.rs` to keep that file under the line-count limit.

use ratatui::style::Style;

use crate::commands::status::{
    data::{ActivityStatus, CompletionBlockerState, StageSummary},
    render::attention_model::{failure_label, is_contract_phase},
    ui::theme::Theme,
};
use crate::models::stage::StageStatus;
use crate::utils::format_elapsed;

use super::super::text::{text_width, truncate};
use super::Cell;

pub(in super::super) fn activity_cell(stage: &StageSummary, width: u16) -> Cell {
    let mut cell =
        completion_blocker_cell(stage).unwrap_or_else(|| standard_activity_cell(stage, width));
    if stage.completion_blocker.is_none() && !matches!(&stage.status, StageStatus::Executing) {
        if let Some(reason) = stage.outgoing_session_exit_reason {
            let separator = if cell.text.is_empty() { "" } else { " · " };
            cell.text = format!("{}{separator}last: {}", cell.text, reason.status_label());
        }
    }
    let mut cell = held_cell(stage, cell);
    cell.text = truncate(&cell.text, usize::from(width));
    cell
}

fn completion_blocker_cell(stage: &StageSummary) -> Option<Cell> {
    if !matches!(
        &stage.status,
        StageStatus::Executing | StageStatus::NeedsHumanReview
    ) {
        return None;
    }
    let blocker = stage.completion_blocker.as_ref()?;
    let style = match blocker.state {
        CompletionBlockerState::Pending => Theme::status_warning(),
        CompletionBlockerState::Blocked | CompletionBlockerState::OwnershipUnknown => {
            Theme::status_blocked()
        }
    };
    Some(Cell {
        text: blocker.activity_text(),
        style,
    })
}

fn standard_activity_cell(stage: &StageSummary, width: u16) -> Cell {
    if matches!(&stage.status, StageStatus::Executing) && stage.incoherence.is_some() {
        Cell {
            text: "incoherent".to_owned(),
            style: Theme::status_blocked(),
        }
    } else {
        match &stage.status {
            StageStatus::Queued => Cell {
                text: "ready".to_owned(),
                style: Theme::status_queued(),
            },
            StageStatus::Executing => executing_activity(stage, width),
            StageStatus::WaitingForInput => Cell {
                text: "awaiting input".to_owned(),
                style: stage.status.tui_style(),
            },
            StageStatus::NeedsHandoff => Cell {
                text: "handing off".to_owned(),
                style: Theme::status_warning(),
            },
            StageStatus::Blocked => retry_activity(stage, blocked_label(stage)),
            StageStatus::CompletedWithFailures => retry_activity(stage, "failed"),
            StageStatus::MergeConflict => Cell {
                text: "conflict".to_owned(),
                style: Theme::status_warning(),
            },
            StageStatus::MergeBlocked => Cell {
                text: "merge error".to_owned(),
                style: Theme::status_blocked(),
            },
            StageStatus::NeedsHumanReview => Cell {
                text: "awaiting you".to_owned(),
                style: stage.status.tui_style(),
            },
            StageStatus::NeedsAdjudication => adjudication_activity(stage),
            _ => Cell {
                text: String::new(),
                style: Style::default(),
            },
        }
    }
}

fn executing_activity(stage: &StageSummary, width: u16) -> Cell {
    if is_contract_phase(stage) {
        if let Some(cell) = contract_writer_activity(stage) {
            return cell;
        }
    }
    match stage.activity_status {
        ActivityStatus::Working => {
            let text = match &stage.last_tool {
                Some(tool) => {
                    let prefix = "working · ";
                    format!(
                        "{prefix}{}",
                        truncate(tool, usize::from(width).saturating_sub(text_width(prefix)))
                    )
                }
                None => "working".to_owned(),
            };
            Cell {
                text,
                style: Theme::status_completed(),
            }
        }
        ActivityStatus::Idle => staleness_activity("idle", stage.staleness_secs, Theme::dimmed()),
        ActivityStatus::Stale => {
            staleness_activity("stale", stage.staleness_secs, Theme::status_warning())
        }
        ActivityStatus::Orphaned => Cell {
            text: "orphaned".to_owned(),
            style: Theme::status_blocked(),
        },
        ActivityStatus::Error => Cell {
            text: "crashed".to_owned(),
            style: Theme::status_blocked(),
        },
    }
}

/// Activity text for the contract-writer phase: styled distinctly (magenta,
/// never yellow/red) so it does not read as a warning or an error. A stale
/// contract writer keeps the same "no heartbeat" duration suffix as any
/// other stale session, just under different wording. `None` for an
/// orphaned or crashed writer, so the caller falls back to the ordinary
/// orphaned/crashed cell instead of reading as healthy.
fn contract_writer_activity(stage: &StageSummary) -> Option<Cell> {
    match stage.activity_status {
        ActivityStatus::Working | ActivityStatus::Idle => Some(Cell {
            text: "writing contract tests".to_owned(),
            style: Theme::status_contract(),
        }),
        ActivityStatus::Stale => Some(staleness_activity(
            "contract writer idle",
            stage.staleness_secs,
            Theme::status_contract(),
        )),
        ActivityStatus::Orphaned | ActivityStatus::Error => None,
    }
}

fn staleness_activity(prefix: &str, seconds: Option<u64>, style: Style) -> Cell {
    let text = seconds.map_or_else(
        || prefix.to_owned(),
        |seconds| {
            format!(
                "{prefix} {}",
                format_elapsed(seconds.try_into().unwrap_or(i64::MAX))
            )
        },
    );
    Cell { text, style }
}

fn retry_activity(stage: &StageSummary, label: &str) -> Cell {
    let maximum = stage.max_retries.unwrap_or(3);
    Cell {
        text: format!("{label} {}/{maximum}", stage.retry_count),
        style: Theme::status_blocked(),
    }
}

fn blocked_label(stage: &StageSummary) -> &'static str {
    stage
        .failure_info
        .as_ref()
        .map(|failure| failure_label(&failure.failure_type))
        .unwrap_or("error")
}

fn adjudication_activity(stage: &StageSummary) -> Cell {
    let state = match stage.judge_heartbeat_secs {
        None => "none",
        Some(seconds) if seconds <= 300 => "working",
        Some(_) => "stale",
    };
    Cell {
        text: format!("dispute {} · judge {state}", stage.dispute_count),
        style: Theme::status_warning(),
    }
}

fn held_cell(stage: &StageSummary, cell: Cell) -> Cell {
    if stage.held && !cell.text.is_empty() {
        Cell {
            text: format!("held · {}", cell.text),
            style: Theme::status_warning(),
        }
    } else {
        cell
    }
}
