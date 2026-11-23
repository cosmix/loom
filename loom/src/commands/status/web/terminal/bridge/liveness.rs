//! The clocks that decide a connected peer is no longer taking part: an input
//! gate that never reopens, an output flush that makes no progress, a peer that
//! answers nothing. Every method takes `now` so tests need no sleeping.

use std::time::{Duration, Instant};

/// The bridge's give-up deadlines.
#[derive(Clone, Copy, Debug)]
pub(in crate::commands::status::web::terminal) struct Timing {
    /// How long the input gate may stay shut, or an output flush may make no
    /// progress, before the bridge gives up on the peer.
    pub(in crate::commands::status::web::terminal) stall: Duration,
    /// Interval between WebSocket pings.
    pub(in crate::commands::status::web::terminal) keepalive: Duration,
    /// How long a peer may stay silent (no frame of any kind, a pong
    /// included) before it is dropped.
    pub(in crate::commands::status::web::terminal) silence: Duration,
}

impl Timing {
    pub(in crate::commands::status::web::terminal) const DEFAULT: Timing = Timing {
        stall: super::GATE_STALL_TIMEOUT,
        keepalive: Duration::from_secs(15),
        silence: Duration::from_secs(60),
    };
}

pub(super) struct Liveness {
    timing: Timing,
    gated_since: Option<Instant>,
    output_since: Option<Instant>,
    last_heard: Instant,
    last_ping: Instant,
}

impl Liveness {
    pub(super) fn new(timing: Timing, now: Instant) -> Self {
        Self {
            timing,
            gated_since: None,
            output_since: None,
            last_heard: now,
            last_ping: now,
        }
    }

    /// Track how long the input queue has sat at its cap, and say when that has
    /// gone on too long.
    ///
    /// Gating is normal and self-clearing while the child consumes, so the clock
    /// starts on the first pass that finds the queue full and is thrown away the
    /// moment it drains; only a gate that never reopens means nobody is draining
    /// this terminal.
    pub(super) fn gate_stalled(&mut self, now: Instant, gate_shut: bool) -> bool {
        elapsed(&mut self.gated_since, now, gate_shut, self.timing.stall)
    }

    /// The same clock for the output direction: `stalled` is whether this
    /// pass's flush or send could not finish. Any pass that completes clears
    /// it, so a peer draining a little each stall period is kept.
    pub(super) fn output_stalled(&mut self, now: Instant, stalled: bool) -> bool {
        elapsed(&mut self.output_since, now, stalled, self.timing.stall)
    }

    /// Something arrived from the peer.
    pub(super) fn heard(&mut self, now: Instant) {
        self.last_heard = now;
    }

    /// True at most once per keepalive interval.
    pub(super) fn ping_due(&mut self, now: Instant) -> bool {
        let due = now.saturating_duration_since(self.last_ping) >= self.timing.keepalive;
        if due {
            self.last_ping = now;
        }
        due
    }

    /// Whether the peer has said nothing for too long. While the input gate is
    /// shut the socket is not read, so a live peer's pongs sit unread: the
    /// deadline is paused, and counts again from the pass the gate reopens.
    pub(super) fn silent(&mut self, now: Instant, gate_shut: bool) -> bool {
        if gate_shut {
            self.last_heard = now;
            return false;
        }
        now.saturating_duration_since(self.last_heard) >= self.timing.silence
    }
}

/// A clock that starts when `active` is first true, is cleared when it turns
/// false, and reports whether `limit` has run out.
fn elapsed(since: &mut Option<Instant>, now: Instant, active: bool, limit: Duration) -> bool {
    if !active {
        *since = None;
        return false;
    }
    now.saturating_duration_since(*since.get_or_insert(now)) >= limit
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    fn timing() -> Timing {
        Timing {
            stall: 10 * SECOND,
            keepalive: 5 * SECOND,
            silence: 20 * SECOND,
        }
    }

    #[test]
    fn the_gate_clock_starts_clears_and_fires() {
        let start = Instant::now();
        let mut live = Liveness::new(timing(), start);
        assert!(!live.gate_stalled(start, false));
        assert!(
            !live.gate_stalled(start + SECOND, true),
            "clock starts here"
        );
        assert!(!live.gate_stalled(start + 10 * SECOND, true), "9 s in");
        assert!(live.gate_stalled(start + 11 * SECOND, true), "10 s in");
        assert!(!live.gate_stalled(start + 12 * SECOND, false), "reopened");
        assert!(!live.gate_stalled(start + 13 * SECOND, true), "restarted");
    }

    #[test]
    fn an_output_flush_that_makes_progress_resets_the_clock() {
        let start = Instant::now();
        let mut live = Liveness::new(timing(), start);
        assert!(!live.output_stalled(start, true));
        assert!(!live.output_stalled(start + 9 * SECOND, true));
        assert!(!live.output_stalled(start + 9 * SECOND, false), "progress");
        assert!(!live.output_stalled(start + 12 * SECOND, true), "restarted");
        assert!(!live.output_stalled(start + 21 * SECOND, true), "9 s in");
        assert!(live.output_stalled(start + 22 * SECOND, true), "10 s in");
    }

    #[test]
    fn a_ping_is_due_once_per_interval() {
        let start = Instant::now();
        let mut live = Liveness::new(timing(), start);
        assert!(!live.ping_due(start + 4 * SECOND));
        assert!(live.ping_due(start + 5 * SECOND));
        assert!(!live.ping_due(start + 6 * SECOND));
        assert!(!live.ping_due(start + 9 * SECOND));
        assert!(live.ping_due(start + 10 * SECOND));
    }

    #[test]
    fn hearing_the_peer_defers_the_silence_deadline() {
        let start = Instant::now();
        let mut live = Liveness::new(timing(), start);
        assert!(!live.silent(start + 19 * SECOND, false));
        live.heard(start + 19 * SECOND);
        assert!(!live.silent(start + 38 * SECOND, false));
        assert!(live.silent(start + 39 * SECOND, false));
    }

    #[test]
    fn the_silence_deadline_is_paused_while_the_gate_is_shut() {
        let start = Instant::now();
        let mut live = Liveness::new(timing(), start);
        assert!(!live.silent(start + 100 * SECOND, true), "gate shut");
        assert!(!live.silent(start + 119 * SECOND, false), "reopened, 19 s");
        assert!(live.silent(start + 120 * SECOND, false), "reopened, 20 s");
    }
}
