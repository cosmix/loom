//! Suggestion box: passages sent to the authoring holder.

use crate::plan_server::wire::paths::to_doc_rel;
use crate::plan_server::wire::review::{ReviewMessage, SuggestionStatus};
use crate::planning::paths::check_relpath_syntax;
use serde_json::{json, Value};

pub use crate::plan_server::wire::review::{
    SuggestedPassageWire as SuggestedPassage, SuggestionActionWire as SuggestionAction,
};

/// Browser identity supplied by the authenticated routing layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserRef {
    /// Opaque browser identifier the caller chooses (production: the public id); the box never routes by it.
    pub id: String,
    pub public_id: String,
    pub name: String,
}

/// A message routed to exactly one browser, outside its serialized payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    pub to_browser: String,
    pub message: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestRefusal {
    NoHolder,
    SenderIsHolder,
    NotHolder,
    NoteTooLong,
    TooMany,
    BadPath(String),
    UnknownId,
}

#[derive(Debug)]
struct PendingSuggestion {
    id: String,
    sender_id: String,
    message: Value,
}

/// At most twenty cards in submission order, scoped to the current holder.
#[derive(Debug, Default)]
pub struct SuggestionBox {
    counter: u64,
    entries: Vec<PendingSuggestion>,
}

impl SuggestionBox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit(
        &mut self,
        sender: &BrowserRef,
        holder: Option<&BrowserRef>,
        passage: SuggestedPassage,
        note: &str,
    ) -> Result<Delivery, SuggestRefusal> {
        let holder = holder.ok_or(SuggestRefusal::NoHolder)?;
        if sender.id == holder.id {
            return Err(SuggestRefusal::SenderIsHolder);
        }
        if note.chars().count() > 1000 {
            return Err(SuggestRefusal::NoteTooLong);
        }
        if self.entries.len() >= 20 {
            return Err(SuggestRefusal::TooMany);
        }
        validate_path(&passage.path)?;
        self.counter = self.counter.checked_add(1).ok_or(SuggestRefusal::TooMany)?;
        let id = format!("sg-{}", self.counter);
        let message = review_message(ReviewMessage::Suggestion {
            id: id.clone(),
            from: sender.name.clone(),
            passage,
            note: note.to_owned(),
        });
        self.entries.push(PendingSuggestion {
            id,
            sender_id: sender.id.clone(),
            message: message.clone(),
        });
        Ok(Delivery {
            to_browser: holder.id.clone(),
            message,
        })
    }

    pub fn answer(
        &mut self,
        by: &BrowserRef,
        holder: Option<&BrowserRef>,
        id: &str,
        action: SuggestionAction,
    ) -> Result<Delivery, SuggestRefusal> {
        let holder = holder.ok_or(SuggestRefusal::NoHolder)?;
        if by.id != holder.id {
            return Err(SuggestRefusal::NotHolder);
        }
        let index = self
            .entries
            .iter()
            .position(|entry| entry.id == id)
            .ok_or(SuggestRefusal::UnknownId)?;
        let entry = self.entries.remove(index);
        let status = match action {
            SuggestionAction::Attach => SuggestionStatus::Attached,
            SuggestionAction::Dismiss => SuggestionStatus::Dismissed,
        };
        Ok(status_delivery(entry, status))
    }

    pub fn pending(&self) -> usize {
        self.entries.len()
    }

    /// Clear cards when the routing layer reports an authoring-holder change.
    pub fn drop_for_holder_change(&mut self) -> Vec<Delivery> {
        self.entries
            .drain(..)
            .map(|entry| {
                status_delivery(
                    entry,
                    SuggestionStatus::Dropped {
                        reason: "Authoring holder changed.".to_owned(),
                    },
                )
            })
            .collect()
    }

    /// Cards for replay after the routing layer authorizes a holder reconnect.
    pub fn pending_for(&self) -> Vec<Value> {
        self.entries
            .iter()
            .map(|entry| entry.message.clone())
            .collect()
    }
}

fn validate_path(path: &str) -> Result<(), SuggestRefusal> {
    let relative = to_doc_rel(path)
        .ok_or_else(|| SuggestRefusal::BadPath("path must start with doc/".to_owned()))?;
    check_relpath_syntax(relative).map_err(|error| SuggestRefusal::BadPath(error.to_string()))?;
    Ok(())
}

fn review_message(message: ReviewMessage) -> Value {
    let mut value = json!(message);
    value["type"] = json!("review");
    value
}

fn status_delivery(entry: PendingSuggestion, status: SuggestionStatus) -> Delivery {
    Delivery {
        to_browser: entry.sender_id,
        message: review_message(ReviewMessage::SuggestionStatus {
            id: entry.id,
            status,
        }),
    }
}
