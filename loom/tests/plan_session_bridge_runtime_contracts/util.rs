//! Bounded waits, an owned child process and the transcript records the contracts append.

use std::thread;
use std::time::{Duration, Instant};

use loom::plan_session::kind::SessionKind;
use serde_json::{json, Value};

/// Poll `f` every 20 ms for at most 5 s.
pub fn wait_for<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(value) = f() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(20));
    }
}

pub fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
    wait_for(what, || f().then_some(()));
}

/// Long enough for a 20 ms driver tick to act on anything pending.
pub fn settle() {
    thread::sleep(Duration::from_millis(500));
}

/// A child process the test owns; killed and reaped on drop.
pub struct OwnedChild(pub std::process::Child);

impl OwnedChild {
    pub fn alive(&mut self) -> bool {
        matches!(self.0.try_wait(), Ok(None))
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn pressure() -> SessionKind {
    SessionKind::Pressure {
        plan: "doc/plans/PLAN-contracts.md".into(),
    }
}

/// One assistant message holding a `tool_use` block per `(id, name, input)`.
pub fn assistant_tools(tools: &[(&str, &str, Value)]) -> Value {
    let content: Vec<Value> = tools
        .iter()
        .map(
            |(id, name, input)| json!({"type": "tool_use", "id": id, "name": name, "input": input}),
        )
        .collect();
    json!({"type": "assistant", "message": {"id": "msg_contracts_1", "role": "assistant",
        "model": "claude-opus-5-5", "content": content, "stop_reason": "tool_use",
        "usage": {"input_tokens": 2, "output_tokens": 9}}})
}

pub fn tool_result(id: &str) -> Value {
    json!({"type": "user", "message": {"role": "user", "content": [{"type": "tool_result",
        "tool_use_id": id, "content": "doc/a.md contents", "is_error": false}]}})
}

/// The record Claude Code writes when Escape interrupts a turn.
pub fn interrupted() -> Value {
    json!({"type": "user", "message": {"role": "user",
        "content": [{"type": "text", "text": "[Request interrupted by user]"}]}})
}
