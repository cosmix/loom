use super::super::types::SandboxSummary;
use super::helpers::append_package_cache_note;

/// Format sandbox restrictions for agent awareness
pub(super) fn format_sandbox_section(summary: &SandboxSummary) -> String {
    let mut content = String::new();

    if !summary.enabled {
        content.push_str("## Sandbox Status\n\n");
        content.push_str("**Sandbox is DISABLED** for this stage.\n\n");
        return content;
    }

    content.push_str("## Sandbox Restrictions\n\n");
    content.push_str("The following restrictions are in effect for this session:\n\n");

    // Filesystem restrictions
    if !summary.deny_read.is_empty() || !summary.deny_write.is_empty() {
        content.push_str("### Filesystem\n\n");

        if !summary.deny_read.is_empty() {
            content.push_str("**Cannot Read:**\n");
            for path in &summary.deny_read {
                content.push_str(&format!("- `{}`\n", path));
            }
            content.push('\n');
        }

        if !summary.deny_write.is_empty() {
            content.push_str("**Cannot Write:**\n");
            for path in &summary.deny_write {
                content.push_str(&format!("- `{}`\n", path));
            }
            content.push('\n');
        }

        if !summary.allow_write.is_empty() {
            content.push_str("**Exceptions (CAN Write):**\n");
            for path in &summary.allow_write {
                content.push_str(&format!("- `{}`\n", path));
            }
            content.push('\n');
        }
    }

    content.push_str(&format_missing_grants_note(&summary.missing_allow_write));

    append_package_cache_note(&mut content);
    content.push_str(&format_network_section(summary));

    // Excluded commands
    if !summary.excluded_commands.is_empty() {
        content.push_str("### Excluded Commands\n\n");
        content.push_str("These commands bypass sandbox restrictions:\n");
        for cmd in &summary.excluded_commands {
            content.push_str(&format!("- `{}`\n", cmd));
        }
        content.push('\n');
    }

    content
}

/// Format the `### Network` block: allowed domains, or an explicit "no
/// network access" line when none are configured. Split out of
/// `format_sandbox_section` to keep that function under the file's line
/// ceiling.
fn format_network_section(summary: &SandboxSummary) -> String {
    let mut content = String::new();
    if !summary.allowed_domains.is_empty() {
        content.push_str("### Network\n\n");
        content.push_str("**Allowed Domains:**\n");
        for domain in &summary.allowed_domains {
            content.push_str(&format!("- `{}`\n", domain));
        }
        content.push('\n');
    } else {
        content.push_str("### Network\n\n");
        content.push_str("**No network access allowed.**\n\n");
    }
    content.push_str(&format_unix_socket_note(summary.unix_sockets_allowed));
    content
}

/// The `**Unix sockets:**` paragraph: what this session may do with Unix
/// sockets, and what to do when a criterion needs one it may not create.
fn format_unix_socket_note(allowed: bool) -> String {
    if allowed {
        return format!(
            "**Unix sockets:** this stage may create Unix sockets. Put each one inside the \
             worktree, at a path shorter than {} bytes (`sun_path`). Host sockets (`/run`, \
             `/var/run`, the ssh and gpg agents, tmux, the loom daemon) are off limits, and on \
             Linux hidden from this session, though a session that outlives a daemon restart \
             can reach the restarted daemon's socket: do not seek them.\n\n",
            crate::daemon::SUN_PATH_MAX
        );
    }
    // Every stage pays this text (`tests_size.rs` caps the floor): keep it short.
    "**Unix sockets:** none can be created (`Operation not permitted`, any path): postgres, \
     SeaweedFS filer/s3, Chrome, tmux, docker, sccache. Use TCP instead (postgres \
     `-c unix_socket_directories=''`). A criterion needing one goes last: do the rest, then \
     `loom stage block <stage-id> \"<criterion> needs a Unix socket\"`.\n\n"
        .to_string()
}

