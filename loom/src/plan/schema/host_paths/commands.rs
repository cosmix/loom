//! Command-string checks (C1 TMPDIR override, C2 hardcoded temp path, C3
//! write outside the worktree) for host-path traps in stage commands.
//!
//! Issues that share a field, a rule, and an offending value collapse into
//! one message spanning every command index, so a `TMPDIR=/tmp/x` prefix
//! repeated across a stage's acceptance criteria prints once instead of once
//! per criterion.

use super::{normalize_leading_double_slash, under_ephemeral_root};

/// Shell tokens that end the current simple command on their own.
const COMMAND_SEPARATORS: [&str; 11] = [
    "&&", "||", ";", "|", "(", ")", "{", "}", "then", "do", "else",
];

/// One host-path problem found in a single command token. Two issues of the
/// same variant carrying the same value are the same problem for grouping.
#[derive(PartialEq)]
enum HostPathIssue {
    /// C1: a `TMPDIR=` override pointed outside the sandbox's own tmpdir.
    TmpdirOverride(String),
    /// C2: a hardcoded path under an ephemeral root.
    HardcodedTempPath(String),
    /// C3: a write (`mkdir`/`touch` operand, or output redirect target)
    /// aimed outside the worktree.
    WriteOutsideWorktree(String),
}

/// Walks every command in one stage field, grouping issues that share a rule
/// and offending value across indices, then formats one message per group in
/// first-occurrence order.
pub(super) fn extend_field<'a>(
    errors: &mut Vec<String>,
    stage_id: &str,
    label: &str,
    commands: impl Iterator<Item = &'a str>,
) {
    let mut groups: Vec<(HostPathIssue, Vec<usize>, &str)> = Vec::new();

    for (idx, command) in commands.enumerate() {
        for issue in command_issues(command) {
            match groups.iter_mut().find(|(seen, _, _)| *seen == issue) {
                Some((_, indices, _)) => indices.push(idx + 1),
                None => groups.push((issue, vec![idx + 1], command)),
            }
        }
    }

    for (issue, indices, command) in &groups {
        errors.push(format_issue(stage_id, label, indices, command, issue));
    }
}

fn format_issue(
    stage_id: &str,
    label: &str,
    indices: &[usize],
    command: &str,
    issue: &HostPathIssue,
) -> String {
    let locator = format_locator(label, indices, &truncate_command(command));
    match issue {
        HostPathIssue::TmpdirOverride(value) => format!(
            "Stage '{stage_id}': {locator} overrides TMPDIR to '{value}'; \
             remove the override - the sandbox already sets TMPDIR to a writable directory \
             outside the repository."
        ),
        HostPathIssue::HardcodedTempPath(path) => format!(
            "Stage '{stage_id}': {locator} hardcodes '{path}', which does not \
             survive a reboot and is absent on other machines; use \"$TMPDIR\" (or mktemp -d \
             \"${{TMPDIR:-/tmp}}/<name>.XXXXXX\")."
        ),
        HostPathIssue::WriteOutsideWorktree(path) => format!(
            "Stage '{stage_id}': {locator} writes to '{path}' outside the \
             worktree; commands run inside the stage sandbox, where everything outside the \
             worktree is read-only - write under the worktree or \"$TMPDIR\"."
        ),
    }
}

