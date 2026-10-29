//! Review loop: restores, suggestions, compare, built notes and open-question counts.

pub mod built;
pub mod commands;
pub mod compare;
pub mod questions;
pub mod restore;
pub mod routes;
pub mod sections;
pub mod suggest;

mod setup;
mod state;
mod turn_routes;

pub use routes::{install, routes};

#[cfg(test)]
mod at_commit_tests;
#[cfg(test)]
mod command_tests;
#[cfg(test)]
mod git_tests;
#[cfg(test)]
mod questions_tests;
#[cfg(test)]
mod restore_tests;
#[cfg(test)]
mod route_tests;
#[cfg(test)]
mod sections_tests;
#[cfg(test)]
mod setup_tests;
#[cfg(test)]
mod suggest_tests;
