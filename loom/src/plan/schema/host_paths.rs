//! Host-path traps in plan-authored `allow_write` grants and stage commands.
//!
//! A stage's session sandbox binds an `allow_write` entry only if the path
//! already exists on the host when the session starts, and every command a
//! stage runs (`setup`, `acceptance`, `wiring_tests`, `before_stage`,
//! `after_stage`) executes inside that same sandbox, where everything
//! outside the worktree - including `/tmp` - is read-only. A plan that
//! grants or names a path under `/tmp`, or whose `setup` tries to `mkdir`
//! one into existence, blocks after spending tokens: the grant is silently
//! dropped and the `mkdir` fails with `Read-only file system`, taking every
//! criterion down with it via the `setup` `&&` prefix. The sandbox already
//! exports a writable `$TMPDIR` outside the repository, so neither the grant
//! nor the override is ever needed; these checks catch the shape before the
//! plan is presented.

use super::types::StageDefinition;

mod commands;

#[cfg(test)]
mod tests;

/// Absolute roots that do not survive a reboot and are absent on other
/// machines - never a valid `allow_write` grant or hardcoded command path.
const EPHEMERAL_ROOTS: [&str; 3] = ["/tmp", "/var/tmp", "/private/tmp"];

/// Host-path problems in `stage`: unusable `allow_write` grants and command
/// strings that reach for a path the stage sandbox cannot grant. One message
/// per problem, in a stable order - grants first, then command fields in
/// declaration order (acceptance, setup, wiring_tests, before_stage,
/// after_stage).
fn host_path_errors(
    stage: &StageDefinition,
    merged_allow_write: &[String],
    home: Option<&std::path::Path>,
) -> Vec<String> {
    let mut errors = grant_errors(&stage.id, merged_allow_write, home);

    commands::extend_field(
        &mut errors,
        &stage.id,
        "acceptance",
        stage.acceptance.iter().map(|c| c.command()),
    );
    commands::extend_field(
        &mut errors,
        &stage.id,
        "setup",
        stage.setup.iter().map(String::as_str),
    );
    commands::extend_field(
        &mut errors,
        &stage.id,
        "wiring_tests",
        stage.wiring_tests.iter().map(|w| w.command.as_str()),
    );
    commands::extend_field(
        &mut errors,
        &stage.id,
        "before_stage",
        stage.before_stage.iter().map(|c| c.command.as_str()),
    );
    commands::extend_field(
        &mut errors,
        &stage.id,
        "after_stage",
        stage.after_stage.iter().map(|c| c.command.as_str()),
    );

    errors
}

/// Entry point for `loom plan verify`: resolves `~/`-relative grants against
/// the real host home directory.
pub fn stage_host_path_errors(
    stage: &StageDefinition,
    merged_allow_write: &[String],
) -> Vec<String> {
    host_path_errors(stage, merged_allow_write, dirs::home_dir().as_deref())
}

// ── Grant checks (G1, G2) ────────────────────────────────────────────────

/// Checks `allow_write` entries: G1 rejects a grant equal to, or under, an
/// ephemeral root; G2 rejects a grant missing on this host, reusing
/// [`crate::sandbox::missing_grant_paths`] and skipping any entry G1 already
/// reported.
fn grant_errors(
    stage_id: &str,
    allow_write: &[String],
    home: Option<&std::path::Path>,
) -> Vec<String> {
    let mut errors = Vec::new();
    let mut ephemeral: Vec<&str> = Vec::new();

    for raw in allow_write {
        let entry = raw.trim();
        if entry.is_empty() || ephemeral.contains(&entry) {
            continue;
        }
        if under_ephemeral_root(&normalize_leading_double_slash(entry)) {
            ephemeral.push(entry);
            errors.push(format!(
                "Stage '{stage_id}': allow_write grant '{entry}' does not survive a reboot and \
                 is absent on other machines; delete the grant and use the sandbox's own \
                 $TMPDIR, which is already writable."
            ));
        }
    }

    for entry in crate::sandbox::missing_grant_paths(allow_write, home) {
        if ephemeral.contains(&entry.as_str()) {
            continue;
        }
        errors.push(format!(
            "Stage '{stage_id}': allow_write grant '{entry}' does not exist on this host; the \
             sandbox binds only a path that already exists when the session starts, and a \
             setup 'mkdir' runs inside the same sandbox so it cannot create it; create \
             '{entry}' on the host and record the prerequisite in the plan, or use $TMPDIR \
             instead."
        ));
    }

    errors
}

fn under_ephemeral_root(path: &str) -> bool {
    EPHEMERAL_ROOTS
        .iter()
        .any(|root| path == *root || path.starts_with(&format!("{root}/")))
}

fn normalize_leading_double_slash(path: &str) -> std::borrow::Cow<'_, str> {
    match path.strip_prefix("//") {
        Some(rest) => std::borrow::Cow::Owned(format!("/{rest}")),
        None => std::borrow::Cow::Borrowed(path),
    }
}
