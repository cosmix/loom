//! A scripted codex app-server peer: answers named client requests, then replays
//! queued raw lines through the real frame reader, then goes silent or closes.

use std::collections::VecDeque;
use std::io::{self, Cursor};
use std::time::Duration;

use serde_json::{json, Value};

use super::clock::SteppingClock;
use super::{THREAD, TURN};
use crate::native_worker::model::MAX_FRAME_BYTES;
use crate::native_worker::protocol::frame::{read_frame, FrameError, Transport};

/// Polls after which the peer panics: a loop still polling by then never ends.
const MAX_POLLS: usize = 200_000;

/// What the peer does once every answer was given and every line was read.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    /// `recv` times out from then on, and the armed clock starts moving.
    Silent,
    /// `recv` reports end of stream from then on.
    Close,
}

struct Answer {
    method: &'static str,
    result: Value,
    then: Vec<Vec<u8>>,
}

pub(crate) struct ScriptedPeer {
    answers: Vec<Answer>,
    queue: VecDeque<Vec<u8>>,
    end: End,
    clock: Option<SteppingClock>,
    polls: usize,
    /// Every frame the loop sent, in order.
    pub(crate) sent: Vec<Value>,
}

impl ScriptedPeer {
    /// A peer that answers `initialize`, `thread/start` and `turn/start`, then
    /// delivers `after_turn_start` and ends as `end`.
    pub(crate) fn through_turn_start(after_turn_start: Vec<Vec<u8>>, end: End) -> Self {
        let mut peer = Self {
            answers: Vec::new(),
            queue: VecDeque::new(),
            end,
            clock: None,
            polls: 0,
            sent: Vec::new(),
        };
        peer.answer("initialize", json!({ "userAgent": "codex" }), Vec::new());
        peer.answer(
            "thread/start",
            json!({ "thread": { "id": THREAD } }),
            Vec::new(),
        );
        peer.answer(
            "turn/start",
            json!({ "turn": { "id": TURN } }),
            after_turn_start,
        );
        peer
    }

    fn answer(&mut self, method: &'static str, result: Value, then: Vec<Vec<u8>>) {
        self.answers.push(Answer {
            method,
            result,
            then,
        });
    }

    /// Arms `clock` when the peer goes silent.
    pub(crate) fn arming(mut self, clock: &SteppingClock) -> Self {
        self.clock = Some(clock.clone());
        self
    }

    /// The `method` of every request or notification the loop sent.
    pub(crate) fn sent_methods(&self) -> Vec<&str> {
        self.sent
            .iter()
            .filter_map(|frame| frame.get("method").and_then(Value::as_str))
            .collect()
    }

    fn ended(&mut self) -> Result<Option<Value>, FrameError> {
        match self.end {
            End::Silent => {
                if let Some(clock) = &self.clock {
                    clock.arm();
                }
                Ok(None)
            }
            End::Close => Err(FrameError::Eof),
        }
    }
}

impl Transport for ScriptedPeer {
    fn send(&mut self, frame: &Value) -> io::Result<()> {
        self.sent.push(frame.clone());
        let method = frame.get("method").and_then(Value::as_str);
        let (Some(id), Some(method)) = (frame.get("id"), method) else {
            return Ok(());
        };
        if let Some(index) = self.answers.iter().position(|a| a.method == method) {
            let answer = self.answers.remove(index);
            self.queue
                .push_back(line(&json!({ "id": id, "result": answer.result })));
            self.queue.extend(answer.then);
        }
        Ok(())
    }

    fn recv(&mut self, _timeout: Duration) -> Result<Option<Value>, FrameError> {
        self.polls += 1;
        assert!(
            self.polls <= MAX_POLLS,
            "the loop polled the peer {MAX_POLLS} times without ending"
        );
        if let Some(raw) = self.queue.pop_front() {
            return read_frame(&mut Cursor::new(raw), MAX_FRAME_BYTES).map(Some);
        }
        if self.answers.is_empty() {
            return self.ended();
        }
        Ok(None)
    }
}

/// One frame as the provider writes it: compact JSON and a newline.
pub(crate) fn line(frame: &Value) -> Vec<u8> {
    let mut raw = serde_json::to_vec(frame).expect("serialize frame");
    raw.push(b'\n');
    raw
}

/// `item/completed` carrying an `agentMessage` of the scripted turn.
pub(crate) fn agent_message(id: &str, text: &str, phase: &str) -> Vec<u8> {
    line(&json!({
        "method": "item/completed",
        "params": {
            "threadId": THREAD,
            "turnId": TURN,
            "item": { "type": "agentMessage", "id": id, "text": text, "phase": phase },
        },
    }))
}

/// `turn/completed` for the scripted turn with `status`.
pub(crate) fn turn_completed(status: &str) -> Vec<u8> {
    line(&json!({
        "method": "turn/completed",
        "params": { "threadId": THREAD, "turn": { "id": TURN, "status": status } },
    }))
}
