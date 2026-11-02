//! What check 5 knows about a hook command's program: shell builtins, wrappers
//! that exec another program, programs that decide what they run themselves, and
//! interpreters whatever their version suffix.
//!
//! The tables cover the names hooks use. A program outside them that runs a file
//! of its own (`awk -f`, `java -jar`) counts as the program, and the rule over
//! every argument in `hook_commands::paths` judges the file it is given.

use std::borrow::Cow;

use crate::plan::schema::validation::shell_lex::Word;

/// Why a command cannot be verified, for the refusal that names it.
pub(super) type Reason = Cow<'static, str>;

/// Shell builtins that neither run a file nor change what a later word means.
const INERT_BUILTINS: [&str; 9] = [
    ":", "true", "false", "exit", "echo", "printf", "test", "[", "pwd",
];

/// Shell builtins with no file to check, whose effect on the shell (directory,
/// variables, traps) changes what later commands resolve to.
const STATEFUL_BUILTINS: [&str; 26] = [
    "cd", "pushd", "popd", "export", "unset", "set", "shopt", "alias", "unalias", "hash", "enable",
    "declare", "typeset", "readonly", "local", "let", "read", "trap", "wait", "umask", "ulimit",
    "getopts", "shift", "return", "break", "continue",
];

/// Shell syntax and commands that run a string or a program this check does not
/// follow.
const UNSUPPORTED_WORDS: [&str; 24] = [
    "eval", "source", ".", "command", "builtin", "sudo", "time", "if", "then", "elif", "else",
    "fi", "for", "while", "until", "do", "done", "case", "esac", "function", "select", "{", "}",
    "!",
];

/// Programs that choose what they run from their own arguments, a project file
/// the session can write, or a package registry.
const SELF_DIRECTED: [&str; 37] = [
    "doas",
    "su",
    "busybox",
    "toybox",
    "watch",
    "parallel",
    "find",
    "chroot",
    "unshare",
    "nsenter",
    "systemd-run",
    "script",
    "strace",
    "ltrace",
    "gdb",
    "ssh",
    "uv",
    "uvx",
    "npx",
    "bunx",
    "pipx",
    "poetry",
    "npm",
    "pnpm",
    "yarn",
    "make",
    "just",
    "go",
    "cargo",
    "gradle",
    "gradlew",
    "mvn",
    "mvnw",
    "rake",
    "tox",
    "nox",
    "dotnet",
];

/// Shell-family interpreters: `-c` code is shell, and is checked as commands.
const SHELLS: [&str; 8] = ["sh", "bash", "zsh", "dash", "ash", "ksh", "csh", "tcsh"];

/// Other interpreters whose first non-flag argument is the file they run, or for
/// `bun` and `deno` the subcommand `runtime_script` reads.
const INTERPRETERS: [&str; 10] = [
    "python", "pypy", "node", "bun", "deno", "ruby", "perl", "php", "lua", "tclsh",
];

/// A wrapper's command line: its options, then `operands` words, then the
/// command it execs.
struct Wrapper {
    name: &'static str,
    flags: &'static [&'static str],
    /// Options that take a value, in the next word or attached to the option.
    valued: &'static [&'static str],
    operands: usize,
}

const WRAPPERS: [Wrapper; 7] = [
    Wrapper {
        name: "timeout",
        flags: &["--foreground", "--preserve-status", "-v", "--verbose"],
        valued: &["-k", "--kill-after", "-s", "--signal"],
        operands: 1,
    },
    Wrapper {
        name: "nohup",
        flags: &[],
        valued: &[],
        operands: 0,
    },
    Wrapper {
        name: "nice",
        flags: &[],
        valued: &["-n", "--adjustment"],
        operands: 0,
    },
    Wrapper {
        name: "stdbuf",
        flags: &[],
        valued: &["-i", "--input", "-o", "--output", "-e", "--error"],
        operands: 0,
    },
    Wrapper {
        name: "setsid",
        flags: &["-c", "--ctty", "-f", "--fork", "-w", "--wait"],
        valued: &[],
        operands: 0,
    },
    // `-c` runs a shell string, so it stays out of the grammar.
    Wrapper {
        name: "flock",
        flags: &[
            "-s",
            "--shared",
            "-x",
            "-e",
            "--exclusive",
            "-u",
            "--unlock",
            "-n",
            "--nb",
            "--nonblock",
            "-o",
            "--close",
            "-F",
            "--no-fork",
            "--verbose",
        ],
        valued: &["-w", "--wait", "--timeout", "-E", "--conflict-exit-code"],
        operands: 1,
    },
    // `-I`, `-i` and `-a` let input or a file supply the program or its
    // arguments, so they stay out of the grammar.
    Wrapper {
        name: "xargs",
        flags: &["-0", "--null", "-r", "--no-run-if-empty", "-t", "--verbose"],
        valued: &[
            "-n",
            "--max-args",
            "-L",
            "--max-lines",
            "-P",
            "--max-procs",
            "-d",
            "--delimiter",
            "-s",
            "--max-chars",
        ],
        operands: 0,
    },
];

