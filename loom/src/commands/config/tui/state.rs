//! Terminal-independent state and persistence rules for the config editor.
//!
//! Pending values retain both their display text and their registry-parsed
//! TOML value. The former makes inline editing predictable while the latter
//! prevents the screen and `loom config -k` from drifting into separate
//! validators.

use anyhow::Result;

use crate::user_config::{
    keys::{KeySpec, KEYS},
    Origin, UserConfig,
};

/// One visible registry row, including an optional value not yet written to disk.
pub(super) struct ConfigRow {
    /// The registry specification that determines this row's name and type.
    spec: &'static KeySpec,
    /// The latest disk-resolved value, before any staged edit.
    value: String,
    /// Whether `value` came from the file or a built-in default.
    origin: Origin,
    /// A validated edit that remains reversible until the operator saves.
    pending: Option<PendingValue>,
}

/// Preserve the exact editing text alongside the typed value needed by `set`.
struct PendingValue {
    /// The text the operator entered and should continue to see in the row.
    raw: String,
    /// The typed TOML value already accepted by the registry validator.
    value: toml_edit::Value,
}

impl ConfigRow {
    /// Build a row from a strict resolved-config snapshot.
    fn from_config(spec: &'static KeySpec, config: &UserConfig) -> Self {
        let (value, origin) = config.value_of(spec);
        Self {
            spec,
            value,
            origin,
            pending: None,
        }
    }

    /// Return the typed registry entry this row represents.
    pub(super) fn spec(&self) -> &'static KeySpec {
        self.spec
    }

    /// Return pending text when present, otherwise the latest disk value.
    pub(super) fn displayed_value(&self) -> &str {
        self.pending
            .as_ref()
            .map_or(&self.value, |pending| &pending.raw)
    }

    /// Return whether the displayed disk value is explicit or a default.
    pub(super) fn origin(&self) -> Origin {
        self.origin
    }

    /// Return whether this row still needs an explicit save.
    pub(super) fn is_modified(&self) -> bool {
        self.pending.is_some()
    }
}

/// All selection, inline-edit, validation, and staged-save behavior for the screen.
pub(super) struct ConfigState {
    /// Registry rows in `KEYS` order, which is intentionally the display order.
    rows: Vec<ConfigRow>,
    /// The focused row; clamped navigation keeps this index valid.
    selected: usize,
    /// The in-progress edit, kept separate so Escape never mutates a row.
    edit_buffer: Option<String>,
    /// The most recent action result shown below the list.
    status: String,
    /// Whether the current status should receive error styling.
    status_is_error: bool,
}

impl ConfigState {
    /// Load every registry row from one strict config snapshot before opening the screen.
    pub(super) fn load() -> Result<Self> {
        let config = UserConfig::load_strict()?;
        let rows = KEYS
            .iter()
            .map(|spec| ConfigRow::from_config(spec, &config))
            .collect();
        Ok(Self {
            rows,
            selected: 0,
            edit_buffer: None,
            status: "Ready. Edit a value, then press s to save.".to_owned(),
            status_is_error: false,
        })
    }

    /// Return every row so rendering can preserve the registry's declared order.
    pub(super) fn rows(&self) -> &[ConfigRow] {
        &self.rows
    }

    /// Return the focused row index for the renderer's selection marker.
    pub(super) fn selected(&self) -> usize {
        self.selected
    }

    /// Return the focused row; `KEYS` is intentionally non-empty.
    pub(super) fn selected_row(&self) -> &ConfigRow {
        &self.rows[self.selected]
    }

    /// Return whether keystrokes currently belong to the inline editor.
    pub(super) fn is_editing(&self) -> bool {
        self.edit_buffer.is_some()
    }

    /// Return the editing buffer when an inline edit is active.
    pub(super) fn edit_buffer(&self) -> Option<&str> {
        self.edit_buffer.as_deref()
    }

    /// Return the current status text for the status line.
    pub(super) fn status(&self) -> &str {
        &self.status
    }

