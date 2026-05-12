//! Image fingerprinting for the container backend.
//!
//! A fingerprint uniquely identifies the container image variant needed for a
//! given project: it encodes which programming languages are present AND the
//! content of the THREE embedded resources that control image shape
//! (`Dockerfile.tmpl`, `firewall.sh`, and `entrypoint.sh`). Any change to the
//! detected language set OR any of the three embedded resources produces a
//! different fingerprint, which in turn triggers a new image build.
//!
//! Codex correction (MN4): the previous version hashed Dockerfile + firewall
//! only and missed `entrypoint.sh`. Stage 4 (this stage) extends the
//! entrypoint significantly — without entrypoint in the hash, cached images
//! would not rebuild on entrypoint changes and operators would silently run
//! the old (insecure) entrypoint. See doc/loom/knowledge/architecture.md.
//!
//! Output format: `"{langs}-{hex[:16]}"` where `langs` is the sorted,
//! deduplicated list of canonical language names joined by `"-"` (or
//! `"base"` for empty sets), and `hex[:16]` is the first 16 hex characters
//! of SHA-256 over the hash input described below. The 16-char hash gives
//! 64 bits of collision resistance — enough to make accidental collisions
//! between independently-tweaked images vanishingly unlikely (the prior
//! 8 hex / 32 bits left a real-world collision risk on busy build farms).

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::language::detect_project_languages;
use crate::verify::transitions::list_all_stages;

/// Embedded Dockerfile template (content only; not written to disk here).
const DOCKERFILE_TMPL: &str = include_str!("../../../../resources/Dockerfile.tmpl");

/// Embedded firewall script (content only; not written to disk here).
const FIREWALL_SH: &str = include_str!("../../../../resources/firewall.sh");

/// Embedded entrypoint script — included in the fingerprint so that
/// hardening changes to the entrypoint (root-check, sudoers cleanup,
/// firewall verification) invalidate cached images.
pub(crate) const ENTRYPOINT_SCRIPT: &str = include_str!("../../../../resources/entrypoint.sh");

/// Collect unique stage `working_dir` values from `.work/stages/*.md` so
/// callers can pass them to [`compute_fingerprint`]. This is what makes
/// language detection actually scan where a project's manifests live (e.g.
/// `loom/Cargo.toml` for a monorepo where the rust crate is in a subdir).
///
/// Empty / `"."` entries are filtered out — they refer to the project root,
/// which `compute_fingerprint` already scans on its own.
///
/// Returns an empty Vec when `.work/stages/` is missing or no stage files
/// parsed; callers fall back to project-root-only detection in that case.
pub fn plan_working_dirs(work_dir: &Path) -> Vec<PathBuf> {
    let stages = match list_all_stages(work_dir) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut dirs: Vec<PathBuf> = stages
        .into_iter()
        .filter_map(|s| s.working_dir)
        .filter(|wd| !wd.is_empty() && wd != ".")
        .map(PathBuf::from)
        .collect();
    dirs.sort();
    dirs.dedup();
    dirs
}

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

    compute_fingerprint_inner(
        &canonical_names,
        DOCKERFILE_TMPL,
        FIREWALL_SH,
        ENTRYPOINT_SCRIPT,
    )
}

