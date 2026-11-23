use super::super::shell_lex::{lex, simple_commands, Word};
use super::{command_start, nested_scripts, MAX_NESTING};

/// Check whether an acceptance criterion invokes a resource that a worktree
/// session's sandboxed acceptance run cannot grant.
///
/// `loom map` and `loom knowledge context` both open `ContextStore`, which
/// resolves its cache under the main project root — read-only from a
/// worktree, and unreachable via `allow_write` since both settings emitters
/// filter out `../` paths. `tmux`/`docker` are host daemons the sandbox does
/// not expose. Returns the matched invocation for use in the warning
/// message, or `None` if the criterion is clean.
pub(in crate::plan::schema::validation) fn criterion_needs_ungrantable_resource(
    cmd: &str,
) -> Option<&'static str> {
    first_resource(cmd, 0)
}

fn first_resource(command: &str, depth: usize) -> Option<&'static str> {
    let tokens = lex(command);
    for simple in simple_commands(&tokens) {
        let argv = &simple.words[command_start(&simple.words)..];
        if let Some(resource) = command_resource(argv) {
            return Some(resource);
        }
        if depth < MAX_NESTING {
            for script in nested_scripts(&simple) {
                if let Some(resource) = first_resource(&script, depth + 1) {
                    return Some(resource);
                }
            }
        }
    }
    None
}

fn command_resource(argv: &[&Word]) -> Option<&'static str> {
    match argv.first()?.command_name() {
        "loom" if argv.get(1).is_some_and(|word| word.value == "map") => Some("loom map"),
        "loom"
            if argv.get(1).is_some_and(|word| word.value == "knowledge")
                && argv.get(2).is_some_and(|word| word.value == "context") =>
        {
            Some("loom knowledge context")
        }
        "tmux" => Some("tmux"),
        "docker" => Some("docker"),
        _ => None,
    }
}
