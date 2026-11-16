//! Native Codex workers: records, protocol and the pure state reducer.

pub mod ids;
pub mod index;
pub mod machine;
pub mod model;
pub mod prompt;
pub mod protocol;
pub mod store;
mod store_io;

#[cfg(test)]
mod core_contract_tests;
