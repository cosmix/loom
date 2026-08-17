//! Unit tests for the context retrieval subsystem.
//!
//! One module per owned area so parallel work never collides in a single file.

mod delivery;
mod fuse;
mod ingest;
mod overlay_key;
mod pack;
mod pack_source;
mod rank;
mod rank_ladder;
mod rank_source;
mod rank_source_matching;
mod retrieve;
mod retrieve_source;
mod schema;
mod source_fixtures;
mod store;
