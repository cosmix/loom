//! Wire types: paths.

/// Convert a repo-relative document path to a doc-relative path.
pub fn to_doc_rel(repo_rel: &str) -> Option<&str> {
    repo_rel.strip_prefix("doc/")
}

/// Convert a doc-relative path to a repo-relative document path.
pub fn to_repo_rel(doc_rel: &str) -> String {
    format!("doc/{doc_rel}")
}
