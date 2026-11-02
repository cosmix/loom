//! Where a hook command's words point, and the one rule over what it passes: no
//! argument or environment value may resolve inside a writable root, whatever
//! the program, because the program can run or load what it is given.

use std::path::{Path, PathBuf};

use super::super::exec_wrappers::{is_shell, Reason};
use super::super::{find_executable, resolved, WritableRoots};
use super::HookScan;
use crate::plan::schema::validation::shell_lex::Word;

pub(super) fn locate_word(
    word: &Word,
    scan: &HookScan<'_>,
    program: bool,
) -> Result<PathBuf, Reason> {
    if !word.substitutions.is_empty() || (word.expands && !known_path_expansion(word)) {
        return Err("dynamic executable or script path".into());
    }
    if !word.expands && word.value.starts_with('$') {
        return Err("literal dollar-prefixed path cannot be verified".into());
    }
    if word.quoted && word.value.starts_with('~') {
        return Err("quoted tilde path cannot be verified".into());
    }
    if has_unquoted_pattern(&word.raw) {
        return Err("shell pattern in executable or script path".into());
    }
    if (word.value.starts_with("$HOME/")
        || word.value.starts_with("${HOME}/")
        || word.value.starts_with("~/"))
        && scan.home.is_none()
    {
        return Err("home directory is unavailable".into());
    }
    locate(&word.value, scan, program).ok_or_else(|| "executable was not found on PATH".into())
}

/// Whether `raw`, as written, holds a character the shell expands as a pattern.
/// A quoted or escaped one is literal, which is how Loom's own registrations
/// spell a hooks directory that holds one.
fn has_unquoted_pattern(raw: &str) -> bool {
    let mut chars = raw.chars();
    let mut quote = None;
    let mut previous = ' ';
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (Some('"'), '\\') | (None, '\\') => {
                chars.next();
            }
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, '*' | '?' | '[') => return true,
            (None, '{') if previous != '$' => return true,
            (None, _) => {}
        }
        previous = c;
    }
    false
}

pub(super) fn known_path_expansion(word: &Word) -> bool {
    [
        "$HOME/",
        "${HOME}/",
        "$CLAUDE_PROJECT_DIR/",
        "${CLAUDE_PROJECT_DIR}/",
    ]
    .iter()
    .any(|prefix| word.value.starts_with(prefix))
        && word.value.matches('$').count() == 1
}

/// Where `word` points: expanded against the home directory and the project,
/// a relative path taken from the project (hooks run there), and a bare
/// program name found on PATH. `None` for a program PATH does not hold.
fn locate(word: &str, scan: &HookScan<'_>, program: bool) -> Option<PathBuf> {
    let path = expand(word, scan);
    if path.is_absolute() {
        Some(path)
    } else if program && !word.contains('/') {
        find_executable(word, scan.path)
    } else {
        Some(scan.repo_root.join(path))
    }
}

/// `word` with a leading `~/`, `$HOME/` or `$CLAUDE_PROJECT_DIR/` (braced or
/// not) expanded.
fn expand(word: &str, scan: &HookScan<'_>) -> PathBuf {
    let bases = [
        ("~", scan.home),
        ("$HOME", scan.home),
        ("${HOME}", scan.home),
        ("$CLAUDE_PROJECT_DIR", Some(scan.repo_root)),
        ("${CLAUDE_PROJECT_DIR}", Some(scan.repo_root)),
    ];
    for (prefix, base) in bases {
        let rest = word
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_prefix('/'));
        if let (Some(rest), Some(base)) = (rest, base) {
            return base.join(rest);
        }
    }
    PathBuf::from(word)
}

