//! Check 5 of plan section 12 and the `R/loom-hooks/**` rule of section 11:
//! the hooks a user or project registers run unsandboxed in every session,
//! so none may run a file a session can write.
//!
//! Commands use the shared shell lexer and a deliberately bounded grammar.
//! Unverified syntax refuses preflight rather than implying a safe target, and a
//! command resolves through every wrapper and inline shell string to what it
//! finally runs (`exec_wrappers`). Whatever the program, no argument or
//! environment value of it may resolve inside a writable root (`paths`).

use serde_json::Value;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::exec_wrappers::{
    check_followable, interpreter_family, interpreter_option_width, is_shell, runs_inline_code,
    runtime_script, shell_runs_inert, wrapped_command, Reason,
};
use super::{resolved, WritableRoots};
use crate::fs::permissions::constants::LOOM_HOOKS;
use crate::plan::schema::validation::shell_lex::{lex, simple_commands, Token, Word};

mod paths;
use paths::{check_argument_paths, check_assignment_path, known_path_expansion, locate_word};

/// How deep wrappers and inline shell strings may nest in one hook command.
const MAX_NESTING: usize = 8;

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
            let targets = match command_targets(&command, scan, 0) {
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

/// The files a hook command runs: its program, what a wrapper execs, and the
/// script or inline shell an interpreter is given.
fn command_targets(
    command: &str,
    scan: &HookScan<'_>,
    depth: usize,
) -> Result<Vec<PathBuf>, Reason> {
    let tokens = lex(command);
    if tokens
        .iter()
        .any(|token| matches!(token, Token::Redirect(_) | Token::Control("(" | ")" | ";;")))
    {
        return Err("unsupported shell operator".into());
    }
    let commands = simple_commands(&tokens);
    if commands.is_empty() {
        return Err("no executable".into());
    }
    let mut targets = Vec::new();
    for command in commands {
        targets.extend(simple_targets(&command.words, scan, depth)?);
    }
    Ok(targets)
}

/// One simple command as the shell runs it: a builtin needs no file, and `exec`
/// or a later wrapper runs a builtin's name from PATH.
fn simple_targets(
    words: &[&Word],
    scan: &HookScan<'_>,
    depth: usize,
) -> Result<Vec<PathBuf>, Reason> {
    let (index, exec) = program_index(words, scan)?;
    let words = &words[index..];
    let program = words.first().ok_or("no executable")?;
    check_arguments(&words[1..])?;
    if !exec && shell_runs_inert(&program.value)? {
        return Ok(Vec::new());
    }
    program_targets(words, scan, depth)
}

fn check_arguments(words: &[&Word]) -> Result<(), Reason> {
    let dynamic = |word: &&&Word| {
        !word.substitutions.is_empty() || (word.expands && !known_path_expansion(word))
    };
    match words.iter().find(dynamic) {
        Some(_) => Err("dynamic shell expansion".into()),
        None => Ok(()),
    }
}

/// `words[0]` as a program, then what it runs in turn.
fn program_targets(
    words: &[&Word],
    scan: &HookScan<'_>,
    depth: usize,
) -> Result<Vec<PathBuf>, Reason> {
    if depth > MAX_NESTING {
        return Err("commands are nested too deeply".into());
    }
    let program = *words.first().ok_or("no executable")?;
    if program.value.starts_with('-') {
        return Err(format!("option `{}` where a program belongs", program.value).into());
    }
    let name = program.value.rsplit('/').next().unwrap_or(&program.value);
    check_followable(name)?;
    let mut targets = vec![locate_word(program, scan, true)?];
    let args = &words[1..];
    let inner = if name == "env" {
        Some(env_command(args, scan)?)
    } else {
        wrapped_command(name, args)?
    };
    let mut own_args = args;
    let family = interpreter_family(name);
    if let Some(inner) = inner {
        own_args = &args[..args.len() - inner.len()];
        targets.extend(program_targets(inner, scan, depth + 1)?);
    } else if let Some(family) = family {
        targets.extend(interpreter_targets(family, args, scan, depth)?);
    }
    check_argument_paths(own_args, family, &targets, scan)?;
    Ok(targets)
}

/// The command `env` runs after its environment changes.
fn env_command<'a, 'w>(
    args: &'a [&'w Word],
    scan: &HookScan<'_>,
) -> Result<&'a [&'w Word], Reason> {
    let mut index = 0;
    while let Some(word) = args.get(index) {
        if word.assignment_name().is_some() {
            check_assignment(word, scan)?;
        } else if word.value != "-i" && word.value != "--ignore-environment" {
            return Ok(&args[index..]);
        }
        index += 1;
    }
    Err("env has no executable".into())
}

/// The assignments and `exec` ahead of a command's program: the program's index,
/// and whether `exec` runs it.
fn program_index(words: &[&Word], scan: &HookScan<'_>) -> Result<(usize, bool), Reason> {
    let mut index = 0;
    while words
        .get(index)
        .is_some_and(|w| w.assignment_name().is_some())
    {
        check_assignment(words[index], scan)?;
        index += 1;
    }
    if words
        .get(index)
        .is_some_and(|w| w.value == "exec" && !w.quoted)
    {
        return Ok((index + 1, true));
    }
    Ok((index, false))
}

/// What the interpreter `family` runs given `words`: a script file, or for a
/// shell its inline code. Reading the program from stdin or from inline code in
/// another language leaves nothing to check, which is a refusal.
fn interpreter_targets(
    family: &str,
    words: &[&Word],
    scan: &HookScan<'_>,
    depth: usize,
) -> Result<Vec<PathBuf>, Reason> {
    let mut index = 0;
    while let Some(word) = words.get(index) {
        if runs_inline_code(family, &word.value) {
            if !is_shell(family) {
                return Err(format!("inline {family} code cannot be checked").into());
            }
            let code = words.get(index + 1).ok_or("shell names no inline code")?;
            return command_targets(&code.value, scan, depth + 1);
        }
        if !word.value.starts_with('-') {
            let script = runtime_script(family, &words[index..])?;
            return Ok(vec![locate_word(script, scan, false)?]);
        }
        index += interpreter_option_width(family, &word.value).ok_or_else(|| {
            format!(
                "`{family}` option `{}` is not one this check can follow",
                word.value
            )
        })?;
    }
    Err(format!("{family} names no script and would read its program from stdin").into())
}

fn check_assignment(word: &Word, scan: &HookScan<'_>) -> Result<(), Reason> {
    if word.expands
        || !word.substitutions.is_empty()
        || matches!(
            word.assignment_name(),
            Some("PATH" | "HOME" | "CLAUDE_PROJECT_DIR")
        )
    {
        return Err("dynamic assignment or path environment override".into());
    }
    check_assignment_path(word, scan)
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
