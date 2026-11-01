//! Check 5 of plan section 12 and the `R/loom-hooks/**` rule of section 11:
//! the hooks a user or project registers run unsandboxed in every session,
//! so none may run a file a session can write.
//!
//! Commands use the shared shell lexer and a deliberately bounded grammar.
//! Unverified syntax refuses preflight rather than implying a safe target.

use serde_json::Value;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::{find_executable, resolved, WritableRoots};
use crate::fs::permissions::constants::LOOM_HOOKS;
use crate::plan::schema::validation::shell_lex::{lex, simple_commands, Token, Word};

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
            let targets = match command_targets(&command, scan) {
                Ok(targets) => targets,
                Err(reason) => {
                    findings.refusals.push(format!(
                        "hook `{event}` in {} cannot be verified: {reason}. Use a literal executable and script path or move the logic into a protected script.",
                        document.label
                    ));
                    continue;
                }
            };
            for target in targets {
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
             under {} instead.",
            crate::assets::install::claude_hooks_dir_text()
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
fn command_targets(command: &str, scan: &HookScan<'_>) -> Result<Vec<PathBuf>, &'static str> {
    let tokens = lex(command);
    if tokens
        .iter()
        .any(|token| matches!(token, Token::Redirect(_) | Token::Control("(" | ")" | ";;")))
    {
        return Err("unsupported shell operator");
    }
    let commands = simple_commands(&tokens);
    if commands.is_empty() {
        return Err("no executable");
    }
    let mut targets = Vec::new();
    for command in commands {
        targets.extend(simple_targets(&command.words, scan)?);
    }
    Ok(targets)
}

fn simple_targets(words: &[&Word], scan: &HookScan<'_>) -> Result<Vec<PathBuf>, &'static str> {
    let mut index = program_index(words)?;
    let mut targets = Vec::new();
    let mut program = *words.get(index).ok_or("no executable")?;
    if program.value.rsplit('/').next() == Some("env") {
        targets.push(locate_word(program, scan, true)?);
        index += 1;
        while let Some(word) = words.get(index) {
            if word.value == "-i" || word.value == "--ignore-environment" {
                index += 1;
                continue;
            }
            if word.assignment_name().is_some() {
                check_assignment(word)?;
                index += 1;
                continue;
            }
            break;
        }
        program = *words.get(index).ok_or("env has no executable")?;
    }
    targets.push(locate_word(program, scan, true)?);
    let name = checked_program_name(program)?;
    for word in &words[index + 1..] {
        if !word.substitutions.is_empty() || (word.expands && !known_path_expansion(word)) {
            return Err("dynamic shell expansion");
        }
    }
    if INTERPRETERS.contains(&name) {
        targets.extend(interpreter_target(name, &words[index + 1..], scan)?);
    }
    Ok(targets)
}

fn checked_program_name(program: &Word) -> Result<&str, &'static str> {
    let name = program.value.rsplit('/').next().unwrap_or(&program.value);
    if matches!(
        name,
        "eval"
            | "source"
            | "."
            | "command"
            | "builtin"
            | "sudo"
            | "env"
            | "if"
            | "then"
            | "for"
            | "while"
            | "case"
            | "function"
    ) {
        return Err("unsupported shell command or wrapper");
    }
    Ok(name)
}

fn program_index(words: &[&Word]) -> Result<usize, &'static str> {
    let mut index = 0;
    while words
        .get(index)
        .is_some_and(|w| w.assignment_name().is_some())
    {
        check_assignment(words[index])?;
        index += 1;
    }
    if words
        .get(index)
        .is_some_and(|w| w.value == "exec" && !w.quoted)
    {
        index += 1;
    }
    Ok(index)
}

fn interpreter_target(
    name: &str,
    words: &[&Word],
    scan: &HookScan<'_>,
) -> Result<Option<PathBuf>, &'static str> {
    for word in words {
        if runs_inline_code(name, &word.value) {
            return Ok(None);
        }
        if word.value.starts_with('-') {
            if !matches!(word.value.as_str(), "-x" | "-v" | "-u" | "-E" | "-I") {
                return Err("unsupported interpreter option");
            }
        } else {
            return locate_word(word, scan, false).map(Some);
        }
    }
    Ok(None)
}

fn check_assignment(word: &Word) -> Result<(), &'static str> {
    if word.expands
        || !word.substitutions.is_empty()
        || matches!(
            word.assignment_name(),
            Some("PATH" | "HOME" | "CLAUDE_PROJECT_DIR")
        )
    {
        Err("dynamic assignment or path environment override")
    } else {
        Ok(())
    }
}

fn locate_word(word: &Word, scan: &HookScan<'_>, program: bool) -> Result<PathBuf, &'static str> {
    if !word.substitutions.is_empty() || (word.expands && !known_path_expansion(word)) {
        return Err("dynamic executable or script path");
    }
    if !word.expands && word.value.starts_with('$') {
        return Err("literal dollar-prefixed path cannot be verified");
    }
    if word.quoted && word.value.starts_with('~') {
        return Err("quoted tilde path cannot be verified");
    }
    if word.value.chars().any(|c| matches!(c, '*' | '?' | '['))
        || (!word.expands && word.value.contains('{'))
    {
        return Err("shell pattern in executable or script path");
    }
    if (word.value.starts_with("$HOME/")
        || word.value.starts_with("${HOME}/")
        || word.value.starts_with("~/"))
        && scan.home.is_none()
    {
        return Err("home directory is unavailable");
    }
    locate(&word.value, scan, program).ok_or("executable was not found on PATH")
}

fn known_path_expansion(word: &Word) -> bool {
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

/// Whether `flag` makes `interpreter` run the code that follows it.
fn runs_inline_code(interpreter: &str, flag: &str) -> bool {
    match interpreter {
        "sh" | "bash" | "zsh" | "dash" | "python" | "python3" => flag == "-c",
        _ => flag == "-e" || flag == "--eval",
    }
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
