//! A clock that stands still until armed, then jumps forward once per reading.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::native_worker::supervisor::Clock;

struct State {
    now: Instant,
    armed: bool,
    jumps: VecDeque<Duration>,
    last: Duration,
}

/// Every reading before `arm` returns the same instant. After `arm`, each reading
/// first advances by the next queued jump; once the queue is empty it keeps
/// advancing by the last jump, so a loop that waits on time always gets there.
/// Clones share one state, so the peer can arm the clock the loop reads.
#[derive(Clone)]
pub(crate) struct SteppingClock {
    state: Arc<Mutex<State>>,
}

impl SteppingClock {
    pub(crate) fn new(jumps: &[Duration]) -> Self {
        let state = State {
            now: Instant::now(),
            armed: false,
            jumps: jumps.iter().copied().collect(),
            last: Duration::ZERO,
        };
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub(crate) fn arm(&self) {
        self.state.lock().expect("clock state").armed = true;
    }
}

impl Clock for SteppingClock {
    fn now(&self) -> Instant {
        let mut state = self.state.lock().expect("clock state");
        if state.armed {
            if let Some(jump) = state.jumps.pop_front() {
                state.last = jump;
            }
            let step = state.last;
            state.now += step;
        }
        state.now
    }
}
