//! Wire model of the planning server: JSON bodies and WebSocket frames.

pub mod chat;
pub mod checklist;
pub mod concept;
pub mod files;
pub mod message;
pub mod paths;
pub mod presence;
pub mod server;

#[cfg(test)]
mod files_tests;
#[cfg(test)]
mod paths_tests;
#[cfg(test)]
mod tests_chat;
#[cfg(test)]
mod tests_concept;
#[cfg(test)]
mod tests_schema;
