//! A session's command table and launch spec as the catalogue owner reads them, and the class
//! check a slash message passes when it is taken for delivery (B-D2, "Checked again at
//! delivery"), which is the send gate's own rule (`input::class_refusal`).

use std::sync::{Arc, PoisonError};

use super::host::LaunchSpec;
use super::session::{Delivering, LocalSettle, Phase, Session, Slash};
use super::Bridge;
use crate::plan_session::commands::{CommandCell, CommandClass, CommandTable};
use crate::plan_session::input::{class_refusal, slash_line};

impl Bridge {
    /// The session's command cell; its table is `CommandTable::loading()` until the catalogue
    /// owner swaps one in. Never write it while holding the bridge's lock.
    pub fn command_cell(&self, id: &str) -> Option<CommandCell> {
        let inner = self.lock();
        let session = inner.sessions.get(id)?;
        Some(Arc::clone(&session.command_cell))
    }

    /// The spec the live session was launched with.
    pub fn launch_spec(&self, id: &str) -> Option<LaunchSpec> {
        let inner = self.lock();
        let session = inner.sessions.get(id)?;
        let live = matches!(session.phase, Phase::Live);
        session.spec.as_deref().filter(|_| live).cloned()
    }
}

impl Session {
    /// The session's table now.
    pub(super) fn table(&self) -> Arc<CommandTable> {
        let table = self.command_cell.read();
        Arc::clone(&table.unwrap_or_else(PoisonError::into_inner))
    }

    /// The canonical name of `/name`: its row's name when the table lists it, else as given.
    pub(super) fn canonical(&self, name: &str) -> String {
        let table = self.table();
        table
            .lookup(name)
            .map_or(name, |entry| &entry.name)
            .to_owned()
    }

    /// The command `text` names, classed by the table now: `None` for a message that is no slash
    /// command, `Err` with the reason the table refuses it.
    pub(super) fn slash_of(&self, text: &str) -> Option<Result<Slash, String>> {
        let (typed, args) = slash_line(text)?;
        let table = self.table();
        if let Some(reason) = class_refusal(&table, typed) {
            return Some(Err(reason));
        }
        let entry = table.lookup(typed)?;
        Some(Ok(Slash {
            name: entry.name.clone(),
            typed: typed.to_owned(),
            args: args.to_owned(),
            class: entry.class.clone(),
        }))
    }
}

impl Delivering {
    /// The delivery of `slash`: a command run with handling is local from its paste on.
    pub(super) fn with_slash(mut self, slash: Option<Slash>) -> Self {
        let handled = slash
            .as_ref()
            .is_some_and(|slash| slash.class == CommandClass::RunWithHandling);
        if handled {
            self.local = Some(LocalSettle::new(&self.item_ids));
        }
        self.slash = slash;
        self
    }

    /// It runs `/name`, given canonical (`name`) and as the record wrote it (`raw`).
    pub(super) fn runs(&self, name: &str, raw: &str) -> bool {
        let slash = self.slash.as_ref();
        slash.is_some_and(|slash| slash.name == name || slash.typed == raw)
    }

    pub(super) fn is_compact(&self) -> bool {
        self.slash
            .as_ref()
            .is_some_and(|slash| slash.name == "compact")
    }

    /// A confirmed local command: it holds the slot until it settles, past any timeout.
    pub(super) fn settling(&self) -> bool {
        let local = self.local.as_ref();
        local.is_some_and(|local| local.confirmed_at.is_some())
    }
}

impl LocalSettle {
    /// Waiting for its confirming record.
    pub(super) fn new(item_ids: &[String]) -> Self {
        LocalSettle {
            delivery_id: item_ids.first().cloned().unwrap_or_default(),
            confirmed_at: None,
            last_record: None,
            busy: false,
        }
    }
}
