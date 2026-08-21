//! Unit tests for the context retrieval subsystem.
//!
//! One module per owned area so parallel work never collides in a single file.

mod delivery;
mod fuse;
mod ingest;
mod lexical_index;
mod lexical_index_cache;
mod overlay_key;
mod pack;
mod pack_source;
mod rank;
mod rank_evidence;
mod rank_fixtures;
mod rank_ladder;
mod rank_source;
mod rank_source_matching;
mod rank_source_scoring;
mod rank_stopwords;
mod retrieve;
mod retrieve_source;
mod schema;
mod source_fixtures;
mod store;
