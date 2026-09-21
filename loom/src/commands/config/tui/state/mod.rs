//! Terminal-independent state and persistence rules for the config editor.
//!
//! Every registry key carries both tiers at once, and edits are staged per
//! tier: the operator can set a value in the project file and a different one
//! in their own, switch tabs between the two, and write both with a single
//! `s`. Nothing here touches a terminal, so the whole of that behavior has
//! ordinary unit tests.
//!
//! The impl is split across three files by what it does to the tiers —
//! `edit` stages, `save` writes, and this file loads and navigates. They share
//! [`ConfigState`]'s private fields because they are the same type's methods,
//! kept apart only so no one file outgrows its limit.

/// Staging edits against the active scope.
mod edit;
/// One registry key across both tiers, with its staged edits.
mod row;
/// Writing staged edits to both config files.
mod save;
/// The two config files, and how each is written and named.
mod scope;

pub(crate) use row::{ConfigRow, Pending, ProjectTier, Source};
pub(crate) use scope::{Scope, SCOPES};

/// Named only by the tests that pin the in-force rule against the daemon's;
/// the renderer asks [`ConfigRow::in_force`] for it and never spells the type.
#[cfg(test)]
pub(crate) use row::InForce;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::user_config::keys::{KeySpec, ValueKind, KEYS};
use crate::user_config::workspace::Workspace;
use crate::user_config::{ConfigValue, UserConfig};

/// What the screen says when the operator points the project tab at a tree
/// that has no `.loom/work`. Informational rather than an error: seeing the
/// tier exist is how they learn the fix.
const NO_WORKSPACE: &str = "no .loom/work in this tree — run loom init to create a project config";

/// Whether `kind` is edited as text; Bool and Enum values are stepped instead.
pub(crate) fn opens_editor(kind: &ValueKind) -> bool {
    matches!(kind, ValueKind::Number | ValueKind::String)
}

/// All selection, inline-edit, validation, and staged-save behavior for the screen.
pub(crate) struct ConfigState {
    /// Registry rows in `KEYS` order, which is intentionally the display order.
    rows: Vec<ConfigRow>,
    /// The focused row; clamped navigation keeps this index valid.
    selected: usize,
    /// The tier the keyboard currently edits.
    scope: Scope,
    /// The project tier's file, absent when the tree has no `.loom/work`.
    workspace: Option<Workspace>,
    /// The tree the project tier was opened from, so a save can re-open it.
    base: PathBuf,
    /// The user config's path as the scope tab prints it.
    user_path: String,
    /// The project config's path as the scope tab prints it.
    project_path: Option<String>,
    /// The in-progress edit, kept separate so Escape never mutates a row.
    edit_buffer: Option<String>,
    /// The most recent action result shown below the list.
    status: String,
    /// Whether the current status should receive error styling.
    status_is_error: bool,
    /// Whether the current status reports a completed write, which the status
    /// block marks with its own glyph.
    wrote: bool,
    /// Whether a quit has already been refused for the edits staged right now.
    ///
    /// With two tabs, an edit can be staged on the one the operator is not
    /// looking at, so a `q` that discarded silently would lose work they have
    /// no way of seeing.
    quit_armed: bool,
}

impl ConfigState {
    /// Load both tiers for the tree `loom config` was invoked in.
    pub(crate) fn load() -> Result<Self> {
        Self::load_from(&std::env::current_dir()?)
    }

    /// The seam a test drives: `base` is the tree whose `.loom/work` supplies
    /// the project tier. A tree without one is an ordinary case — `loom
    /// config` runs outside a repository — so the project tab reports the
    /// absence rather than refusing to open.
    pub(crate) fn load_from(base: &Path) -> Result<Self> {
        let config = UserConfig::load_strict()?;
        let workspace = Workspace::open(base)?;
        let rows = KEYS
            .iter()
            .map(|spec| ConfigRow::new(spec, &config, workspace.as_ref()))
            .collect::<Result<Vec<_>>>()?;
        let project_path = scope::project_path_label(workspace.as_ref().map(Workspace::root), base);
        Ok(Self {
            rows,
            selected: 0,
            scope: Scope::User,
            workspace,
            base: base.to_path_buf(),
            user_path: scope::user_path_label(),
            project_path,
            edit_buffer: None,
            status: "Ready. Edit a value, then press s to save.".to_owned(),
            status_is_error: false,
            wrote: false,
            quit_armed: false,
        })
    }

    /// Every row so rendering can preserve the registry's declared order.
    pub(crate) fn rows(&self) -> &[ConfigRow] {
        &self.rows
    }

    /// The focused row index for the renderer's selection marker.
    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    /// The focused row; `KEYS` is intentionally non-empty.
    pub(crate) fn selected_row(&self) -> &ConfigRow {
        &self.rows[self.selected]
    }

    /// The tier the keyboard edits and the table highlights.
    pub(crate) fn scope(&self) -> Scope {
        self.scope
    }

    /// The user config's path, `$HOME` collapsed to `~`.
    pub(crate) fn user_path(&self) -> &str {
        &self.user_path
    }

    /// The project config's path, or `None` when the tree has no workspace.
    pub(crate) fn project_path(&self) -> Option<&str> {
        self.project_path.as_deref()
    }

    /// The file `s` writes for the active scope, so pressing it is never a
    /// guess. `None` when the active scope has no file in this tree.
    pub(crate) fn active_path(&self) -> Option<&str> {
        match self.scope {
            Scope::User => Some(&self.user_path),
            Scope::Project => self.project_path(),
        }
    }

    /// Whether keystrokes currently belong to the inline editor.
    pub(crate) fn is_editing(&self) -> bool {
        self.edit_buffer.is_some()
    }

