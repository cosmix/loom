//! Pure state transitions for the native worker protocol.

use std::path::PathBuf;

use serde_json::Value;

use super::model::{ControlMessage, NativeEvent, Outcome, SandboxMode};
use super::protocol::{
    correlate::{Correlator, Pending},
    frame::{Incoming, RpcError},
    policy,
    report::ReportAccumulator,
    wire,
};
use messages::MessageBook;

mod messages;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_cancel;
#[cfg(test)]
mod tests_messages;
#[cfg(test)]
mod tests_turns;
mod turns;

#[derive(Debug, Clone)]
pub struct MachineConfig {
    pub cwd: PathBuf,
    pub model: String,
    pub effort: String,
    pub task_text: String,
    pub sandbox_mode: SandboxMode,
    pub client_version: String,
}

#[derive(Debug, Clone)]
pub enum Input {
    Start,
    Frame(Incoming),
    Control(ControlMessage),
    Cancel { reason: String },
    Deadline,
    GraceElapsed,
    TransportClosed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Send(Value),
    Persist(NativeEvent),
    StoreReport { text: String, complete: bool },
    ArmDeadline,
    ArmGrace,
    Teardown { outcome: Outcome, reason: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    New,
    AwaitInit,
    AwaitThread,
    InTurn,
    Finished,
}

struct Closed {
    outcome: Outcome,
    reason: String,
}

pub struct Machine {
    config: MachineConfig,
    state: State,
    correlator: Correlator,
    thread_id: Option<String>,
    turn_id: Option<String>,
    done_turns: Vec<String>,
    closed: Option<Closed>,
    interrupt_sent: bool,
    messages: MessageBook,
    report: ReportAccumulator,
}

impl Machine {
    pub fn new(config: MachineConfig) -> Self {
        Self {
            config,
            state: State::New,
            correlator: Correlator::new(),
            thread_id: None,
            turn_id: None,
            done_turns: Vec::new(),
            closed: None,
            interrupt_sent: false,
            messages: MessageBook::default(),
            report: ReportAccumulator::new(),
        }
    }

    pub fn step(&mut self, input: Input) -> Vec<Action> {
        if self.is_finished() {
            return Vec::new();
        }
        match input {
            Input::Start => self.on_start(),
            Input::Frame(frame) => self.on_frame(frame),
            Input::Control(message) => self.on_control(message),
            Input::Cancel { reason } => self.on_cancel(reason),
            Input::Deadline => self.on_deadline(),
            Input::GraceElapsed => self.on_grace(),
            Input::TransportClosed => {
                self.end(Outcome::Failed, "codex app-server closed its output")
            }
        }
    }

    pub fn is_finished(&self) -> bool {
        self.state == State::Finished
    }

    fn request(&mut self, pending: Pending, build: impl FnOnce(u64) -> Value) -> Action {
        let id = self.correlator.register(pending);
        Action::Send(build(id))
    }

    fn finish(&mut self, outcome: Outcome, reason: impl Into<String>) -> Vec<Action> {
        if self.is_finished() {
            return Vec::new();
        }
        self.state = State::Finished;
        let mut actions = self.messages.settle();
        actions.push(Action::Teardown {
            outcome,
            reason: reason.into(),
        });
        actions
    }

    fn end(&mut self, outcome: Outcome, reason: impl Into<String>) -> Vec<Action> {
        if let Some(closed) = &self.closed {
            self.finish(closed.outcome, closed.reason.clone())
        } else {
            self.finish(outcome, reason)
        }
    }

    fn on_start(&mut self) -> Vec<Action> {
        if self.state != State::New {
            return Vec::new();
        }
        self.state = State::AwaitInit;
        let version = self.config.client_version.clone();
        vec![self.request(Pending::Initialize, |id| wire::initialize(id, &version))]
    }

    fn on_frame(&mut self, frame: Incoming) -> Vec<Action> {
        match frame {
            Incoming::Response { id, result } => match self.correlator.resolve(id) {
                Some(pending) => self.on_response(pending, result),
                None => Vec::new(),
            },
            Incoming::Notification { method, params } => self.on_notification(&method, &params),
            Incoming::ServerRequest { id, method, .. } => vec![
                Action::Send(wire::server_reply(&id, &policy::reply_for(&method))),
                Action::Persist(NativeEvent::ServerRequestRefused { method }),
            ],
        }
    }

    fn on_response(&mut self, pending: Pending, result: Result<Value, RpcError>) -> Vec<Action> {
        match pending {
            Pending::Initialize => self.on_initialize(result),
            Pending::ThreadStart => self.on_thread_start(result),
            Pending::TurnStart => self.on_turn_start(result),
            Pending::TurnSteer { message_id } => self.messages.steer_result(
                &message_id,
                result.map(|_| ()).map_err(|error| error.message),
            ),
            Pending::TurnInterrupt => Vec::new(),
        }
    }

    fn on_initialize(&mut self, result: Result<Value, RpcError>) -> Vec<Action> {
        if let Err(error) = result {
            return self.end(
                Outcome::Failed,
                format!("initialize failed: {}", error.message),
            );
        }
        if self.closed.is_some() {
            return self.end(Outcome::Cancelled, "worker is closing");
        }
        self.state = State::AwaitThread;
        let config = self.config.clone();
        vec![
            Action::Send(wire::initialized()),
            self.request(Pending::ThreadStart, |id| {
                wire::thread_start(
                    id,
                    &config.model,
                    &config.cwd,
                    config.sandbox_mode,
                    wire::DEVELOPER_INSTRUCTIONS,
                )
            }),
        ]
    }

    fn on_thread_start(&mut self, result: Result<Value, RpcError>) -> Vec<Action> {
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                return self.end(
                    Outcome::Failed,
                    format!("thread/start failed: {}", error.message),
                );
            }
        };
        let Some(thread_id) = wire::thread_id_from_result(&value) else {
            return self.end(Outcome::Failed, "thread/start response missing thread.id");
        };
        self.thread_id = Some(thread_id.clone());
        let mut actions = vec![Action::Persist(NativeEvent::ThreadStarted {
            thread_id: thread_id.clone(),
        })];
        if self.closed.is_some() {
            actions.extend(self.end(Outcome::Cancelled, "worker is closing"));
            return actions;
        }
        self.state = State::InTurn;
        let task_text = self.config.task_text.clone();
        actions.push(self.turn_request(&thread_id, std::slice::from_ref(&task_text)));
        actions.push(Action::ArmDeadline);
        actions
    }
}
