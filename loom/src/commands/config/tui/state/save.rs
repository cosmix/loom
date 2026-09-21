//! Writing staged edits to both config files in one pass.
//!
//! A save is per edit, not per screen: each staged edit is written on its own
//! and cleared only if its own write succeeded, so one unwritable file cannot
//! silently discard the edits the operator made to the other.

use anyhow::Result;

use crate::user_config::workspace::Workspace;
use crate::user_config::UserConfig;

use super::{scope, tier_breakdown, ConfigState, Scope};

impl ConfigState {
    /// Write every staged edit across both scopes, then refresh both tiers
    /// from disk.
    pub(crate) fn save(&mut self) {
        let staged = self.staged();
        if staged.is_empty() {
            self.set_status(false, "0 keys written; nothing pending.".to_owned());
            return;
        }

        let mut saved = Vec::new();
        let mut errors = Vec::new();
        for (index, scope, spec, value) in staged {
            match scope::write(scope, self.workspace.as_ref(), spec, value) {
                Ok(()) => saved.push((index, scope)),
                Err(error) => errors.push(format!("{}: {error}", spec.name)),
            }
        }
        self.refresh_after_save(&saved, &errors);
    }

    /// Reload both files after writes so values, sources and the in-force tier
    /// reflect the persisted documents. An edit whose own write failed stays
    /// staged, and the status names its key. A `saved` key clears regardless
    /// of whether the reload that follows succeeds: the write already
    /// happened, and the pending slot only records unwritten intent — leaving
    /// it set would re-issue a write that already landed.
    fn refresh_after_save(&mut self, saved: &[(usize, Scope)], errors: &[String]) {
        for &(index, scope) in saved {
            self.rows[index].clear_pending(scope);
        }

        let reload_error = if saved.is_empty() {
            None
        } else {
            self.reload_tiers().err()
        };

        let users = saved
            .iter()
            .filter(|(_, scope)| *scope == Scope::User)
            .count();
        let written = format!(
            "{} written{}",
            key_count(saved.len()),
            tier_breakdown(users, saved.len() - users)
        );

        match (reload_error, errors.is_empty()) {
            (None, true) => self.report_written(format!("{written}.")),
            (None, false) => self.set_status(
                true,
                format!("{written}; could not save {}.", errors.join("; ")),
            ),
            (Some(error), true) => self.set_status(
                true,
                format!("{written}, but the screen may be stale: refresh failed: {error}"),
            ),
            (Some(error), false) => self.set_status(
                true,
                format!(
                    "{written}; could not save {}; the screen may be stale: refresh failed: {error}",
                    errors.join("; ")
                ),
            ),
        }
    }

    /// Re-read both tiers from disk into every row.
    fn reload_tiers(&mut self) -> Result<()> {
        let config = UserConfig::load_strict()?;
        let workspace = Workspace::open(&self.base)?;
        for row in &mut self.rows {
            row.reload(&config, workspace.as_ref())?;
        }
        self.project_path =
            scope::project_path_label(workspace.as_ref().map(Workspace::root), &self.base);
        self.workspace = workspace;
        Ok(())
    }
}

/// Format a grammatically useful write count for both success and error messages.
fn key_count(count: usize) -> String {
    format!("{count} key{}", if count == 1 { "" } else { "s" })
}
