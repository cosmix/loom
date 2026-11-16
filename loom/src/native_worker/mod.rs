//! Native Codex workers: records, protocol and the pure state reducer.

pub mod boundary;
pub mod bridge;
pub mod ids;
pub mod index;
pub mod machine;
pub mod model;
pub mod prompt;
pub mod protocol;
pub mod proxy;
pub mod store;
mod store_io;
pub mod supervisor;

#[cfg(test)]
mod core_contract_tests;
#[cfg(test)]
mod runtime_contract_tests;
