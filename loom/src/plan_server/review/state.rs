//! Shared state of the review loop, held in the context's extensions by `routes::install`.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use crate::plan_server::ids::BrowserId;

use super::suggest::SuggestionBox;

/// The suggestion box, where each suggestion came from, and the last open-question counts sent.
#[derive(Default)]
pub(crate) struct ReviewState {
    pub(crate) suggestions: Mutex<SuggestionBox>,
    /// Suggestion id to the sender's browser (a cookie value: used to route, never sent).
    pub(crate) senders: Mutex<HashMap<String, BrowserId>>,
    /// The counts last broadcast, so a batch that changes none sends nothing.
    pub(crate) counts: Mutex<BTreeMap<String, usize>>,
}
