//! Recorded review (DESIGN D12): [`report`] reads a reviewer's `loom-review`
//! block, [`fingerprint`] hashes the changes a round saw, as the loom daemon
//! computes them (`observer`), [`store`] keeps the rounds, rulings and
//! carried findings, [`handoffs`] the base runs a criterion narrowing rests
//! on and the tests it hands off, and [`gate`] refuses completion until the review is
//! current and every finding is closed.

mod base_move;
pub mod fingerprint;
mod frozen_cover;
pub mod gate;
pub mod handoffs;
mod observer;
pub mod prose;
pub mod report;
pub mod store;
pub mod verdict_records;

#[cfg(test)]
mod frozen_cover_contract_tests;
