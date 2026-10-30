//! The fake session host and turn pins the bridge's unit tests run against (no tmux), and the
//! one import surface of those tests: the harness and the helpers are re-exported here.

use std::collections::HashMap;
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

pub(super) use super::harness::{Harness, Sess, CLOSING, OPENING};
use super::host::{LaunchSpec, SessionHost, Spawned, TeardownFailed};
pub(super) use super::tests::{
    append_record, assistant_tools, interrupted, pressure, record_line, settle, tool_result,
    wait_until,
};
use super::TurnPins;
use crate::process::{IdentityStatus, ProcessIdentity};

/// One call the bridge made on the host, by socket.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Call {
    Spawn(String),
    /// One paste together with its own Enter.
    Deliver(String, String),
    /// One further Enter, pressed without a paste.
    PressEnter(String),
    Interrupt(String),
    Teardown(String),
}

/// Records every call; identities are VerifiedAlive unless a test sets them.
#[derive(Default)]
pub(super) struct FakeHost {
    calls: Mutex<Vec<Call>>,
    specs: Mutex<Vec<(LaunchSpec, u32)>>,
    identities: Mutex<HashMap<u32, IdentityStatus>>,
    fail_teardown: AtomicBool,
    fail_spawn: AtomicBool,
    fail_record: AtomicBool,
    fail_deliver: AtomicBool,
    hold_spawn: AtomicBool,
    spawn_teardown_fails: AtomicBool,
    next_pid: AtomicU32,
}

impl FakeHost {
    pub(super) fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    fn log(&self, call: Call) {
        self.calls.lock().unwrap().push(call);
    }

    pub(super) fn spec(&self, id: &str) -> Option<(LaunchSpec, u32)> {
        let specs = self.specs.lock().unwrap();
        specs.iter().find(|(spec, _)| spec.id == id).cloned()
    }

    pub(super) fn set_identity(&self, pid: u32, status: IdentityStatus) {
        self.identities.lock().unwrap().insert(pid, status);
    }

    pub(super) fn fail_teardown(&self, fail: bool) {
        self.fail_teardown.store(fail, Ordering::SeqCst);
    }

    pub(super) fn fail_spawn(&self) {
        self.fail_spawn.store(true, Ordering::SeqCst);
    }

    /// A spawn swaps `session.json` for a symlink: the no-follow write completing it fails.
    pub(super) fn fail_record(&self) {
        self.fail_record.store(true, Ordering::SeqCst);
    }

    pub(super) fn fail_deliver(&self) {
        self.fail_deliver.store(true, Ordering::SeqCst);
    }

    /// Spawns block, at most 10 s, until `hold_spawn(false)`: a launch in flight.
    pub(super) fn hold_spawn(&self, hold: bool) {
        self.hold_spawn.store(hold, Ordering::SeqCst);
    }

    /// The next spawn fails after its server may have started, and its teardown fails too.
    pub(super) fn fail_spawn_teardown(&self) {
        self.spawn_teardown_fails.store(true, Ordering::SeqCst);
    }

    pub(super) fn delivered(&self, socket: &str) -> Vec<String> {
        let calls = self.calls();
        let texts = calls.into_iter().filter_map(|call| match call {
            Call::Deliver(to, text) if to == socket => Some(text),
            _ => None,
        });
        texts.collect()
    }

    pub(super) fn count(&self, wanted: &Call) -> usize {
        self.calls().iter().filter(|call| *call == wanted).count()
    }
}

impl SessionHost for FakeHost {
    fn spawn(&self, spec: &LaunchSpec) -> anyhow::Result<Spawned> {
        self.log(Call::Spawn(spec.socket.clone()));
        let held = Instant::now() + Duration::from_secs(10);
        while self.hold_spawn.load(Ordering::SeqCst) && Instant::now() < held {
            thread::sleep(Duration::from_millis(5));
        }
        anyhow::ensure!(
            !self.fail_spawn.load(Ordering::SeqCst),
            "scripted spawn failure"
        );
        if self.spawn_teardown_fails.swap(false, Ordering::SeqCst) {
            let error = anyhow::anyhow!("scripted identity failure");
            let teardown = anyhow::anyhow!("scripted teardown failure");
            return Err(anyhow::Error::new(TeardownFailed { error, teardown }));
        }
        if self.fail_record.load(Ordering::SeqCst) {
            // Refused for any uid, unlike a read-only mode; the record stays readable.
            let (record, kept) = (spec.dir.record(), spec.dir.path().join("record.kept"));
            fs::rename(&record, &kept)?;
            std::os::unix::fs::symlink(&kept, &record)?;
        }
        let pid = 3_700_000 + self.next_pid.fetch_add(1, Ordering::SeqCst);
        self.specs.lock().unwrap().push((spec.clone(), pid));
        let start_time = Some(42);
        let identity = ProcessIdentity { pid, start_time };
        Ok(Spawned { identity })
    }

    fn deliver(&self, socket: &str, _target: &str, text: &str) -> anyhow::Result<()> {
        self.log(Call::Deliver(socket.to_owned(), text.to_owned()));
        let failing = self.fail_deliver.load(Ordering::SeqCst);
        anyhow::ensure!(!failing, "scripted paste failure");
        Ok(())
    }

    fn press_enter(&self, socket: &str, _target: &str) -> anyhow::Result<()> {
        self.log(Call::PressEnter(socket.to_owned()));
        Ok(())
    }

    fn interrupt(&self, socket: &str, _target: &str) -> anyhow::Result<()> {
        self.log(Call::Interrupt(socket.to_owned()));
        Ok(())
    }

    fn teardown(&self, socket: &str) -> anyhow::Result<()> {
        self.log(Call::Teardown(socket.to_owned()));
        anyhow::ensure!(
            !self.fail_teardown.load(Ordering::SeqCst),
            "scripted teardown failure"
        );
        Ok(())
    }

    fn identity(&self, identity: ProcessIdentity) -> IdentityStatus {
        let identities = self.identities.lock().unwrap();
        let status = identities.get(&identity.pid).copied();
        status.unwrap_or(IdentityStatus::VerifiedAlive)
    }
}

/// Records each pin key; `end_turn` reports `changes`.
#[derive(Default)]
pub(super) struct FakePins {
    pub(super) begins: Mutex<Vec<String>>,
    pub(super) ends: Mutex<Vec<String>>,
    pub(super) changes: Mutex<Vec<(String, Option<String>)>>,
}

impl TurnPins for FakePins {
    fn begin_turn(&self, turn: &str) {
        self.begins.lock().unwrap().push(turn.to_owned());
    }

    fn end_turn(&self, turn: &str) -> Vec<(String, Option<String>)> {
        self.ends.lock().unwrap().push(turn.to_owned());
        self.changes.lock().unwrap().clone()
    }
}