/// Refuse a program's own arguments when one resolves inside a writable root.
/// `targets` are already judged as what the command runs, with their own
/// wording. Inline shell code after `-c` is judged as the commands it holds.
pub(super) fn check_argument_paths(
    args: &[&Word],
    family: Option<&str>,
    targets: &[PathBuf],
    scan: &HookScan<'_>,
) -> Result<(), Reason> {
    let roots = WritableRoots::new(scan.writable_roots);
    let shell = family.is_some_and(is_shell);
    let is_code = |index: usize| shell && index > 0 && args[index - 1].value == "-c";
    for (_, word) in args
        .iter()
        .enumerate()
        .filter(|(index, _)| !is_code(*index))
    {
        for segment in argument_segments(&word.value) {
            let Some(path) = named_path(segment, false, scan) else {
                continue;
            };
            let path = resolved(&path);
            let Some(root) = roots.containing(&path) else {
                continue;
            };
            if !targets.iter().any(|target| resolved(target) == path) {
                return Err(under_root(
                    &format!("argument `{}`", word.value),
                    &path,
                    root,
                ));
            }
        }
    }
    Ok(())
}

/// Refuse an environment assignment whose value resolves inside a writable
/// root: `BASH_ENV`, `LD_PRELOAD`, `PYTHONPATH` and their like load from it.
pub(super) fn check_assignment_path(word: &Word, scan: &HookScan<'_>) -> Result<(), Reason> {
    let roots = WritableRoots::new(scan.writable_roots);
    let value = word.value.split_once('=').map_or("", |(_, value)| value);
    for segment in value.split(SEPARATORS).filter(|part| !part.is_empty()) {
        let Some(path) = named_path(segment, true, scan) else {
            continue;
        };
        let path = resolved(&path);
        if let Some(root) = roots.containing(&path) {
            return Err(under_root(
                &format!("assignment `{}`", word.value),
                &path,
                root,
            ));
        }
    }
    Ok(())
}

fn under_root(what: &str, path: &Path, root: &Path) -> Reason {
    format!(
        "{what} resolves to {}, under the session-writable root {}, and a session could replace \
         what it loads. Move it outside every writable root",
        path.display(),
        root.display()
    )
    .into()
}

/// What separates the paths of a search list or a list-valued option.
const SEPARATORS: [char; 2] = [':', ','];

/// The parts of an argument that can name a path: an option's value (`--x=v`,
/// `-Xv`), and for any other word the whole of it and what follows an `=`
/// (`if=/path`), each split as a list.
fn argument_segments(value: &str) -> impl Iterator<Item = &str> {
    let after_equals = value.split_once('=').map(|(_, rest)| rest);
    let (whole, part) = match value.strip_prefix('-') {
        Some(rest) if rest.starts_with('-') => (None, after_equals),
        Some(_) => (None, after_equals.or_else(|| value.get(2..))),
        None => (Some(value), after_equals),
    };
    [whole, part]
        .into_iter()
        .flatten()
        .flat_map(|text| text.split(SEPARATORS))
        .filter(|segment| !segment.is_empty())
}

/// The path `segment` names, if it names one. Absolute, home- or project-rooted
/// and `./`-style words always do. Any other word is ordinary text (`5`,
/// `America/New_York`, a subcommand) unless the project holds it, where a hook's
/// working directory finds it, or PATH finds it as a program. A bare `value` of
/// an assignment may name a directory; a bare argument must name a file, so a
/// subcommand that shares a directory's name stays text.
fn named_path(segment: &str, value: bool, scan: &HookScan<'_>) -> Option<PathBuf> {
    let path = expand(segment, scan);
    if path.is_absolute() {
        return Some(path);
    }
    let in_project = scan.repo_root.join(&path);
    let dotted =
        matches!(segment, "." | "..") || segment.starts_with("./") || segment.starts_with("../");
    if dotted {
        return Some(in_project);
    }
    let metadata = std::fs::symlink_metadata(&in_project).ok();
    let bare = !segment.contains('/');
    if metadata.is_some_and(|meta| value || !bare || !meta.is_dir()) {
        return Some(in_project);
    }
    if bare {
        return find_executable(segment, scan.path);
    }
    None
}
