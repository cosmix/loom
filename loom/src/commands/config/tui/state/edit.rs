//! Staging edits against the active scope.
//!
//! Nothing here touches a file. Every operation lands in the focused row's
//! pending slot for the tier the operator is looking at, which is what makes
//! the whole screen reversible until `s`.

use crate::user_config::{keys::ValueKind, ConfigValue};

use super::{opens_editor, ConfigState, Pending, ProjectTier, NO_WORKSPACE};

impl ConfigState {
    /// Start a reversible edit seeded with the active scope's displayed value.
    pub(crate) fn begin_edit(&mut self) {
        let Some(text) = self.selected_row().displayed_text(self.scope) else {
            self.report_unavailable();
            return;
        };
        self.edit_buffer = Some(text);
        self.set_status(false, "Editing: Enter commits; Esc cancels.".to_owned());
    }

    /// Step the active scope's value without opening the text editor.
    pub(crate) fn cycle(&mut self, delta: i32) {
        let scope = self.scope;
        let (spec, current) = {
            let row = self.selected_row();
            (row.spec(), row.displayed(scope))
        };
        let Some(current) = current else {
            self.report_unavailable();
            return;
        };
        let value = match (&spec.kind, current) {
            (ValueKind::Bool, ConfigValue::Bool(value)) => ConfigValue::Bool(!value),
            (ValueKind::Enum(variants), ConfigValue::Text(value)) => {
                let index = variants
                    .iter()
                    .position(|variant| *variant == value.as_str())
                    .unwrap_or(0) as i32;
                let next = (index + delta).rem_euclid(variants.len() as i32) as usize;
                ConfigValue::Text(variants[next].to_owned())
            }
            (ValueKind::Number | ValueKind::String, _) => {
                self.set_status(
                    true,
                    format!(
                        "{} has no variants to cycle; press Enter to edit.",
                        spec.name
                    ),
                );
                return;
            }
            _ => unreachable!("strict config values match their registry kind"),
        };
        self.stage_selected(
            Pending::Set {
                raw: value.to_string(),
                value,
            },
            format!("{} staged; press s to save.", spec.name),
        );
    }

    /// Stage the removal of the focused key from the active scope's file,
    /// reverting it to whatever that scope inherits.
    pub(crate) fn stage_clear(&mut self) {
        let scope = self.scope;
        let row = self.selected_row();
        let name = row.spec().name;
        let available = row.displayed(scope).is_some();
        let set = row.is_set(scope);
        if !available {
            self.report_unavailable();
        } else if !set {
            self.set_status(
                false,
                format!(
                    "{name} is not set at {} scope; nothing to clear.",
                    scope.word()
                ),
            );
        } else {
            self.stage_selected(
                Pending::Clear,
                format!("{name} staged for clearing; press s to save."),
            );
        }
    }

    /// Open the text editor for a free-form kind, otherwise step it forward.
    pub(crate) fn activate(&mut self) {
        if opens_editor(&self.selected_row().spec().kind) {
            self.begin_edit();
        } else {
            self.cycle(1);
        }
    }

    /// Append a printable character while an inline edit owns the keyboard.
    pub(crate) fn append_char(&mut self, character: char) {
        if let Some(buffer) = &mut self.edit_buffer {
            buffer.push(character);
        }
    }

    /// Remove one Unicode scalar from the inline editor without touching the row.
    pub(crate) fn backspace(&mut self) {
        if let Some(buffer) = &mut self.edit_buffer {
            buffer.pop();
        }
    }

    /// Discard only the in-progress text; a row changes solely after a valid commit.
    pub(crate) fn cancel_edit(&mut self) {
        if self.edit_buffer.take().is_some() {
            self.set_status(false, "Edit cancelled; value restored.".to_owned());
        }
    }

    /// Validate the buffer through its `KeySpec`, staging valid values against
    /// the active scope.
    pub(crate) fn commit_edit(&mut self) {
        let Some(raw) = self.edit_buffer.clone() else {
            return;
        };
        let spec = self.selected_row().spec();
        match spec.parse(&raw) {
            Ok(value) => {
                self.edit_buffer = None;
                self.stage_selected(
                    Pending::Set { raw, value },
                    format!("{} staged; press s to save.", spec.name),
                );
            }
            Err(error) => self.set_status(true, error.to_string()),
        }
    }

    /// Stage `pending` against the focused row at the active scope.
    fn stage_selected(&mut self, pending: Pending, status: String) {
        let scope = self.scope;
        self.rows[self.selected].stage(scope, pending);
        self.set_status(false, status);
    }

    /// The active scope has nothing to edit for the focused row. Which of the
    /// two reasons it is matters: one is fixed by `loom init`, the other
    /// cannot be fixed at all.
    fn report_unavailable(&mut self) {
        let name = self.selected_row().spec().name;
        let status = match self.selected_row().project() {
            ProjectTier::Unbacked => {
                format!("{name} has no project scope; it is a user-level setting only.")
            }
            _ => NO_WORKSPACE.to_owned(),
        };
        self.set_status(false, status);
    }
}