/// Renders the label and command index(es) leading an issue's message:
/// `label #N (`cmd`)` for one occurrence, `label #1, #2, #3 (first: `cmd`)`
/// once grouping has collapsed several into one.
fn format_locator(label: &str, indices: &[usize], echoed: &str) -> String {
    match indices {
        [only] => format!("{label} #{only} (`{echoed}`)"),
        many => {
            let list = many
                .iter()
                .map(|i| format!("#{i}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{label} {list} (first: `{echoed}`)")
        }
    }
}

fn truncate_command(command: &str) -> String {
    const MAX: usize = 80;
    if command.chars().count() <= MAX {
        return command.to_string();
    }
    let truncated: String = command.chars().take(MAX).collect();
    format!("{truncated}…")
}

/// Tokenises `command` and walks it tracking simple-command boundaries,
/// collecting every C1/C2/C3 issue found. Skips a command `shell_words`
/// cannot split - other validation reports the syntax error.
fn command_issues(command: &str) -> Vec<HostPathIssue> {
    let Ok(tokens) = shell_words::split(command) else {
        return Vec::new();
    };

    let mut issues = Vec::new();
    let mut state = CommandState::new();

    for raw in &tokens {
        if COMMAND_SEPARATORS.contains(&raw.as_str()) {
            state = CommandState::new();
            continue;
        }
        match raw.strip_suffix(';') {
            Some(stripped) if !stripped.is_empty() => {
                state.process(stripped, &mut issues);
                state = CommandState::new();
            }
            _ => state.process(raw, &mut issues),
        }
    }

    issues
}

/// Per-simple-command state for the token walk: the basename of the command
/// word seen so far (if any), whether we are still in the leading-assignment
/// prefix, and whether the previous token was a lone redirect operator
/// awaiting its target.
struct CommandState {
    command_word: Option<String>,
    in_leading_assignment: bool,
    pending_redirect: Option<bool>,
}

impl CommandState {
    fn new() -> Self {
        Self {
            command_word: None,
            in_leading_assignment: true,
            pending_redirect: None,
        }
    }

    fn process(&mut self, token: &str, issues: &mut Vec<HostPathIssue>) {
        if let Some(is_output) = self.pending_redirect.take() {
            self.evaluate_operand(token, true, is_output, issues);
            return;
        }

        if self.in_leading_assignment {
            if let Some((name, value)) = as_assignment(token) {
                if name == "TMPDIR" {
                    issues.extend(tmpdir_override_issue(value));
                }
                return;
            }
            self.in_leading_assignment = false;
            self.command_word = Some(basename(token).to_string());
        }

        if matches!(self.command_word.as_deref(), Some("export") | Some("env")) {
            if let Some((name, value)) = as_assignment(token) {
                if name == "TMPDIR" {
                    issues.extend(tmpdir_override_issue(value));
                    return;
                }
            }
        }

        self.evaluate_operand(token, false, false, issues);
    }

    fn evaluate_operand(
        &mut self,
        token: &str,
        is_redirect_target: bool,
        is_output_redirect: bool,
        issues: &mut Vec<HostPathIssue>,
    ) {
        if !is_redirect_target {
            if let Some((is_output, rest)) = strip_redirect(token) {
                if rest.is_empty() {
                    self.pending_redirect = Some(is_output);
                    return;
                }
                self.check_candidate(rest, true, is_output, issues);
                return;
            }
        }

        let candidate = flag_value(token).unwrap_or(token);
        self.check_candidate(candidate, is_redirect_target, is_output_redirect, issues);
    }

    fn check_candidate(
        &self,
        candidate: &str,
        is_redirect_target: bool,
        is_output_redirect: bool,
        issues: &mut Vec<HostPathIssue>,
    ) {
        let normalized = normalize_leading_double_slash(candidate);
        let word = self.command_word.as_deref().unwrap_or("");

        if !is_excluded_command(word) && under_ephemeral_root(&normalized) {
            issues.push(HostPathIssue::HardcodedTempPath(candidate.to_string()));
            return;
        }

        let writes_outside = if is_redirect_target {
            is_output_redirect && !normalized.starts_with("/dev/")
        } else {
            matches!(word, "mkdir" | "touch")
        };
        if writes_outside && starts_outside_worktree(&normalized) {
            issues.push(HostPathIssue::WriteOutsideWorktree(candidate.to_string()));
        }
    }
}

fn tmpdir_override_issue(value: &str) -> Option<HostPathIssue> {
    starts_outside_worktree(value).then(|| HostPathIssue::TmpdirOverride(value.to_string()))
}

fn starts_outside_worktree(candidate: &str) -> bool {
    candidate.starts_with('/') || candidate.starts_with('~') || candidate.starts_with("$HOME")
}

fn is_excluded_command(word: &str) -> bool {
    matches!(word, "rg" | "grep" | "egrep" | "fgrep" | "sed" | "awk")
}

fn basename(token: &str) -> &str {
    token.rsplit('/').next().unwrap_or(token)
}

/// Splits `token` into `(name, value)` when it has the shape of a shell
/// assignment (`^[A-Za-z_][A-Za-z0-9_]*=`), e.g. `TMPDIR=/tmp/x`.
fn as_assignment(token: &str) -> Option<(&str, &str)> {
    let eq = token.find('=')?;
    let name = &token[..eq];
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((name, &token[eq + 1..]))
}

/// Strips a leading redirect operator (`[0-9]*(>>|>|<)`) from `token`,
/// returning whether it was an output redirect (`>`/`>>`, vs. input `<`) and
/// the remainder. `None` when `token` has no such prefix.
fn strip_redirect(token: &str) -> Option<(bool, &str)> {
    let digits_end = token
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(token.len());
    let rest = &token[digits_end..];
    if let Some(r) = rest.strip_prefix(">>") {
        Some((true, r))
    } else if let Some(r) = rest.strip_prefix('>') {
        Some((true, r))
    } else {
        rest.strip_prefix('<').map(|r| (false, r))
    }
}

/// Extracts `value` from a `--flag=value` token.
fn flag_value(token: &str) -> Option<&str> {
    let rest = token.strip_prefix("--")?;
    rest.find('=').map(|eq| &rest[eq + 1..])
}
