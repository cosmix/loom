//! Conservative executable resolution for prepared acceptance commands.

use std::ffi::OsStr;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::confine::{CommandSpec, PreparedCommand};

/// Where `env` finds its command once the environment it passes on has no
/// `PATH` (`-i`, `-u PATH`): the C library's default search path, which
/// `execvp` falls back to.
#[cfg(target_os = "macos")]
const DEFAULT_EXEC_PATH: &str = "/usr/bin:/bin";
#[cfg(not(target_os = "macos"))]
const DEFAULT_EXEC_PATH: &str = "/bin:/usr/bin";

pub(super) fn resolve(prepared: &PreparedCommand) -> Option<Vec<PathBuf>> {
    let command = prepared.command();
    let working_dir = prepared.working_dir()?;
    let search = command_search_path(command);
    let mut paths = vec![resolve_named(command.get_program(), search, working_dir)?];
    if let CommandSpec::Shell(line) = prepared.spec() {
        for name in shell_command_names(line)? {
            let search = match name.search {
                Search::Command => search,
                Search::Default => Some(OsStr::new(DEFAULT_EXEC_PATH)),
            };
            paths.push(resolve_named(OsStr::new(&name.word), search, working_dir)?);
        }
    }
    paths.sort();
    paths.dedup();
    Some(paths)
}

fn command_search_path(command: &Command) -> Option<&OsStr> {
    command.get_envs().find_map(|(key, value)| {
        if key == OsStr::new("PATH") {
            value
        } else {
            None
        }
    })
}

fn resolve_named(name: &OsStr, search: Option<&OsStr>, working_dir: &Path) -> Option<PathBuf> {
    let program = Path::new(name);
    if program.is_absolute() {
        return Some(program.to_path_buf());
    }
    if program.components().count() > 1 {
        return Some(working_dir.join(program));
    }
    std::env::split_paths(search?)
        .map(|directory| {
            if directory.is_absolute() {
                directory.join(program)
            } else {
                working_dir.join(directory).join(program)
            }
        })
        .find(|candidate| candidate.is_file() && executable_path(candidate))
}

