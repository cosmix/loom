//! Image fingerprinting for the container backend.
//!
//! A fingerprint uniquely identifies the container image variant needed for a
//! given project: it encodes which programming languages are present AND the
//! content of the two embedded resources that control image shape
//! (Dockerfile.tmpl and firewall.sh).  Any change to the detected language
//! set, the Dockerfile template, or the firewall script produces a different
//! fingerprint, which in turn triggers a new image build.
//!
//! Output format: `"{langs}-{hex[:8]}"` where `langs` is the sorted,
//! deduplicated list of canonical language names joined by `"-"` (or
//! `"base"` for empty sets), and `hex[:8]` is the first 8 hex characters of
//! SHA-256 over the hash input described below.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::language::detect_project_languages;

/// Embedded Dockerfile template (content only; not written to disk here).
const DOCKERFILE_TMPL: &str = include_str!("../../../../resources/Dockerfile.tmpl");

/// Embedded firewall script (content only; not written to disk here).
const FIREWALL_SH: &str = include_str!("../../../../resources/firewall.sh");

/// Compute a stable fingerprint for the container image that would be built
/// for this project configuration.
///
/// Scanning is performed on `project_root` and on each element of
/// `plan_working_dirs` (resolved against `project_root` when relative).
/// The resulting language list is sorted and deduplicated by
/// [`canonical_name`](crate::language::DetectedLanguage::canonical_name).
///
/// The fingerprint embeds the current content of `Dockerfile.tmpl` and
/// `firewall.sh` so that changes to either resource automatically
/// invalidate the cached image without requiring a manual rebuild.
pub fn compute_fingerprint(project_root: &Path, plan_working_dirs: &[PathBuf]) -> String {
    // 1. Collect all scan roots (project root + resolved working dirs).
    let mut all_roots: Vec<PathBuf> = Vec::with_capacity(1 + plan_working_dirs.len());
    all_roots.push(project_root.to_path_buf());
    for wd in plan_working_dirs {
        if wd.is_absolute() {
            all_roots.push(wd.clone());
        } else {
            all_roots.push(project_root.join(wd));
        }
    }

    // 2. Detect languages in every root, dedup by canonical name.
    let mut canonical_names: Vec<&'static str> = all_roots
        .iter()
        .flat_map(|root| detect_project_languages(root))
        .map(|lang| lang.canonical_name())
        .collect();
    canonical_names.sort_unstable();
    canonical_names.dedup();

    compute_fingerprint_inner(&canonical_names, DOCKERFILE_TMPL, FIREWALL_SH)
}

