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

    append_package_cache_note(&mut content);

    // Network restrictions
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
