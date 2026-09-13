use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct StreamTokenTotals {
    pub(crate) observations: usize,
    pub(crate) input: u64,
    pub(crate) cache_creation: u64,
    pub(crate) cache_read: u64,
    pub(crate) output: u64,
    pub(crate) cache_write_5m: u64,
    pub(crate) cache_write_1h: u64,
}

impl StreamTokenTotals {
    pub(crate) fn add(&mut self, values: [u64; 6]) {
        self.observations += 1;
        self.input = self.input.saturating_add(values[0]);
        self.cache_creation = self.cache_creation.saturating_add(values[1]);
        self.cache_read = self.cache_read.saturating_add(values[2]);
        self.output = self.output.saturating_add(values[3]);
        self.cache_write_5m = self.cache_write_5m.saturating_add(values[4]);
        self.cache_write_1h = self.cache_write_1h.saturating_add(values[5]);
    }

    pub(super) fn merge(&mut self, other: Self) {
        self.observations += other.observations;
        self.input = self.input.saturating_add(other.input);
        self.cache_creation = self.cache_creation.saturating_add(other.cache_creation);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.output = self.output.saturating_add(other.output);
        self.cache_write_5m = self.cache_write_5m.saturating_add(other.cache_write_5m);
        self.cache_write_1h = self.cache_write_1h.saturating_add(other.cache_write_1h);
    }
}