/// What the shell does with `name` before any PATH lookup, for a word the shell
/// itself runs: `Ok(true)` for a builtin with nothing to check, `Ok(false)` for
/// a program, an error for a builtin or keyword this check cannot follow.
pub(super) fn shell_runs_inert(name: &str) -> Result<bool, Reason> {
    if STATEFUL_BUILTINS.contains(&name) {
        return Err(
            format!("shell builtin `{name}` changes what later commands resolve to").into(),
        );
    }
    Ok(INERT_BUILTINS.contains(&name))
}

/// Refuse a program this check cannot follow.
pub(super) fn check_followable(name: &str) -> Result<(), Reason> {
    if UNSUPPORTED_WORDS.contains(&name) {
        return Err(format!("unsupported shell command or wrapper `{name}`").into());
    }
    if SELF_DIRECTED.contains(&name) {
        return Err(
            format!("`{name}` chooses what it runs in a way this check cannot follow").into(),
        );
    }
    Ok(())
}

/// The command `name` execs when it is a known wrapper; `Ok(None)` when it is
/// not one. An option outside its grammar or a missing command is an error.
pub(super) fn wrapped_command<'a, 'w>(
    name: &str,
    args: &'a [&'w Word],
) -> Result<Option<&'a [&'w Word]>, Reason> {
    let Some(wrapper) = WRAPPERS.iter().find(|wrapper| wrapper.name == name) else {
        return Ok(None);
    };
    let mut index = 0;
    while let Some(word) = args.get(index) {
        let value = word.value.as_str();
        if value == "--" {
            index += 1;
            break;
        }
        if !value.starts_with('-') || value == "-" {
            break;
        }
        index += option_width(wrapper, value)
            .ok_or_else(|| format!("`{name}` option `{value}` is not one this check can follow"))?;
    }
    index += wrapper.operands;
    match args.get(index..) {
        Some(command) if !command.is_empty() => Ok(Some(command)),
        _ => Err(format!("`{name}` names no command to run").into()),
    }
}

/// How many words `option` takes up in `wrapper`'s grammar.
fn option_width(wrapper: &Wrapper, option: &str) -> Option<usize> {
    if wrapper.flags.contains(&option) {
        return Some(1);
    }
    if wrapper.valued.contains(&option) {
        return Some(2);
    }
    let long = option.split_once('=').map(|(name, _)| name);
    let short = option.get(..2).filter(|_| !option.starts_with("--"));
    [long, short]
        .into_iter()
        .flatten()
        .any(|name| wrapper.valued.contains(&name))
        .then_some(1)
}

/// The interpreter family of `name`, whatever its version suffix
/// (`python3.12`, `node20`, `bash-5.2`).
pub(super) fn interpreter_family(name: &str) -> Option<&str> {
    let family =
        name.trim_end_matches(|c: char| c.is_ascii_digit() || matches!(c, '.' | '-' | '_'));
    (SHELLS.contains(&family) || INTERPRETERS.contains(&family)).then_some(family)
}

pub(super) fn is_shell(family: &str) -> bool {
    SHELLS.contains(&family)
}

/// Whether `flag` makes the interpreter `family` run the code that follows it.
pub(super) fn runs_inline_code(family: &str, flag: &str) -> bool {
    if is_shell(family) || family == "python" || family == "pypy" {
        flag == "-c"
    } else {
        flag == "-e" || flag == "--eval" || (family == "perl" && flag == "-E")
    }
}

/// How many words `option` takes up on the interpreter `family`'s command line;
/// `None` outside this check's grammar. `-I` names a directory for perl and ruby
/// and is python's isolated-mode flag.
pub(super) fn interpreter_option_width(family: &str, option: &str) -> Option<usize> {
    match family {
        "python" | "pypy" => matches!(option, "-x" | "-v" | "-u" | "-E" | "-I").then_some(1),
        "perl" | "ruby" if option == "-I" => Some(2),
        "perl" | "ruby" => option.starts_with("-I").then_some(1),
        _ if is_shell(family) => matches!(option, "-x" | "-v" | "-u" | "-E").then_some(1),
        _ => None,
    }
}

/// The word naming the file a command line starting at `words[0]`, its first
/// non-option word, runs. Only a runtime with subcommands looks past it:
/// `bun run <script>` and `deno run <script>` name a file, and any other
/// subcommand or a `package.json` script name chooses what it runs itself.
pub(super) fn runtime_script<'w>(family: &str, words: &[&'w Word]) -> Result<&'w Word, Reason> {
    let first = words[0];
    if family != "bun" && family != "deno" {
        return Ok(first);
    }
    let names_file = |word: &Word| word.value.contains(['/', '.']);
    if first.value != "run" {
        return if names_file(first) {
            Ok(first)
        } else {
            Err(format!(
                "`{family} {}` is a subcommand this check cannot follow",
                first.value
            )
            .into())
        };
    }
    let script = *words
        .get(1)
        .ok_or_else(|| format!("`{family} run` names no script"))?;
    if script.value.starts_with('-') {
        return Err(format!(
            "`{family} run` option `{}` is not one this check can follow",
            script.value
        )
        .into());
    }
    if !names_file(script) {
        return Err(format!(
            "`{family} run {}` runs a project script or package binary this check cannot follow",
            script.value
        )
        .into());
    }
    Ok(script)
}
