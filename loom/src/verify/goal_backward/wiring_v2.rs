//! Plan v2 wiring rules (DESIGN D11). A `source` holding `*`, `?` or `[` is
//! a glob relative to `working_dir`, and the check passes when any file it
//! matches contains the pattern; `literal: true` matches `pattern` as plain
//! text. Reads share v1's bounded read that refuses symlinked components.

use regex::Regex;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

use super::result::{GapType, VerificationGap};
use super::wiring::{compile_pattern, missing_source_gap, not_found_gap, read_source};
use crate::plan::schema::WiringCheck;

/// The files a check's `source` names, relative to `root`.
struct Sources {
    root: PathBuf,
    files: Vec<PathBuf>,
}

/// Verify every check under the v2 rules; one gap per failing check.
pub(super) fn verify_checks(wiring: &[WiringCheck], working_dir: &Path) -> Vec<VerificationGap> {
    wiring
        .iter()
        .filter_map(|check| verify_check(check, working_dir).err())
        .collect()
}

/// One v2 check: resolve `source`, compile `pattern`, then scan the files.
fn verify_check(check: &WiringCheck, working_dir: &Path) -> Result<(), VerificationGap> {
    let sources = resolve_sources(check, working_dir)?;
    let regex = compile_pattern(check, &pattern_source(check))?;
    scan(check, &sources, &regex)
}

/// The regex a check compiles: `pattern` itself, or escaped when `literal`.
fn pattern_source(check: &WiringCheck) -> Cow<'_, str> {
    if check.literal {
        Cow::Owned(regex::escape(&check.pattern))
    } else {
        Cow::Borrowed(&check.pattern)
    }
}

/// Resolve `source` to the files it names: a glob expands inside
/// `working_dir`; any other value is a literal path, as in v1.
fn resolve_sources(check: &WiringCheck, working_dir: &Path) -> Result<Sources, VerificationGap> {
    if check.source.contains(['*', '?', '[']) {
        return expand_glob(check, working_dir);
    }
    if !working_dir.join(&check.source).exists() {
        return Err(missing_source_gap(check));
    }
    Ok(Sources {
        root: working_dir.to_path_buf(),
        files: vec![PathBuf::from(&check.source)],
    })
}

/// Expand a glob `source` to the regular files it matches, relative to the
/// canonical `working_dir`. A match that canonicalises outside `working_dir`
/// (a `..` component, an outbound symlink) is skipped.
fn expand_glob(check: &WiringCheck, working_dir: &Path) -> Result<Sources, VerificationGap> {
    let base = glob::Pattern::escape(&working_dir.to_string_lossy());
    let matches =
        glob::glob(&format!("{base}/{}", check.source)).map_err(|e| invalid_glob_gap(check, &e))?;
    let root = working_dir
        .canonicalize()
        .map_err(|_| unmatched_glob_gap(check))?;
    let files: Vec<PathBuf> = matches
        .flatten()
        .filter_map(|path| path.canonicalize().ok())
        .filter(|path| path.is_file())
        .filter_map(|path| path.strip_prefix(&root).ok().map(Path::to_path_buf))
        .collect();
    if files.is_empty() {
        return Err(unmatched_glob_gap(check));
    }
    Ok(Sources { root, files })
}

/// Pass when any file's content matches `regex`. Otherwise the gap is the
/// first read failure when no file could be read, else "Wiring not found".
fn scan(check: &WiringCheck, sources: &Sources, regex: &Regex) -> Result<(), VerificationGap> {
    let mut unreadable = None;
    let mut any_read = false;
    for file in &sources.files {
        match read_source(&sources.root, file) {
            Ok(content) if regex.is_match(&content) => return Ok(()),
            Ok(_) => any_read = true,
            Err(gap) => unreadable = unreadable.or(Some(gap)),
        }
    }
    match unreadable {
        Some(gap) if !any_read => Err(gap),
        _ => Err(not_found_gap(check)),
    }
}