    /// Return whether the status line describes an error.
    pub(super) fn status_is_error(&self) -> bool {
        self.status_is_error
    }

    /// Move up one row, clamping at the first registry key rather than wrapping.
    pub(super) fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move down one row, clamping at the last registry key rather than wrapping.
    pub(super) fn move_down(&mut self) {
        self.selected = self.selected.saturating_add(1).min(self.rows.len() - 1);
    }

    /// Start a reversible edit seeded with the row's currently displayed value.
    pub(super) fn begin_edit(&mut self) {
        self.edit_buffer = Some(self.selected_row().displayed_value().to_owned());
        self.set_status(false, "Editing: Enter commits; Esc cancels.".to_owned());
    }

    /// Append a printable character while an inline edit owns the keyboard.
    pub(super) fn append_char(&mut self, character: char) {
        if let Some(buffer) = &mut self.edit_buffer {
            buffer.push(character);
        }
    }

    /// Remove one Unicode scalar from the inline editor without touching the row.
    pub(super) fn backspace(&mut self) {
        if let Some(buffer) = &mut self.edit_buffer {
            buffer.pop();
        }
    }

    /// Discard only the in-progress text; a row changes solely after a valid commit.
    pub(super) fn cancel_edit(&mut self) {
        if self.edit_buffer.take().is_some() {
            self.set_status(false, "Edit cancelled; value restored.".to_owned());
        }
    }

    /// Validate the buffer through its `KeySpec`, staging valid values in memory.
    pub(super) fn commit_edit(&mut self) {
        let Some(raw) = self.edit_buffer.clone() else {
            return;
        };
        let spec = self.selected_row().spec();
        match spec.parse(&raw) {
            Ok(value) => {
                self.rows[self.selected].pending = Some(PendingValue { raw, value });
                self.edit_buffer = None;
                self.set_status(false, format!("{} staged; press s to save.", spec.name));
            }
            Err(error) => self.set_status(true, error.to_string()),
        }
    }

    /// Write each pending row, then refresh successful values from a strict disk snapshot.
    pub(super) fn save(&mut self) {
        let pending: Vec<(usize, &'static KeySpec, toml_edit::Value)> = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                row.pending
                    .as_ref()
                    .map(|pending| (index, row.spec(), pending.value.clone()))
            })
            .collect();
        if pending.is_empty() {
            self.set_status(false, "0 keys written; nothing pending.".to_owned());
            return;
        }

        let mut saved = Vec::new();
        let mut errors = Vec::new();
        for (index, spec, value) in pending {
            match crate::user_config::set(spec, value) {
                Ok(_) => saved.push(index),
                Err(error) => errors.push(format!("{}: {error}", spec.name)),
            }
        }
        self.refresh_after_save(&saved, &errors);
    }

    /// Reload the file after writes so values and origins reflect the persisted document.
    fn refresh_after_save(&mut self, saved: &[usize], errors: &[String]) {
        if !saved.is_empty() {
            let config = match UserConfig::load_strict() {
                Ok(config) => config,
                Err(error) => {
                    self.set_status(
                        true,
                        format!(
                            "{} written, but refresh failed: {error}",
                            key_count(saved.len())
                        ),
                    );
                    return;
                }
            };
            for row in &mut self.rows {
                let (value, origin) = config.value_of(row.spec());
                row.value = value;
                row.origin = origin;
            }
            for &index in saved {
                self.rows[index].pending = None;
            }
        }

        if errors.is_empty() {
            self.set_status(false, format!("{} written.", key_count(saved.len())));
        } else {
            self.set_status(
                true,
                format!(
                    "{} written; could not save {}.",
                    key_count(saved.len()),
                    errors.join("; ")
                ),
            );
        }
    }

    /// Replace the status line and remember the style it should receive.
    fn set_status(&mut self, is_error: bool, status: String) {
        self.status = status;
        self.status_is_error = is_error;
    }
}

/// Format a grammatically useful write count for both success and error messages.
fn key_count(count: usize) -> String {
    format!("{count} key{}", if count == 1 { "" } else { "s" })
}
