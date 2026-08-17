//! `loom context` — record and inspect the retrieval context of a stage.
//!
//! These entry points are called by loom's shell hooks rather than by a human,
//! so they share one discipline: no model call, no network call, quiet on
//! success, and never fatal to the tool call that invoked them.

pub mod record_edit;