/// Inner implementation, exposed for unit testing.
///
/// Accepts the already-resolved canonical language names plus the content of
/// the two embedded resources, so tests can vary them independently at
/// compile time without having to modify the embedded constants.
pub(crate) fn compute_fingerprint_inner(
    canonical_names: &[&str],
    dockerfile_content: &str,
    firewall_content: &str,
) -> String {
    // 3. Build lang prefix (sorted inputs are assumed by caller).
    let langs_prefix = if canonical_names.is_empty() {
        "base".to_string()
    } else {
        canonical_names.join("-")
    };

    // 4. Build deterministic feature-switch fragment.
    //    Keys are sorted so that the output is identical regardless of
    //    detection order.  We use four fixed keys matching the four
    //    currently-supported languages.
    let has_rust = canonical_names.contains(&"rust");
    let has_typescript = canonical_names.contains(&"typescript");
    let has_python = canonical_names.contains(&"python");
    let has_go = canonical_names.contains(&"go");

    let feature_fragment = format!(
        "has_go={has_go}\nhas_python={has_python}\nhas_rust={has_rust}\nhas_typescript={has_typescript}\n"
    );

    // 5. SHA-256 over: Dockerfile content + firewall content + feature fragment.
    let mut hasher = Sha256::new();
    hasher.update(dockerfile_content.as_bytes());
    hasher.update(firewall_content.as_bytes());
    hasher.update(feature_fragment.as_bytes());
    let digest = hasher.finalize();
    let hex_full = hex::encode(digest);
    // Safe to use chars() per mistakes.md "String Handling: UTF-8 Truncation Panic".
    let hex8: String = hex_full.chars().take(8).collect();

    format!("{langs_prefix}-{hex8}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // --- tests for compute_fingerprint (public API) ---

    #[test]
    fn test_fingerprint_detects_nested_rust() {
        let root = TempDir::new().unwrap();
        // Create a Cargo.toml in a subdirectory, NOT in the project root.
        let subdir = root.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        let fp = compute_fingerprint(root.path(), &[PathBuf::from("subdir")]);

        // The subdir is passed as a plan working dir, so Rust must be detected.
        assert!(
            fp.starts_with("rust-"),
            "expected fingerprint to start with 'rust-', got: {fp}"
        );
    }

    #[test]
    fn test_fingerprint_deterministic() {
        let root = TempDir::new().unwrap();
        fs::write(root.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        let fp1 = compute_fingerprint(root.path(), &[]);
        let fp2 = compute_fingerprint(root.path(), &[]);

        assert_eq!(fp1, fp2, "fingerprint must be deterministic across calls");
    }

    #[test]
    fn test_canonical_go_in_fingerprint_prefix() {
        let root = TempDir::new().unwrap();
        fs::write(
            root.path().join("go.mod"),
            "module example.com/myapp\n\ngo 1.22",
        )
        .unwrap();

        let fp = compute_fingerprint(root.path(), &[]);

        // Regression for finding #18: must be "go-...", NOT "golang-...".
        assert!(
            fp.starts_with("go-"),
            "expected fingerprint to start with 'go-', got: {fp}"
        );
        assert!(
            !fp.starts_with("golang-"),
            "fingerprint must NOT start with 'golang-', got: {fp}"
        );
    }

    #[test]
    fn test_fingerprint_fallback_base() {
        let root = TempDir::new().unwrap();
        // Empty directory — no language manifests.

        let fp = compute_fingerprint(root.path(), &[]);

        assert!(
            fp.starts_with("base-"),
            "expected fingerprint to start with 'base-', got: {fp}"
        );
    }

    // --- tests for compute_fingerprint_inner (testability of resource embedding) ---

    #[test]
    fn test_firewall_content_affects_fingerprint() {
        // Regression for finding #19: flipping firewall content must change fingerprint.
        let fp1 = compute_fingerprint_inner(&["rust"], "FROM ubuntu:22.04", "iptables -A INPUT");
        let fp2 = compute_fingerprint_inner(&["rust"], "FROM ubuntu:22.04", "iptables -A OUTPUT");

        assert_ne!(
            fp1, fp2,
            "different firewall content must produce different fingerprints"
        );
    }

    #[test]
    fn test_dockerfile_content_affects_fingerprint() {
        let fp1 = compute_fingerprint_inner(&["rust"], "FROM ubuntu:22.04", "iptables -A INPUT");
        let fp2 = compute_fingerprint_inner(&["rust"], "FROM debian:12", "iptables -A INPUT");

        assert_ne!(
            fp1, fp2,
            "different Dockerfile content must produce different fingerprints"
        );
    }

    #[test]
    fn test_fingerprint_format_structure() {
        // Verify exact output shape: "{prefix}-{8 hex chars}".
        let fp = compute_fingerprint_inner(&["go", "rust"], "FROM x", "fw");

        // Prefix for sorted ["go", "rust"] is "go-rust".
        assert!(fp.starts_with("go-rust-"), "got: {fp}");

        // Suffix (after the last '-') must be exactly 8 lowercase hex chars.
        let suffix = fp.rsplit('-').next().unwrap();
        assert_eq!(suffix.len(), 8, "hash suffix must be 8 chars, got: {fp}");
        assert!(
            suffix.chars().all(|c| c.is_ascii_hexdigit()),
            "suffix must be hex, got: {fp}"
        );
    }

    #[test]
    fn test_fingerprint_inner_empty_langs() {
        let fp = compute_fingerprint_inner(&[], "FROM x", "fw");
        assert!(
            fp.starts_with("base-"),
            "empty lang set must produce 'base-' prefix, got: {fp}"
        );
    }
}