/// Format the "missing on the host" note for `allow_write` grants the
/// session sandbox will not bind, because the path did not exist at session
/// start (see `sandbox::missing_grant_paths`). Renders nothing when `missing`
/// is empty.
fn format_missing_grants_note(missing: &[String]) -> String {
    if missing.is_empty() {
        return String::new();
    }
    let mut content = String::new();
    content.push_str("**Missing on the host, so NOT writable this session:**\n");
    for path in missing {
        content.push_str(&format!("- `{}`\n", path));
    }
    content.push_str(
        "\nThese grants did not exist when this session was spawned, and the sandbox binds \
         only paths that exist at session start. Writes there fail with `Read-only file \
         system`, and a `mkdir` inside the session cannot create them. That is a sandbox \
         limit, not a bug in your change: block the stage with `loom stage block <stage-id> \
         \"<path> is missing on the host\"`: the operator creates the path and retries the \
         stage.\n\n",
    );
    content
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary_with(allow_write: Vec<String>, missing_allow_write: Vec<String>) -> SandboxSummary {
        SandboxSummary {
            enabled: true,
            deny_read: vec![],
            deny_write: vec!["../../**".to_string()],
            allow_write,
            missing_allow_write,
            allowed_domains: vec![],
            unix_sockets_allowed: false,
            excluded_commands: vec![],
        }
    }

    #[test]
    fn missing_grants_render_a_heading_and_each_path() {
        let summary = summary_with(
            vec!["/tmp/loom-pre-commit-plan".to_string()],
            vec!["/tmp/loom-pre-commit-plan".to_string()],
        );

        let content = format_sandbox_section(&summary);

        assert!(content.contains("Missing on the host, so NOT writable this session"));
        assert!(content.contains("`/tmp/loom-pre-commit-plan`"));
        assert!(content.contains("loom stage block"));
        assert!(!content.contains("report it as a blocker"));
    }

    #[test]
    fn no_missing_grants_omits_the_heading() {
        let summary = summary_with(vec!["/tmp/loom-pre-commit-plan".to_string()], vec![]);

        let content = format_sandbox_section(&summary);

        assert!(!content.contains("Missing on the host"));
    }

    #[test]
    fn missing_grants_render_even_with_no_stage_level_deny_rules() {
        // Regression: `summary_with` always sets `deny_write`, which hid the
        // bug where the missing-grants note was rendered only inside the
        // `deny_read`/`deny_write` block. A stage with no stage-level deny
        // rules (the plan-level-only grant case) must still see the warning.
        let summary = SandboxSummary {
            enabled: true,
            deny_read: vec![],
            deny_write: vec![],
            allow_write: vec![],
            missing_allow_write: vec!["/tmp/loom-pre-commit-plan".to_string()],
            allowed_domains: vec![],
            unix_sockets_allowed: false,
            excluded_commands: vec![],
        };

        let content = format_sandbox_section(&summary);

        assert!(content.contains("Missing on the host, so NOT writable this session"));
        assert!(content.contains("`/tmp/loom-pre-commit-plan`"));
    }

    #[test]
    fn the_unix_socket_note_follows_the_grant() {
        let mut summary = summary_with(vec![], vec![]);

        let denied = format_sandbox_section(&summary);
        assert!(denied.contains("**Unix sockets:** none can be created"));
        assert!(denied.contains("`loom stage block <stage-id>"), "{denied}");
        assert!(denied.contains("unix_socket_directories=''"), "{denied}");

        summary.unix_sockets_allowed = true;
        let allowed = format_sandbox_section(&summary);
        assert!(allowed.contains("**Unix sockets:** this stage may create Unix sockets"));
        assert!(allowed.contains("shorter than 104 bytes"), "{allowed}");
        assert!(allowed.contains("outlives a daemon restart"), "{allowed}");
        assert!(!allowed.contains("Operation not permitted"), "{allowed}");
    }
}
