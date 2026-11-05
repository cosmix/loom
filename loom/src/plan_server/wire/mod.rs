//! Wire model of the planning server: JSON bodies and WebSocket frames.

pub mod chat;
pub mod checklist;
pub mod concept;
pub mod files;
pub mod kickoff;
pub mod message;
pub mod paths;
pub mod presence;
pub mod pressure;
pub mod review;
pub mod run;
pub mod server;
pub mod summaries;
pub mod team;

#[cfg(test)]
mod built_contract_tests;
#[cfg(test)]
mod files_tests;
#[cfg(test)]
mod paths_tests;
#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod tests_chat;
#[cfg(test)]
mod tests_concept;
#[cfg(test)]
mod tests_pressure;
#[cfg(test)]
mod tests_review;
#[cfg(test)]
mod tests_run;
#[cfg(test)]
mod tests_schema;
#[cfg(test)]
mod tests_summaries;
