//! Outbound request-id allocation and response matching.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    Initialize,
    ThreadStart,
    TurnStart,
    TurnSteer { message_id: String },
    TurnInterrupt,
}

#[derive(Debug, Default)]
pub struct Correlator {
    issued: u64,
    pending: HashMap<u64, Pending>,
}

impl Correlator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, pending: Pending) -> u64 {
        self.issued += 1;
        self.pending.insert(self.issued, pending);
        self.issued
    }

    pub fn resolve(&mut self, id: u64) -> Option<Pending> {
        self.pending.remove(&id)
    }

    pub fn outstanding(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
#[path = "correlate_tests.rs"]
mod tests;
