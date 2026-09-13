//! Conservative executable resolution for prepared acceptance commands.

use std::ffi::OsStr;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::confine::{CommandSpec, PreparedCommand};

pub(super) fn resolve(prepared: &PreparedCommand) -> Option<Vec<PathBuf>> {
    let command = prepared.command();
    let working_dir = prepared.working_dir()?;
    let mut paths = vec![resolve_named(command.get_program(), command, working_dir)?];
    if let CommandSpec::Shell(line) = prepared.spec() {
        for name in shell_command_names(line)? {
            paths.push(resolve_named(OsStr::new(&name), command, working_dir)?);
        }
    }
    paths.sort();
    paths.dedup();
    Some(paths)
}

fn resolve_named(name: &OsStr, command: &Command, working_dir: &Path) -> Option<PathBuf> {
    let program = Path::new(name);
    if program.is_absolute() {
        return Some(program.to_path_buf());
    }
    if program.components().count() > 1 {
        return Some(working_dir.join(program));
    }
    let path = command.get_envs().find_map(|(key, value)| {
        if key == OsStr::new("PATH") {
            value
        } else {
            None
        }
    })?;
    std::env::split_paths(path)
        .map(|directory| {
            if directory.is_absolute() {
                directory.join(program)
            } else {
                working_dir.join(directory).join(program)
            }
        })
        .find(|candidate| candidate.is_file() && executable_path(candidate))
}

fn shell_command_names(line: &str) -> Option<Vec<String>> {
    if has_dynamic_dispatch(line) {
        return None;
    }
    let mut names = Vec::new();
    let mut expects_command = true;
    for raw in line.split_whitespace() {
        if is_operator(raw) {
            expects_command = true;
            continue;
        }
        let token = normalized_token(raw);
        if expects_command && !token.is_empty() {
            if shell_assignment(token) || token == "!" {
                continue;
            }
            if uncertain_command(token) {
                return None;
            }
            if !shell_builtin(token) {
                if unresolved_command_token(token) {
                    return None;
                }
                names.push(token.to_string());
            }
            expects_command = false;
        }
        if ends_command(raw) {
            expects_command = true;
        }
    }
    (!expects_command).then_some(names)
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
            | "env"
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
