//! Concept mode: the product and engineering overviews and per-document summary pages, resolved
//! server-side from plan 4's summary store.

pub mod body;
mod delta;
mod empty;
pub mod engine;
pub mod key;
pub mod links;
pub mod page;
mod page_key;
pub mod routes;
pub mod state;

pub(crate) use delta::page_keys;
pub use engine::{install, ConceptState};
pub use page::{load_page, PageContext, PageKey, Scope};
pub use routes::routes;

#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod key_tests;
#[cfg(test)]
mod links_tests;
#[cfg(test)]
mod route_tests;
#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_rebuilds;

#[cfg(test)]
mod feature_tests;
#[cfg(test)]
mod fixtures;
