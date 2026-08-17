//! File-level fallback for anything no grammar claims.
//!
//! A file loom cannot parse must still be *findable*: retrieval ranks over
//! paths and lexical overlap as well as symbols, so dropping unparseable files
//! would make whole directories invisible. Every such file gets exactly one
//! node, tagged [`FileCoverage::LexicalOnly`] so a consumer can tell "no
//! symbols here" from "no symbols found here".

use std::path::Path;

use super::FileExtraction;
use crate::context::source_graph::{FileCoverage, NodeLanguage};

/// `parser_version` stamped on a node no grammar produced.
pub const LEXICAL_PARSER_VERSION: &str = "lexical+v1";

/// Language tag for a path, by extension.
///
/// Returns the [`NodeLanguage`] arm matching one of loom's supported languages
/// when the extension is recognized, and `Other(<extension>)` otherwise so
/// output can still name what the file was.
pub fn language_for_path(path: &Path) -> NodeLanguage {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "rs" => NodeLanguage::Rust,
        "ts" | "tsx" | "mts" | "cts" => NodeLanguage::TypeScript,
        "py" | "pyi" => NodeLanguage::Python,
        "go" => NodeLanguage::Go,
        "" => NodeLanguage::Other("unknown".to_string()),
        other => NodeLanguage::Other(other.to_string()),
    }
}

/// Build the file-level-only extraction for `path`.
pub fn extract(path: &Path, bytes: &[u8]) -> FileExtraction {
    let language = language_for_path(path);
    let detail = format!("no source-graph extractor for {language}");
    FileExtraction::file_level(
        path,
        bytes,
        language,
        LEXICAL_PARSER_VERSION.to_string(),
        FileCoverage::LexicalOnly { detail },
    )
}
