//! Recorded review (DESIGN D12): [`report`] reads a reviewer's `loom-review`
//! block, [`fingerprint`] hashes the changes a round saw, [`store`] keeps the
//! rounds, rulings and carried findings, and [`gate`] refuses completion until
//! the review is current and every finding is closed.

pub mod fingerprint;
pub mod gate;
pub mod report;
pub mod store;
pub mod verdict_records;
