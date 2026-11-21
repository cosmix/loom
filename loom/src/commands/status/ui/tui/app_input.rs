//! Keys, mouse and daemon responses: how the app's state changes.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::time::Instant;

use super::{
    handle_key_event, handle_mouse_event, is_scroll_key, scroll_alias, KeyEventResult, LiveStatus,
    Response, TuiApp, POLL_TIMEOUT,
};

impl TuiApp {
    pub(super) fn handle_input(&mut self) -> Result<()> {
        if !event::poll(POLL_TIMEOUT)? {
            return Ok(());
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                self.handle_key(key.code, key.modifiers);
            }
            Event::Mouse(mouse) if !self.legend_open || self.plan_report.is_some() => {
                handle_mouse_event(mouse.kind, &mut self.graph_state)
            }
            _ => {}
        }
        Ok(())
    }

    pub(in super::super) fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let result = if self.plan_report.is_some() {
            // The report has no legend or token pane: the keys that scroll it
            // and quit are the only ones that act.
            match handle_key_event(scroll_alias(code), modifiers, &mut self.graph_state) {
                KeyEventResult::ToggleLegend | KeyEventResult::ToggleTokens => {
                    KeyEventResult::Continue
                }
                KeyEventResult::CloseLegend => KeyEventResult::Exit,
                other => other,
            }
        } else if self.legend_open && is_scroll_key(code) {
            KeyEventResult::Continue
        } else {
            handle_key_event(code, modifiers, &mut self.graph_state)
        };
        match result {
            KeyEventResult::Exit => self.exiting = true,
            KeyEventResult::ToggleLegend => self.legend_open = !self.legend_open,
            KeyEventResult::ToggleTokens => self.show_tokens = !self.show_tokens,
            KeyEventResult::CloseLegend if self.legend_open => self.legend_open = false,
            KeyEventResult::CloseLegend => self.exiting = true,
            KeyEventResult::Continue => {}
        }
    }

    /// Handle a response from the daemon.
    pub(super) fn handle_response(&mut self, response: Response) {
        match response {
            Response::StatusUpdate { data } => {
                let mut data = *data;
                if let Some(record) = data.plan_stats.take() {
                    self.plan_report = Some(record);
                }
                self.status = LiveStatus { data };
                let all_stages = self.status.all_stages();
                self.activity_log.update(&all_stages);
                if self.completion_summary.is_none()
                    && !self
                        .last_error
                        .as_deref()
                        .is_some_and(|message| message.starts_with("Orchestration: "))
                {
                    self.last_error = None;
                }
            }
            Response::OrchestrationComplete { summary } => {
                if summary.failure_count == 0 {
                    self.last_error = None;
                }
                self.completion_summary = Some(summary);
                self.completion_received_at = Some(Instant::now());
            }
            Response::Error { message } => {
                self.last_error = Some(message);
            }
            _ => {}
        }
    }
}