/// Gap for a glob `source` that matches no file inside `working_dir`.
fn unmatched_glob_gap(check: &WiringCheck) -> VerificationGap {
    VerificationGap::new(
        GapType::WiringBroken,
        format!(
            "Wiring source: no file matches source glob {} ({})",
            check.source, check.description
        ),
        format!("Create a file matching {} or fix the glob", check.source),
    )
}

/// Gap for a `source` that is not a valid glob.
fn invalid_glob_gap(check: &WiringCheck, error: &glob::PatternError) -> VerificationGap {
    VerificationGap::new(
        GapType::WiringBroken,
        format!("Invalid wiring source glob '{}': {error}", check.source),
        "Fix the glob pattern".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::goal_backward::verify_wiring;
    use std::fs;

    fn check(source: &str, pattern: &str, literal: bool) -> WiringCheck {
        WiringCheck {
            source: source.to_string(),
            pattern: pattern.to_string(),
            description: "wiring under test".to_string(),
            literal,
        }
    }

    /// A tree where only `src/a/b.rs` holds `register_handler`.
    fn glob_tree() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        fs::create_dir_all(src.join("a")).unwrap();
        fs::write(src.join("lib.rs"), "mod a;\n").unwrap();
        fs::write(src.join("a/b.rs"), "fn f() { register_handler(); }\n").unwrap();
        root
    }

    fn run(checks: &[WiringCheck], root: &Path, plan_version: u32) -> Vec<VerificationGap> {
        verify_wiring(checks, root, plan_version).unwrap()
    }

    #[test]
    fn v2_glob_source_matches_any_file() {
        let root = glob_tree();
        let wiring = [check("src/**/*.rs", "register_handler", false)];

        let gaps = run(&wiring, root.path(), 2);

        assert!(gaps.is_empty(), "{gaps:?}");
    }

    #[test]
    fn v1_glob_source_stays_a_literal_path() {
        let root = glob_tree();
        let wiring = [check("src/**/*.rs", "register_handler", false)];

        let gaps = run(&wiring, root.path(), 1);

        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert_eq!(
            gaps[0].description,
            "Wiring source file missing: src/**/*.rs (wiring under test)"
        );
    }

    #[test]
    fn v2_literal_pattern_matches_metacharacters_literally() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("main.rs"), "fn main() { run(); }\n").unwrap();

        let literal = run(&[check("main.rs", "run(", true)], root.path(), 2);
        let regex = run(&[check("main.rs", "run(", false)], root.path(), 2);

        assert!(literal.is_empty(), "{literal:?}");
        assert_eq!(regex.len(), 1, "{regex:?}");
        assert!(
            regex[0]
                .description
                .starts_with("Invalid wiring pattern 'run(':"),
            "{regex:?}"
        );
    }

    #[test]
    fn v2_unmatched_glob_is_a_gap() {
        let root = glob_tree();
        let wiring = [check("src/**/*.py", "register_handler", false)];

        let gaps = run(&wiring, root.path(), 2);

        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert!(
            gaps[0]
                .description
                .contains("no file matches source glob src/**/*.py"),
            "{gaps:?}"
        );
    }

    #[test]
    fn v2_glob_pattern_absent_from_every_file_is_not_found() {
        let root = glob_tree();
        let wiring = [check("src/**/*.rs", "never_called", false)];

        let gaps = run(&wiring, root.path(), 2);

        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert!(
            gaps[0].description.starts_with("Wiring not found:"),
            "{gaps:?}"
        );
    }

    #[test]
    fn v2_glob_skips_match_that_resolves_outside_working_dir() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("leak.rs"), "register_handler();\n").unwrap();
        fs::create_dir(root.path().join("src")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("leak.rs"),
            root.path().join("src/leak.rs"),
        )
        .unwrap();

        let wiring = [check("src/*.rs", "register_handler", false)];

        let gaps = run(&wiring, root.path(), 2);

        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert!(
            gaps[0].description.contains("no file matches source glob"),
            "{gaps:?}"
        );
    }
}
