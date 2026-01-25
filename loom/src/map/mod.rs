//! Codebase mapping and analysis module.
//!
//! Provides automated codebase analysis including:
//! - Project type detection (Rust, Node, Go, Python, etc.)
//! - Dependency analysis from manifest files
//! - Entry point discovery
//! - Directory structure mapping
//! - Convention detection
//! - Concern identification (TODOs, FIXMEs, security issues)

pub mod analyzer;
pub mod detectors;

pub use analyzer::{analyze_codebase, AnalysisResult};
