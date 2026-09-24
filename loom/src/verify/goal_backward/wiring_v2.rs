//! Plan v2 wiring rules (DESIGN D11). A `source` holding `*`, `?` or `[` is
//! a glob relative to `working_dir`, and the check passes when any file it
//! matches contains the pattern; `literal: true` matches `pattern` as plain
//! text. Reads share v1's bounded read that refuses symlinked components.
//! A match does not count when every occurrence in it of a name the file
//! defines sits on a line defining that name, so a pattern that only finds
//! the symbol's own definition is a gap.

use regex::Regex;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::definition_sites;
use super::result::{GapType, VerificationGap};
use super::wiring::{compile_pattern, missing_source_gap, not_found_gap, read_source};
use crate::context::whole_term_ranges;
use crate::plan::schema::WiringCheck;

/// The files a check's `source` names, relative to `root`.
struct Sources {
    root: PathBuf,
    files: Vec<PathBuf>,
}

/// What a passing v2 check rests on.
#[derive(Debug, PartialEq)]
pub(super) enum Pass {
    /// A counting match in a file whose definition sites were checked.
    Checked,
    /// Counting matches only in these files, whose definition sites could not
    /// be checked, so none were excluded.
    UncheckedOnly(Vec<PathBuf>),
}

/// Verify every check under the v2 rules; one gap per failing check. A pass
/// resting only on files with unchecked definition sites warns on stderr.
pub(super) fn verify_checks(wiring: &[WiringCheck], working_dir: &Path) -> Vec<VerificationGap> {
    wiring
        .iter()
        .filter_map(|check| match verify_check(check, working_dir) {
            Ok(Pass::Checked) => None,
            Ok(Pass::UncheckedOnly(files)) => {
                warn_unchecked(check, &files);
                None
            }
            Err(gap) => Some(gap),
        })
        .collect()
}

/// One v2 check: resolve `source`, compile `pattern`, then scan the files.
pub(super) fn verify_check(
    check: &WiringCheck,
    working_dir: &Path,
) -> Result<Pass, VerificationGap> {
    let sources = resolve_sources(check, working_dir)?;
    let regex = compile_pattern(check, &pattern_source(check))?;
    scan(check, &sources, &regex)
}

/// Warn that `check` passed only in `files`, where no definition site was
/// excluded.
fn warn_unchecked(check: &WiringCheck, files: &[PathBuf]) {
    let files: Vec<String> = files
        .iter()
        .map(|file| file.display().to_string())
        .collect();
    eprintln!(
        "warning: wiring check '{}' passed only in {}; definition sites there were not \
         excluded because no source-graph extractor parsed them",
        check.description,
        files.join(", ")
    );
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

/// How one file's matches count once definition sites are excluded.
enum FileMatch {
    /// The pattern does not occur in the file.
    Absent,
    /// A match counts in a file whose definition sites were checked.
    Counts,
    /// The pattern occurs, but the file's definition sites could not be
    /// checked, so every match counts.
    Unchecked,
    /// Every match is definition-only; carries the first match's name.
    DefinitionOnly(String),
}

/// Pass when any file holds a match that counts: `Checked` once a checked
/// file does, else `UncheckedOnly`. Otherwise the gap is the first file whose
/// matches are all definitions, else the first read failure when no file
/// could be read, else "Wiring not found".
fn scan(check: &WiringCheck, sources: &Sources, regex: &Regex) -> Result<Pass, VerificationGap> {
    let mut unreadable = None;
    let mut any_read = false;
    let mut definition_only = None;
    let mut unchecked = Vec::new();
    for file in &sources.files {
        match read_source(&sources.root, file) {
            Ok(content) => {
                any_read = true;
                match file_match(file, &content, regex) {
                    FileMatch::Counts => return Ok(Pass::Checked),
                    FileMatch::Unchecked => unchecked.push(file.clone()),
                    FileMatch::DefinitionOnly(name) => {
                        definition_only = definition_only.or(Some((name, file)));
                    }
                    FileMatch::Absent => {}
                }
            }
            Err(gap) => unreadable = unreadable.or(Some(gap)),
        }
    }
    if !unchecked.is_empty() {
        return Ok(Pass::UncheckedOnly(unchecked));
    }
    if let Some((name, file)) = definition_only {
        return Err(definition_only_gap(check, &name, file));
    }
    match unreadable {
        Some(gap) if !any_read => Err(gap),
        _ => Err(not_found_gap(check)),
    }
}

/// Classify `content`'s matches; a match counts unless it is definition-only
/// (see [`definition_only_name`]). A file whose definition sites cannot be
/// checked is `Unchecked`.
fn file_match(file: &Path, content: &str, regex: &Regex) -> FileMatch {
    let mut matches = regex.find_iter(content).peekable();
    if matches.peek().is_none() {
        return FileMatch::Absent;
    }
    let Some(definitions) = definition_sites::definition_lines(file, content.as_bytes()) else {
        return FileMatch::Unchecked;
    };
    let mut defined_at: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (line, name) in definitions {
        defined_at.entry(name).or_default().push(line);
    }
    let (mut line, mut scanned) = (1, 0);
    let mut excluded = None;
    for found in matches {
        line += content[scanned..found.start()].matches('\n').count();
        scanned = found.start();
        let Some(name) = definition_only_name(&defined_at, line, found.as_str()) else {
            return FileMatch::Counts;
        };
        excluded.get_or_insert_with(|| name.to_string());
    }
    excluded.map_or(FileMatch::Absent, FileMatch::DefinitionOnly)
}

/// The name of the first definition occurrence in `text`, a match starting on
/// `line`, when the match is definition-only; `None` when it counts. Each
/// identifier-bounded occurrence in `text` of a name in `defined_at` is a
/// definition occurrence on a line defining that name, else a use occurrence.
/// A match is definition-only when it holds a definition occurrence and no
/// use occurrence; a match holding neither counts.
fn definition_only_name<'a>(
    defined_at: &'a BTreeMap<String, Vec<usize>>,
    line: usize,
    text: &str,
) -> Option<&'a str> {
    let mut first: Option<(usize, &str)> = None;
    for (name, lines) in defined_at {
        // Raw case on purpose: identifiers are case-sensitive, so the
        // helper's ASCII-lowercase convention does not apply here.
        for (start, _) in whole_term_ranges(text, name) {
            if !lines.contains(&(line + text[..start].matches('\n').count())) {
                return None;
            }
            if first.is_none_or(|(earliest, _)| start < earliest) {
                first = Some((start, name.as_str()));
            }
        }
    }
    first.map(|(_, name)| name)
}

/// Gap for a pattern whose every match is definition-only.
fn definition_only_gap(check: &WiringCheck, name: &str, file: &Path) -> VerificationGap {
    VerificationGap::new(
        GapType::WiringBroken,
        format!(
            "Wiring source: pattern matches only the definition of {name} in {}; point it at a consumer ({})",
            file.display(),
            check.description
        ),
        format!(
            "Change pattern '{}' or source {} to match a consumer of {name}",
            check.pattern, check.source
        ),
    )
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
