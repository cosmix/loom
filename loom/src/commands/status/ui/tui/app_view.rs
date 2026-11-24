//! What the app draws, and how it runs against a finished report.

use std::sync::atomic::Ordering;

use anyhow::Result;
use chrono::Utc;
use ratatui::{layout::Rect, widgets::Paragraph, Frame};

use super::{
    attention_entries, busy, ledger, render_completion, render_plan_overview, LedgerView,
    PlanStatsRecord, TuiApp,
};

impl TuiApp {
    /// Draw the current view on the app's own terminal.
    pub(super) fn render(&mut self) -> Result<()> {
        let Some(mut terminal) = self.terminal.take() else {
            return Ok(());
        };
        let drawn = terminal.draw(|frame| self.draw_view(frame)).map(|_| ());
        self.terminal = Some(terminal);
        Ok(drawn?)
    }

    /// Draw the current view into any backend: the report once the plan has
    /// one, else the legacy completion summary, else the ledger. Keeps the
    /// scroll viewport in step with what was drawn.
    pub(in super::super) fn draw_view(&mut self, frame: &mut Frame<'_>) {
        if let Some(record) = &self.plan_report {
            let area = frame.area();
            let metrics = render_plan_overview(frame, area, record, self.graph_state.scroll_y);
            self.graph_state.total_lines = metrics.total_lines;
            self.graph_state.viewport_height = metrics.viewport;
            self.graph_state.scroll_y = metrics.scroll_y;
        } else if let Some(summary) = &self.completion_summary {
            render_completion(frame, frame.area(), summary);
            if let Some(error) = &self.last_error {
                let area = frame.area();
                let line = Rect::new(area.x, area.y.saturating_add(2), area.width, 1);
                frame.render_widget(Paragraph::new(error.as_str()), line);
            }
        } else {
            self.draw_ledger(frame);
        }
    }

    fn draw_ledger(&mut self, frame: &mut Frame<'_>) {
        let levels = self.status.compute_levels();
        let ordered = self.status.all_stages_with_levels(&levels);
        let attention = attention_entries(&self.status.data.stages);
        let view = LedgerView {
            data: &self.status.data,
            levels: &levels,
            ordered: &ordered,
            attention: &attention,
            activity: &self.activity_log,
            alerts: &self.alerts,
            spinner: self.spinner_char(),
            scroll_y: self.graph_state.scroll_y,
            legend_open: self.legend_open,
            show_tokens: self.show_tokens,
            // Per frame, so the elapsed time advances between alert refreshes.
            loop_state: busy::loop_state(self.tick.as_ref(), Utc::now()),
            last_error: self.last_error.as_deref(),
            now_epoch: Utc::now().timestamp(),
            // The previous frame's viewport; one frame of lag is fine.
            scrollable: ordered.len() > usize::from(self.graph_state.viewport_height),
        };
        let outcome = ledger::render(frame, &view);
        self.graph_state.total_lines = ordered.len().min(u16::MAX as usize) as u16;
        self.graph_state.viewport_height = outcome.table_viewport_rows;
    }

    /// Get spinner character for current frame.
    fn spinner_char(&self) -> char {
        const SPINNER: [char; 10] = [
            '\u{280B}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283C}', '\u{2834}', '\u{2826}',
            '\u{2827}', '\u{2807}', '\u{280F}',
        ];
        SPINNER[self.spinner_frame % SPINNER.len()]
    }

    /// Show a finished plan's report until the user quits, without a daemon.
    pub(in super::super) fn run_report(
        &mut self,
        work_path: &std::path::Path,
        record: PlanStatsRecord,
    ) -> Result<()> {
        crate::utils::write_tui_marker(work_path);
        self.install_exit_handler()?;
        self.plan_report = Some(record);
        self.render()?;
        while self.running.load(Ordering::SeqCst) && !self.exiting {
            self.handle_input()?;
            self.render()?;
        }
        self.cleanup_terminal();
        if self.interrupted() {
            std::process::exit(130);
        }
        Ok(())
    }

    /// An app that is never drawn to a terminal, for tests.
    #[cfg(test)]
    pub(in super::super) fn headless() -> Self {
        use std::sync::{atomic::AtomicBool, Arc};

        use super::{GraphState, LiveStatus, TerminalGuard, TuiActivityLog};
        Self {
            terminal: None,
            running: Arc::new(AtomicBool::new(true)),
            status: LiveStatus::default(),
            spinner_frame: 0,
            last_error: None,
            graph_state: GraphState::default(),
            activity_log: TuiActivityLog::new(),
            legend_open: false,
            show_tokens: false,
            tick: None,
            terminal_guard: TerminalGuard::inactive_for_test(),
            exiting: false,
            completion_summary: None,
            completion_received_at: None,
            cleaned_up: true,
            alerts: Vec::new(),
            alerts_refreshed_at: None,
            plan_report: None,
            socket_closed: false,
        }
    }
}