/// A program a shell line runs, and the search path that finds it.
#[derive(Debug, PartialEq, Eq)]
struct CommandName {
    word: String,
    search: Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Search {
    /// The prepared command's own `PATH`.
    Command,
    /// [`DEFAULT_EXEC_PATH`], for a command `env` runs without `PATH`.
    Default,
}

/// What the next word of a simple command is.
#[derive(Debug, Clone, Copy)]
enum Expect {
    Command,
    Env(EnvArgs),
    Arguments,
}

fn shell_command_names(line: &str) -> Option<Vec<CommandName>> {
    if has_dynamic_dispatch(line) {
        return None;
    }
    let mut names = Vec::new();
    let mut expect = Expect::Command;
    for raw in line.split_whitespace() {
        let token = normalized_token(raw);
        if !is_operator(raw) && !token.is_empty() {
            expect = match expect {
                Expect::Command => command_word(token, &mut names)?,
                Expect::Env(args) => args.next(token, &mut names)?,
                Expect::Arguments => Expect::Arguments,
            };
        }
        if is_operator(raw) || ends_command(raw) {
            // `env` with no command prints the environment: never cached.
            if matches!(expect, Expect::Env(_)) {
                return None;
            }
            expect = Expect::Command;
        }
    }
    matches!(expect, Expect::Arguments).then_some(names)
}

/// The first word of a simple command, past assignments and `!`.
fn command_word(token: &str, names: &mut Vec<CommandName>) -> Option<Expect> {
    if shell_assignment(token) || token == "!" {
        return Some(Expect::Command);
    }
    if token == "env" {
        names.push(CommandName::searched(token, Search::Command));
        return Some(Expect::Env(EnvArgs::default()));
    }
    if uncertain_command(token) {
        return None;
    }
    if !shell_builtin(token) {
        if unresolved_command_token(token) {
            return None;
        }
        names.push(CommandName::searched(token, Search::Command));
    }
    Some(Expect::Arguments)
}

impl CommandName {
    fn searched(word: &str, search: Search) -> Self {
        Self {
            word: word.to_string(),
            search,
        }
    }
}

/// `env`'s arguments read so far. GNU and BSD `env` take options up to the
/// first operand, then `NAME=VALUE` assignments, then the command. Only the
/// options that adjust the environment resolve; the contract digest holds
/// the command text, and so what they change. Anything else, such as `-S`,
/// `-C` or `--chdir`, makes the command uncertain.
#[derive(Debug, Clone, Copy, Default)]
struct EnvArgs {
    operands: bool,
    unset_next: bool,
    /// The command env runs sees no `PATH`.
    default_path: bool,
}

impl EnvArgs {
    fn next(mut self, token: &str, names: &mut Vec<CommandName>) -> Option<Expect> {
        if self.unset_next {
            self.unset_next = false;
            self.default_path |= token == "PATH";
            return Some(Expect::Env(self));
        }
        if !self.operands {
            match token {
                "-i" | "--ignore-environment" => self.default_path = true,
                "-u" => self.unset_next = true,
                _ if token.starts_with("--unset=") => {
                    self.default_path |= token == "--unset=PATH";
                }
                _ if token.starts_with('-') => return None,
                _ => self.operands = true,
            }
            if !self.operands {
                return Some(Expect::Env(self));
            }
        }
        if shell_assignment(token) {
            // A `PATH` env sets would also pick the command it runs.
            return (!token.starts_with("PATH=")).then_some(Expect::Env(self));
        }
        // A word past the options that starts with `-` is the program env
        // runs (`env A=1 -i cargo` runs `-i`).
        if token.starts_with('-')
            || token == "env"
            || uncertain_command(token)
            || unresolved_command_token(token)
        {
            return None;
        }
        let search = if self.default_path {
            Search::Default
        } else {
            Search::Command
        };
        names.push(CommandName::searched(token, search));
        Some(Expect::Arguments)
    }
}

fn has_dynamic_dispatch(line: &str) -> bool {
    line.contains("$(") || line.contains('`') || line.contains("<(") || line.contains(">(")
}

fn normalized_token(token: &str) -> &str {
    token
        .trim_matches(is_shell_syntax)
        .trim_matches(|value| value == '\'' || value == '"')
}

fn unresolved_command_token(token: &str) -> bool {
    token.contains('$')
        || token
            .chars()
            .any(|value| matches!(value, '*' | '?' | '[' | ']'))
}

fn is_operator(token: &str) -> bool {
    matches!(token, "&&" | "||" | "|" | ";")
}

fn ends_command(token: &str) -> bool {
    token.ends_with(';') || token.ends_with('|') || token.ends_with("&&") || token.ends_with("||")
}

fn is_shell_syntax(value: char) -> bool {
    matches!(value, '(' | ')' | ';' | '|' | '&')
}

fn shell_assignment(token: &str) -> bool {
    token.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    })
}

fn shell_builtin(token: &str) -> bool {
    matches!(
        token,
        ":" | "break"
            | "continue"
            | "echo"
            | "exit"
            | "export"
            | "false"
            | "printf"
            | "pwd"
            | "read"
            | "return"
            | "set"
            | "shift"
            | "test"
            | "true"
            | "unset"
            | "["
    )
}

fn uncertain_command(token: &str) -> bool {
    matches!(
        token,
        "." | "source"
            | "eval"
            | "exec"
            | "command"
            | "time"
            | "xargs"
            | "curl"
            | "wget"
            | "ssh"
            | "scp"
            | "sftp"
            | "nc"
            | "netcat"
            | "telnet"
            | "ftp"
            | "for"
            | "while"
            | "until"
            | "case"
            | "if"
            | "then"
            | "do"
    )
}

#[cfg(unix)]
pub(super) fn is_executable(metadata: &Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
pub(super) fn is_executable(_metadata: &Metadata) -> bool {
    true
}

#[cfg(unix)]
fn executable_path(path: &Path) -> bool {
    path.metadata().is_ok_and(|value| is_executable(&value))
}

#[cfg(not(unix))]
fn executable_path(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
#[path = "cache_executable_tests.rs"]
mod tests;
