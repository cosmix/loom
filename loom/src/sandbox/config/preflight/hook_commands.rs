//! Check 5 of plan section 12 and the `R/loom-hooks/**` rule of section 11:
//! the hooks a user or project registers run unsandboxed in every session,
//! so none may run a file a session can write.
//!
//! A registration's command runs through a shell, so it is read the way that
//! shell starts it: leading `NAME=value` assignments and an `exec` are
//! skipped, the first word is the program (a bare name resolves through
//! PATH), and for an interpreter such as `bash` or `python3` the first
//! argument that is not a flag is the script it runs, unless a flag like `-c`
//! makes it run inline code instead.

use serde_json::Value;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::{find_executable, resolved, WritableRoots};
use crate::fs::permissions::constants::LOOM_HOOKS;

/// Programs whose first non-flag argument is the file they run.
const INTERPRETERS: [&str; 11] = [
    "sh", "bash", "zsh", "dash", "python", "python3", "node", "bun", "deno", "ruby", "perl",
];

/// A settings document whose `hooks` block Claude Code runs, and the label
/// findings name it by.
pub(crate) struct HookDocument {
    pub label: String,
    pub settings: Value,
}

/// What `scan_hook_registrations` reads.
pub(crate) struct HookScan<'a> {
    pub documents: &'a [HookDocument],
    pub repo_root: &'a Path,
    pub home: Option<&'a Path>,
    /// The PATH a bare program name resolves through; hooks inherit the
    /// session's own.
    pub path: &'a [PathBuf],
    pub writable_roots: &'a [PathBuf],
}

/// Refusals, and warnings that do not refuse.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct HookFindings {
    pub refusals: Vec<String>,
    pub warnings: Vec<String>,
}

/// The documents whose hooks every loom session runs: `R/.claude/settings.json`,
/// `R/.claude/settings.local.json` and `~/.claude/settings.json`. An absent
/// or unparseable one is skipped.
pub(crate) fn hook_documents(repo_root: &Path, home: Option<&Path>) -> Vec<HookDocument> {
    let claude = repo_root.join(".claude");
    let mut paths = vec![
        claude.join("settings.json"),
        claude.join("settings.local.json"),
    ];
    paths.extend(home.map(|home| home.join(".claude").join("settings.json")));
    paths
        .into_iter()
        .filter_map(|path| {
            let settings = serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
            Some(HookDocument {
                label: path.display().to_string(),
                settings,
            })
        })
        .collect()
}

/// Check 5 and the `R/loom-hooks/**` rule over every registration in
/// `scan.documents`, plus a warning for a hook file with more than one hard
/// link.
pub(crate) fn scan_hook_registrations(scan: &HookScan<'_>) -> HookFindings {
    let roots = WritableRoots::new(scan.writable_roots);
    let repo = resolved(scan.repo_root);
    let mut findings = HookFindings::default();
    for document in scan.documents {
        for (event, command) in hook_commands(&document.settings) {
            for target in command_targets(&command, scan) {
                let target = resolved(&target);
                let hook = format!(
                    "hook `{event}` in {} runs {}",
                    document.label,
                    target.display()
                );
                if let Some(root) = roots.containing(&target) {
                    findings.refusals.push(refusal(&hook, &target, &repo, root));
                } else if let Some(links) = extra_links(&target) {
                    findings.warnings.push(format!(
                        "{hook}, which has {links} hard links: another link can change it. \
                         Replace it with a single-link copy."
                    ));
                }
            }
        }
    }
    findings
}

/// The refusal for a hook running `target` under the writable `root`; a loom
/// script inside the repository gets the `R/loom-hooks/**` wording.
fn refusal(hook: &str, target: &Path, repo: &Path, root: &Path) -> String {
    if is_loom_script(target) && target.starts_with(repo) {
        format!(
            "loom {hook}, inside the repository: stages write the repository by design, so \
             the hook would run session-written code unsandboxed. Register the installed copy \
             under ~/.claude/hooks/loom instead."
        )
    } else {
        format!(
            "{hook}, under the session-writable root {}: a session could replace it, and hooks \
             run unsandboxed in every session. Move it outside every writable root.",
            root.display()
        )
    }
}

/// Every `(event, command)` pair in a settings document's `hooks` block;
/// malformed entries are skipped. Delegates the walk to
/// `fs::permissions::flatten_hook_triples`, dropping the matcher this scan
/// does not need.
fn hook_commands(settings: &Value) -> Vec<(String, String)> {
    let Some(hooks) = settings.get("hooks") else {
        return Vec::new();
    };
    crate::fs::permissions::flatten_hook_triples(hooks)
        .into_iter()
        .map(|(event, _matcher, command)| (event, command))
        .collect()
}

/// The files a hook command runs: its program and, for an interpreter, the
/// script it is given.
fn command_targets(command: &str, scan: &HookScan<'_>) -> Vec<PathBuf> {
    let mut words = command
        .split_whitespace()
        .map(|word| word.trim_matches(|c| c == '"' || c == '\''))
        .skip_while(|word| *word == "exec" || is_assignment(word));
    let Some(program) = words.next() else {
        return Vec::new();
    };
    let mut targets: Vec<PathBuf> = locate(program, scan, true).into_iter().collect();
    let name = program.rsplit('/').next().unwrap_or(program);
    if INTERPRETERS.contains(&name) {
        let script = words
            .take_while(|word| !runs_inline_code(name, word))
            .find(|word| !word.starts_with('-'));
        targets.extend(script.and_then(|word| locate(word, scan, false)));
    }
    targets
}

/// Whether `flag` makes `interpreter` run the code that follows it.
fn runs_inline_code(interpreter: &str, flag: &str) -> bool {
    match interpreter {
        "sh" | "bash" | "zsh" | "dash" | "python" | "python3" => flag == "-c",
        _ => flag == "-e" || flag == "--eval",
    }
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        name.starts_with(|c: char| c == '_' || c.is_ascii_alphabetic())
            && name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
    })
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

/// Whether `path` is named like one of loom's own hook scripts.
fn is_loom_script(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| LOOM_HOOKS.iter().any(|(script, _)| *script == name))
}

/// The hard-link count of a regular file the operator owns and that has more
/// than one. A file owned by someone else cannot be changed through any of
/// its links, so its link count says nothing.
fn extra_links(path: &Path) -> Option<u64> {
    let metadata = std::fs::metadata(path).ok()?;
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let operator = unsafe { libc::getuid() };
    let linked = metadata.is_file() && metadata.uid() == operator && metadata.nlink() > 1;
    linked.then(|| metadata.nlink())
}
