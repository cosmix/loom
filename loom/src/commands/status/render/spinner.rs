//! Braille dot spinner animation

use std::time::{Duration, Instant};

/// Braille spinner frames (~100ms per frame)
const SPINNER_FRAMES: [char; 8] = ['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];

/// Spinner state for animation
pub struct Spinner {
    frame: usize,
    last_update: Instant,
    frame_duration: Duration,
}

impl Spinner {
    pub fn new() -> Self {
        Self {
            frame: 0,
            last_update: Instant::now(),
            frame_duration: Duration::from_millis(100),
        }
    }

    /// Advance spinner if enough time has passed
    pub fn tick(&mut self) {
        if self.last_update.elapsed() >= self.frame_duration {
            self.frame = (self.frame + 1) % SPINNER_FRAMES.len();
            self.last_update = Instant::now();
        }
    }

    /// Get current spinner character
    pub fn current(&self) -> char {
        SPINNER_FRAMES[self.frame]
    }

    /// Get spinner with label
    pub fn with_label(&self, label: &str) -> String {
        format!("{} {}", self.current(), label)
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}