/// Inner implementation, exposed for unit testing.
///
/// Accepts the already-resolved canonical language names plus the content of
/// the THREE embedded resources (Dockerfile, firewall, entrypoint), so tests
/// can vary them independently at compile time without having to modify the
/// embedded constants.
pub(crate) fn compute_fingerprint_inner(
    canonical_names: &[&str],
    dockerfile_content: &str,
    firewall_content: &str,
    entrypoint_content: &str,
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

    // 5. SHA-256 over: Dockerfile + firewall + entrypoint + feature fragment.
    //    Codex correction: entrypoint was missing from the hash input, so
    //    cached images would not rebuild on entrypoint changes. With Stage 4
    //    extending the entrypoint extensively, this was no longer safe.
    let mut hasher = Sha256::new();
    hasher.update(dockerfile_content.as_bytes());
    hasher.update(firewall_content.as_bytes());
    hasher.update(entrypoint_content.as_bytes());
    hasher.update(feature_fragment.as_bytes());
    let digest = hasher.finalize();
    let hex_full = hex::encode(digest);
    // Widened from 8 → 16 hex chars (32 → 64 bits) for stronger collision
    // resistance across the build farm. Safe to use chars() per mistakes.md
    // "String Handling: UTF-8 Truncation Panic".
    let hex16: String = hex_full.chars().take(16).collect();

    format!("{langs_prefix}-{hex16}")
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

    // --- tests for plan_working_dirs ---

    fn write_stage_with_working_dir(stages_dir: &Path, stage_id: &str, working_dir: Option<&str>) {
        let stage = crate::models::stage::Stage {
            id: stage_id.to_string(),
            name: stage_id.to_string(),
            working_dir: working_dir.map(|s| s.to_string()),
            ..crate::models::stage::Stage::default()
        };
        let content = crate::verify::transitions::serialize_stage_to_markdown(&stage).unwrap();
        fs::write(stages_dir.join(format!("01-{stage_id}.md")), content).unwrap();
    }

    #[test]
    fn plan_working_dirs_returns_empty_when_no_work_dir() {
        let temp = TempDir::new().unwrap();
        // No .work/stages/ exists.
        assert_eq!(plan_working_dirs(temp.path()), Vec::<PathBuf>::new());
    }

    #[test]
    fn plan_working_dirs_returns_empty_when_stages_dir_missing() {
        let temp = TempDir::new().unwrap();
        fs::create_dir(temp.path().join("other")).unwrap();
        assert_eq!(plan_working_dirs(temp.path()), Vec::<PathBuf>::new());
    }

    #[test]
    fn plan_working_dirs_collects_unique_non_root_dirs() {
        let temp = TempDir::new().unwrap();
        let stages_dir = temp.path().join("stages");
        fs::create_dir_all(&stages_dir).unwrap();

        // Two stages share working_dir=loom, one uses frontend, one uses "."
        // (root, should be filtered), one is None (also filtered).
        write_stage_with_working_dir(&stages_dir, "alpha", Some("loom"));
        write_stage_with_working_dir(&stages_dir, "beta", Some("loom"));
        write_stage_with_working_dir(&stages_dir, "gamma", Some("frontend"));
        write_stage_with_working_dir(&stages_dir, "delta", Some("."));
        write_stage_with_working_dir(&stages_dir, "epsilon", None);

        let dirs = plan_working_dirs(temp.path());
        assert_eq!(
            dirs,
            vec![PathBuf::from("frontend"), PathBuf::from("loom")],
            "got: {dirs:?}"
        );
    }

    #[test]
    fn plan_working_dirs_unblocks_nested_rust_detection() {
        // End-to-end: a project with no root Cargo.toml but a stage whose
        // working_dir points at a subdir containing Cargo.toml MUST yield a
        // rust-prefixed fingerprint when callers feed plan_working_dirs into
        // compute_fingerprint. This regression-locks the bug where the four
        // production callers were passing &[] and missing nested manifests.
        let project = TempDir::new().unwrap();
        let work_dir = project.path().join(".work");
        let stages_dir = work_dir.join("stages");
        fs::create_dir_all(&stages_dir).unwrap();

        // Cargo.toml lives in `loom/`, NOT at project root.
        let crate_dir = project.path().join("loom");
        fs::create_dir_all(&crate_dir).unwrap();
        fs::write(crate_dir.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        write_stage_with_working_dir(&stages_dir, "build", Some("loom"));

        let working_dirs = plan_working_dirs(&work_dir);
        assert_eq!(working_dirs, vec![PathBuf::from("loom")]);

        let fp = compute_fingerprint(project.path(), &working_dirs);
        assert!(
            fp.starts_with("rust-"),
            "fingerprint must include rust when stage working_dir points at the crate, got: {fp}"
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
        let fp1 = compute_fingerprint_inner(
            &["rust"],
            "FROM ubuntu:22.04",
            "iptables -A INPUT",
            "exec gosu loom",
        );
        let fp2 = compute_fingerprint_inner(
            &["rust"],
            "FROM ubuntu:22.04",
            "iptables -A OUTPUT",
            "exec gosu loom",
        );

        assert_ne!(
            fp1, fp2,
            "different firewall content must produce different fingerprints"
        );
    }

    #[test]
    fn test_dockerfile_content_affects_fingerprint() {
        let fp1 = compute_fingerprint_inner(
            &["rust"],
            "FROM ubuntu:22.04",
            "iptables -A INPUT",
            "exec gosu loom",
        );
        let fp2 = compute_fingerprint_inner(
            &["rust"],
            "FROM debian:12",
            "iptables -A INPUT",
            "exec gosu loom",
        );

        assert_ne!(
            fp1, fp2,
            "different Dockerfile content must produce different fingerprints"
        );
    }

    #[test]
    fn test_entrypoint_content_affects_fingerprint() {
        // Regression for Codex MN4 finding: entrypoint must be in the hash.
        // Without this, hardening changes to entrypoint.sh do not invalidate
        // cached images and operators silently keep the old entrypoint.
        let fp1 = compute_fingerprint_inner(
            &["rust"],
            "FROM ubuntu:22.04",
            "iptables -A INPUT",
            "exec gosu loom",
        );
        let fp2 = compute_fingerprint_inner(
            &["rust"],
            "FROM ubuntu:22.04",
            "iptables -A INPUT",
            "exec gosu loom -- bash -lc 'echo new entrypoint'",
        );

        assert_ne!(
            fp1, fp2,
            "different entrypoint content must produce different fingerprints"
        );
    }

    #[test]
    fn test_fingerprint_format_structure() {
        // Verify exact output shape: "{prefix}-{16 hex chars}".
        let fp = compute_fingerprint_inner(&["go", "rust"], "FROM x", "fw", "ep");

        // Prefix for sorted ["go", "rust"] is "go-rust".
        assert!(fp.starts_with("go-rust-"), "got: {fp}");

        // Suffix (after the last '-') must be exactly 16 lowercase hex chars.
        let suffix = fp.rsplit('-').next().unwrap();
        assert_eq!(suffix.len(), 16, "hash suffix must be 16 chars, got: {fp}");
        assert!(
            suffix.chars().all(|c| c.is_ascii_hexdigit()),
            "suffix must be hex, got: {fp}"
        );
    }

    #[test]
    fn test_fingerprint_inner_empty_langs() {
        let fp = compute_fingerprint_inner(&[], "FROM x", "fw", "ep");
        assert!(
            fp.starts_with("base-"),
            "empty lang set must produce 'base-' prefix, got: {fp}"
        );
    }
}
