//! A human prompt record: its delivered user item(s), and which server "sent" items it replaces.
//!
//! A record replaces one sent item whose match key ([`match_key`]) equals its own, and the
//! delivered item then shows the sent item's own text and attachments. A kickoff opening that
//! carries typed text is sent as two items, the guidance and the typed text, which Claude Code
//! records as one prompt joined by a blank line: that record replaces both and shows each once.

use chrono::{DateTime, Utc};

use super::records::{cap_chars, item_id, USER_TEXT_CHARS};
use super::{Ingest, Mapper, RecordMark};
use crate::plan_session::attachments::{parse_unwrapped, unwrap_pasted, Attachment};
use crate::plan_session::input::match_key;
use crate::plan_session::model::{ChatDelta, Delivery, Item, Origin};

/// The match key of a server user item (`s-<n>`), the key a record is matched by.
fn sent_key(item: &Item) -> Option<String> {
    match item {
        Item::User { id, text, .. } if id.starts_with("s-") => Some(match_key(text)),
        _ => None,
    }
}

fn is_single(items: &[Item], clean: &str) -> bool {
    items.first().and_then(sent_key).as_deref() == Some(clean)
}

fn is_pair(items: &[Item], clean: &str) -> bool {
    match items {
        [guidance, typed] => match (sent_key(guidance), sent_key(typed)) {
            (Some(guidance), Some(typed)) => format!("{guidance} {typed}") == clean,
            _ => false,
        },
        _ => false,
    }
}

/// The text, attachments and origin of a removed sent item.
fn parts(item: Item) -> (String, Vec<Attachment>, Option<Origin>) {
    match item {
        Item::User {
            text,
            attachments,
            origin,
            ..
        } => (text, attachments, origin),
        _ => (String::new(), Vec::new(), None),
    }
}

fn user(id: String, text: String, attachments: Vec<Attachment>, origin: Option<Origin>) -> Item {
    Item::User {
        id,
        text,
        attachments,
        delivery: Delivery::Delivered,
        origin,
        command: None,
    }
}

impl Mapper {
    /// A human prompt: delivered user item(s) with the attachment chips, replacing the server's
    /// "sent" item(s) of the same match key.
    pub(super) fn human(
        &mut self,
        uuid: &str,
        at: Option<DateTime<Utc>>,
        delivered: &str,
        out: &mut Ingest,
    ) {
        let delivered = cap_chars(&unwrap_pasted(delivered), USER_TEXT_CHARS);
        let (text, attachments) = parse_unwrapped(&delivered);
        let clean = match_key(&text);
        let origin = self.take_origin(&clean);
        let mut sent = self.take_server_items(&clean, &mut out.deltas);
        let turn = self.turn_for(at, &mut out.deltas);
        let id = format!("u-{uuid}");
        let items = match (sent.pop(), sent.pop()) {
            (Some(typed), Some(guidance)) => {
                let (guidance_text, _, guidance_origin) = parts(guidance);
                let (typed_text, _, _) = parts(typed);
                vec![
                    user(
                        id.clone(),
                        guidance_text,
                        Vec::new(),
                        origin.or(guidance_origin),
                    ),
                    user(format!("{id}-typed"), typed_text, attachments, None),
                ]
            }
            (Some(only), None) => {
                let (sent_text, sent_attachments, server_origin) = parts(only);
                vec![user(
                    id,
                    sent_text,
                    sent_attachments,
                    origin.or(server_origin),
                )]
            }
            _ => vec![user(id, text, attachments, origin)],
        };
        for item in items {
            self.push_item(turn, item, &mut out.deltas);
        }
        if let Some(at) = at {
            out.marks.push(RecordMark::HumanUser { at, text: clean });
        }
    }

    fn take_origin(&mut self, clean: &str) -> Option<Origin> {
        if self.closing.as_deref() == Some(clean) {
            self.closing = None;
            return Some(Origin::Closing);
        }
        self.opening.take().flatten()
    }

    /// Removes the server user items this record replaces, in order: the one sent item with
    /// this match key, else the two consecutive items whose keys joined by a space equal it. Latest turn first. A sent item the hook already marked delivered still counts:
    /// the hook arrives before the record.
    fn take_server_items(&mut self, clean: &str, deltas: &mut Vec<ChatDelta>) -> Vec<Item> {
        let found = self
            .locate(1, |items| is_single(items, clean))
            .map(|at| (1, at))
            .or_else(|| {
                let at = self.locate(2, |items| is_pair(items, clean))?;
                Some((2, at))
            });
        let Some((width, (turn, at))) = found else {
            return Vec::new();
        };
        let mut removed = Vec::with_capacity(width);
        for _ in 0..width {
            let item = self.turns[turn].items.remove(at);
            let turn_id = self.turns[turn].id.clone();
            let id = item_id(&item).to_owned();
            deltas.push(ChatDelta::ItemRemoved { turn_id, id });
            removed.push(item);
        }
        removed
    }

    /// The turn and position of the first run of `width` items `matches` accepts, latest turn
    /// first.
    fn locate(&self, width: usize, matches: impl Fn(&[Item]) -> bool) -> Option<(usize, usize)> {
        self.turns
            .iter()
            .enumerate()
            .rev()
            .find_map(|(turn, entry)| {
                let at = entry.items.windows(width).position(&matches)?;
                Some((turn, at))
            })
    }
}
