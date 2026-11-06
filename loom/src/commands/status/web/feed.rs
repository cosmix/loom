//! The `/ws` message stream: turns each collected [`WebSnapshot`] into the
//! messages that bring a client from the previous snapshot to this one.
//!
//! Every message is a JSON text frame tagged `t`:
//!
//! - `full`: `{"t":"full","snapshot":<WebSnapshot>}`, the same JSON `/api/status`
//!   serves. The first message of every connection, and re-sent when the stage
//!   set or an immutable key changes and every [`FULL_INTERVAL`].
//! - `state`: stages, `status` keys and top-level keys that changed, each
//!   stage's dynamic part whole.
//! - `tokens`: the token flow, or null when it is gone.
//! - `tick`: the moving clock values. Sent on every collect, even when nothing
//!   moved, as the client's liveness signal.
//!
//! The clients' reconstruction rules are in `web/src/state/merge.ts`; the split
//! of keys is in [`super::feed_split`].

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Map, Value};

use super::feed_split::{needs_full, state_changes, tick_stages, Split};
use super::model::WebSnapshot;

#[cfg(test)]
#[path = "feed_stream_tests.rs"]
mod stream_tests;
#[cfg(test)]
#[path = "feed_tests.rs"]
mod tests;

/// How often a client is sent the whole snapshot again, so a bug in the delta
/// rules cannot leave a long-lived tab wrong for more than this long.
pub(super) const FULL_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Serialize)]
struct FullFrame<'a> {
    t: &'static str,
    snapshot: &'a WebSnapshot,
}

/// The latest snapshot and the state needed to diff the next one against it.
#[derive(Default)]
pub(super) struct Feed {
    latest: Option<(WebSnapshot, Split)>,
    last_full: Option<Instant>,
    /// The `full` message for `latest`, kept while no delta has followed it so
    /// a burst of connecting tabs serializes the snapshot once.
    primer: Option<Arc<String>>,
}

impl Feed {
    /// Take the next snapshot and return the messages to send for it, in
    /// order. A failure leaves the feed as it was.
    pub(super) fn accept(
        &mut self,
        snapshot: WebSnapshot,
        now: Instant,
    ) -> Result<Vec<Arc<String>>> {
        let split = Split::of(&snapshot)?;
        let deltas = match (&self.latest, self.last_full) {
            (Some((_, prev)), Some(sent))
                if now.saturating_duration_since(sent) < FULL_INTERVAL
                    && !needs_full(prev, &split) =>
            {
                Some(delta_frames(prev, &split)?)
            }
            _ => None,
        };
        let frames = match deltas {
            Some(frames) => {
                self.primer = None;
                frames
            }
            None => self.full_frames(&snapshot, now)?,
        };
        self.latest = Some((snapshot, split));
        Ok(frames)
    }

    fn full_frames(&mut self, snapshot: &WebSnapshot, now: Instant) -> Result<Vec<Arc<String>>> {
        let frame = encode_full(snapshot)?;
        self.primer = Some(Arc::clone(&frame));
        self.last_full = Some(now);
        Ok(vec![frame])
    }

    /// The `full` message that brings a new client to the latest snapshot.
    pub(super) fn full_frame(&mut self) -> Option<Arc<String>> {
        if self.primer.is_none() {
            let (snapshot, _) = self.latest.as_ref()?;
            match encode_full(snapshot) {
                Ok(frame) => self.primer = Some(frame),
                Err(error) => {
                    tracing::warn!("dashboard could not serialize a full frame: {error:#}");
                    return None;
                }
            }
        }
        self.primer.clone()
    }

    /// The latest snapshot as the bare JSON `/api/status` serves.
    pub(super) fn snapshot_json(&self) -> Option<Arc<String>> {
        let (snapshot, _) = self.latest.as_ref()?;
        serde_json::to_string(snapshot).ok().map(Arc::new)
    }

    pub(super) fn latest(&self) -> Option<&WebSnapshot> {
        self.latest.as_ref().map(|(snapshot, _)| snapshot)
    }
}

fn encode_full(snapshot: &WebSnapshot) -> Result<Arc<String>> {
    // Serialized from the struct, not a JSON value, so the keys keep the order
    // `/api/status` uses.
    serde_json::to_string(&FullFrame {
        t: "full",
        snapshot,
    })
    .map(Arc::new)
    .context("serialize full frame")
}

fn encode(kind: &'static str, mut body: Map<String, Value>) -> Result<Arc<String>> {
    body.insert("t".to_owned(), Value::from(kind));
    serde_json::to_string(&body)
        .map(Arc::new)
        .with_context(|| format!("serialize {kind} frame"))
}

/// `state` when something non-clock changed, `tokens` when the flow changed,
/// then the `tick` that is always sent.
fn delta_frames(prev: &Split, next: &Split) -> Result<Vec<Arc<String>>> {
    let mut frames = Vec::with_capacity(3);
    let (stages, status, mut body) = state_changes(prev, next);
    if !(stages.is_empty() && status.is_empty() && body.is_empty()) {
        body.insert("at".to_owned(), next.at.clone());
        if !stages.is_empty() {
            body.insert("stages".to_owned(), Value::Object(stages));
        }
        if !status.is_empty() {
            body.insert("status".to_owned(), Value::Object(status));
        }
        frames.push(encode("state", body)?);
    }
    if next.tokens != prev.tokens {
        let body = Map::from_iter([("tokens".to_owned(), next.tokens.clone())]);
        frames.push(encode("tokens", body)?);
    }
    let body = Map::from_iter([
        ("at".to_owned(), next.at.clone()),
        ("tick_age".to_owned(), next.tick_age.clone()),
        ("stages".to_owned(), Value::Object(tick_stages(prev, next)?)),
    ]);
    frames.push(encode("tick", body)?);
    Ok(frames)
}
