//! The document model of a project's `doc/` tree: discovery, plan slugs, heading sections,
//! the shared write-path predicate, runtime paths and a polling watcher.

pub mod discover;
pub mod kinds;
pub mod paths;
pub mod runtime;
pub mod sections;
pub mod slug;
pub mod watch;

#[cfg(test)]
mod tests;
