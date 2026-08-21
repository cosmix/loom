//! How a query names a file: exact paths, bare stems, stage-dependency paths,
//! and the test-file convention that damps them.
//!
//! Split out of [`super`] so the scoring pass and the path vocabulary it
//! consults stay under the file line limit independently. Everything here is
//! purely lexical — no filesystem access — because it runs against candidate
//! paths that may not exist in this checkout at all.

use crate::context::config::RetrievalConfig;
use crate::context::lexical::{contains_whole_term, ExactGate, TermEvidence};
use crate::context::rank::RankQuery;
use crate::context::schema::SourceNode;

/// How the query named the node's file, when it did at all.
pub(super) enum PathMatch {
    /// The full relative path appeared verbatim.
    FullPath,
    /// Only the bare file stem appeared, with the evidence that admitted it.
    Stem(TermEvidence),
}

/// True when the query names the node's file, spelled either as a path or as a
/// bare stem — `src/context/pack.rs` and `pack` must both match.
///
/// The two arms are not equally trustworthy, which is why they are separate
/// variants. A slashed path in a prompt is unambiguous: nobody types
/// `src/context/pack.rs` by accident. A bare stem is just a word — `point`,
/// `write`, `quality` and `home` are all real file stems in real repositories —
/// so it goes through `gate` and earns its rung only on identifier-shaped
/// evidence.
pub(super) fn matches_path(
    query_text: &str,
    node: &SourceNode,
    gate: &ExactGate<'_>,
) -> Option<PathMatch> {
    if contains_whole_term(query_text, &node.path.display().to_string()) {
        return Some(PathMatch::FullPath);
    }
    node.path
        .file_stem()
        .and_then(|stem| gate.admits(&stem.to_string_lossy()))
        .map(PathMatch::Stem)
}

/// True when the node's file is one the query's stage dependencies own.
///
/// Exact equality on normalized paths, never a prefix: a dependency listing
/// `src/` or a glob like `src/**/*.rs` would otherwise boost the entire tree by
/// 30 points and flatten the ladder into noise. A glob entry simply matches
/// nothing here, which is the correct degradation — it is a pattern, and this
/// rung is about a specific file being the subject of work we depend on.
pub(super) fn names_dependency_path(query: &RankQuery, node: &SourceNode) -> bool {
    if query.dependency_paths.is_empty() {
        return false;
    }
    let node_path = normalize_dependency_path(&node.path.display().to_string());
    query
        .dependency_paths
        .iter()
        .any(|candidate| normalize_dependency_path(candidate) == node_path)
}

/// Normalize a project-relative path for [`RankQuery::dependency_paths`]
/// comparison: trimmed, forward-slashed, leading `./` removed.
///
/// Shared with the producer in `orchestrator::signals::retrieval` on purpose.
/// Both ends of an exact-string match must agree byte for byte, and two
/// independent normalizers agreeing today is not the same as their agreeing
/// after the next edit — a silent disagreement here shows up as a boost that
/// simply never fires, with nothing in any output to say why.
///
/// An absolute path is left absolute rather than being made relative to a root
/// this function does not know: it then matches no node, which is the honest
/// outcome for a path that was never project-relative to begin with.
pub fn normalize_dependency_path(path: &str) -> String {
    let mut normalized = path.trim().replace('\\', "/");
    while let Some(rest) = normalized.strip_prefix("./") {
        normalized = rest.to_string();
    }
    normalized
}

/// Multiply a test file's score by `test_path_factor`.
///
/// Ordering pressure, not exclusion: reasons and confidence are untouched, and
/// a test that is the ONLY match still appears in the results. Test code shares
/// almost all of its vocabulary with the code it exercises, and it usually
/// repeats the names more often, so on a pure BM25 contest a fixture builder
/// routinely outranks the function under test. What the reader asked about is
/// nearly always the implementation.
pub(super) fn apply_test_path_factor(
    node: &SourceNode,
    score: f32,
    config: &RetrievalConfig,
) -> f32 {
    let scope_says_test = node.scope.iter().any(|segment| segment == "tests");
    if scope_says_test || is_test_path(&node.path.display().to_string()) {
        return score * config.test_path_factor;
    }
    score
}

/// True for a path that follows a test-file convention in any supported
/// language.
///
/// Directory conventions (`tests/`, `test/`, `__tests__/`) cover Rust, Go and
/// the JS ecosystem; the file-name conventions cover the languages that keep
/// tests beside the code they test. Rust's in-file `mod tests` has no path to
/// inspect at all and is handled by the scope check in
/// [`apply_test_path_factor`].
fn is_test_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let mut segments = normalized.split('/').peekable();
    let mut file_name = "";
    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            file_name = segment;
        } else if matches!(segment, "tests" | "test" | "__tests__") {
            return true;
        }
    }
    is_test_file_name(file_name)
}

/// True for a bare file name matching a test-file naming convention.
fn is_test_file_name(name: &str) -> bool {
    name == "tests.rs"
        || name.ends_with("_tests.rs")
        || name.ends_with("_test.go")
        || name.ends_with("_test.py")
        || (name.starts_with("test_") && name.ends_with(".py"))
        || name.contains(".test.")
        || name.contains(".spec.")
}
