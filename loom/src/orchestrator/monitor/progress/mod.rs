//! Useful-progress time and heartbeat-update helpers.

use std::time::Duration;

use chrono::{DateTime, Utc};

use super::events::MonitorEvent;
use super::heartbeat::HeartbeatUpdate;

/// Clock used by heartbeat liveness decisions.
#[derive(Debug, Clone, Default)]
pub(crate) struct Clock {
    #[cfg(test)]
    fixed: Option<DateTime<Utc>>,
}

impl Clock {
    pub(crate) fn now(&self) -> DateTime<Utc> {
        #[cfg(test)]
        if let Some(now) = self.fixed {
            return now;
        }
        Utc::now()
    }

    #[cfg(test)]
    pub(crate) fn fixed(now: DateTime<Utc>) -> Self {
        Self { fixed: Some(now) }
    }
}

pub(crate) fn age_secs(now: DateTime<Utc>, then: DateTime<Utc>) -> u64 {
    u64::try_from(now.signed_duration_since(then).num_seconds()).unwrap_or(0)
}

pub(crate) fn is_stale_at(now: DateTime<Utc>, then: DateTime<Utc>, timeout: Duration) -> bool {
    chrono::Duration::from_std(timeout).is_ok_and(|limit| now.signed_duration_since(then) > limit)
}

pub(crate) fn heartbeat_event(update: &HeartbeatUpdate) -> MonitorEvent {
    MonitorEvent::HeartbeatReceived {
        stage_id: update.heartbeat.stage_id.clone(),
        session_id: update.heartbeat.session_id.clone(),
        progress_at: update.heartbeat.effective_progress_at(),
        context_tokens: update.heartbeat.context_tokens,
        transcript_path: update.heartbeat.transcript_path.clone(),
        last_tool: update.heartbeat.last_tool.clone(),
    }
}