    /// The editing buffer when an inline edit is active.
    pub(crate) fn edit_buffer(&self) -> Option<&str> {
        self.edit_buffer.as_deref()
    }

    /// The current status text for the status block.
    pub(crate) fn status(&self) -> &str {
        &self.status
    }

    /// Whether the status line describes an error.
    pub(crate) fn status_is_error(&self) -> bool {
        self.status_is_error
    }

    /// Whether the status line reports a completed write.
    pub(crate) fn wrote(&self) -> bool {
        self.wrote
    }

    /// The active scope's text for the focused row. A test seam: the renderer
    /// draws every row, so it reaches for [`ConfigRow::displayed_text`] with
    /// an explicit scope rather than through the selection.
    #[cfg(test)]
    pub(crate) fn displayed_value(&self) -> String {
        self.selected_row()
            .displayed_text(self.scope)
            .unwrap_or_else(|| "—".to_owned())
    }

    /// Whether the focused row has an edit staged at the active scope. A test
    /// seam, for the same reason as [`Self::displayed_value`].
    #[cfg(test)]
    pub(crate) fn is_modified(&self) -> bool {
        self.selected_row().is_modified(self.scope)
    }

    /// Move up one row, clamping at the first registry key rather than wrapping.
    pub(crate) fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move down one row, clamping at the last registry key rather than wrapping.
    pub(crate) fn move_down(&mut self) {
        self.selected = self.selected.saturating_add(1).min(self.rows.len() - 1);
    }

    /// Jump to the first registry key.
    pub(crate) fn move_to_first(&mut self) {
        self.selected = 0;
    }

    /// Jump to the last registry key.
    pub(crate) fn move_to_last(&mut self) {
        self.selected = self.rows.len() - 1;
    }

    /// Point the editor at the other tier.
    pub(crate) fn toggle_scope(&mut self) {
        self.set_scope(self.scope.other());
    }

    /// Point the editor at `scope`, naming the file it now writes.
    ///
    /// Switching to the project tier of a tree that has no `.loom/work` is
    /// allowed on purpose: an operator who cannot see the tier cannot learn it
    /// exists, so the tab opens and says what is missing.
    pub(crate) fn set_scope(&mut self, scope: Scope) {
        self.scope = scope;
        let status = match self.active_path() {
            Some(path) => format!("{} scope — writes go to {path}.", scope.word()),
            None => NO_WORKSPACE.to_owned(),
        };
        self.set_status(false, status);
    }

    /// Whether `q` may quit now.
    ///
    /// The first press with anything staged refuses and warns instead,
    /// counting the edits on BOTH tabs — the ones on the tab the operator
    /// cannot see are exactly the ones a silent discard would cost them. The
    /// next press quits, unless something in between disarmed the warning.
    pub(crate) fn request_quit(&mut self) -> bool {
        if self.quit_armed {
            return true;
        }
        let (users, projects) = self.staged_counts();
        let total = users + projects;
        if total == 0 {
            return true;
        }
        self.set_status(
            true,
            format!(
                "{total} edit{} staged{} — press q again to discard, or s to save.",
                if total == 1 { "" } else { "s" },
                tier_breakdown(users, projects)
            ),
        );
        self.quit_armed = true;
        false
    }

    /// Forget a refused quit. Any key but a second `q` means the operator is
    /// still working, so the next one has to warn again rather than discard.
    pub(crate) fn disarm_quit(&mut self) {
        self.quit_armed = false;
    }

    /// Whether `Esc` may quit now.
    ///
    /// `Esc` means cancel everywhere else in this editor, so it must not also
    /// count as the second press that confirms a discard — only `q` does
    /// that. An armed warning is dismissed instead, leaving the edits staged;
    /// with nothing armed, `Esc` falls through to the same rule `q` uses.
    pub(crate) fn dismiss_or_request_quit(&mut self) -> bool {
        if self.quit_armed {
            self.set_status(
                false,
                "Quit cancelled; the staged edits are still there.".to_owned(),
            );
            return false;
        }
        self.request_quit()
    }

    /// Every staged edit across both scopes, as the write each one is:
    /// `None` is a clear, `Some` is a set.
    fn staged(&self) -> Vec<(usize, Scope, &'static KeySpec, Option<ConfigValue>)> {
        let mut staged = Vec::new();
        for (index, row) in self.rows.iter().enumerate() {
            for scope in SCOPES {
                let value = match row.pending(scope) {
                    Some(Pending::Set { value, .. }) => Some(value.clone()),
                    Some(Pending::Clear) => None,
                    None => continue,
                };
                staged.push((index, scope, row.spec(), value));
            }
        }
        staged
    }

    /// How many edits are staged at each scope, user first.
    fn staged_counts(&self) -> (usize, usize) {
        let staged = self.staged();
        let users = staged
            .iter()
            .filter(|(_, scope, _, _)| *scope == Scope::User)
            .count();
        (users, staged.len() - users)
    }

    /// Replace the status line and remember the style it should receive.
    fn set_status(&mut self, is_error: bool, status: String) {
        self.status = status;
        self.status_is_error = is_error;
        self.wrote = false;
        self.quit_armed = false;
    }

    /// Report a completed write, which the status block marks with `✓`.
    fn report_written(&mut self, status: String) {
        self.status = status;
        self.status_is_error = false;
        self.wrote = true;
        self.quit_armed = false;
    }
}

/// The per-tier split of a count, naming only the tiers that contribute — a
/// user-only save reading "· 0 project" is noise.
fn tier_breakdown(users: usize, projects: usize) -> String {
    let mut parts = Vec::new();
    if users > 0 {
        parts.push(format!("{users} user"));
    }
    if projects > 0 {
        parts.push(format!("{projects} project"));
    }
    if parts.is_empty() {
        return String::new();
    }
    format!(" · {}", parts.join(" · "))
}
