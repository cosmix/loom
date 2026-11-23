//! Native Codex workers: records, protocol and the pure state reducer.

pub mod admission;
pub mod boundary;
pub mod bridge;
pub mod ids;
pub mod index;
pub mod machine;
pub mod model;
pub mod prompt;
pub mod protocol;
pub mod proxy;
pub mod runner;
pub mod store;
mod store_io;
pub mod supervisor;

#[cfg(test)]
mod core_contract_tests;
#[cfg(test)]
mod ingress_contract_tests;
#[cfg(test)]
mod runner_recover_contract_tests;
#[cfg(test)]
mod runtime_contract_tests;
#[cfg(test)]
mod store_contract_tests;
