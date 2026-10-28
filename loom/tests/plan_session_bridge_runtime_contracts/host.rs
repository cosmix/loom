//! The scripted session host, the recording turn pins and the pinned tmux socket directory.

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use loom::plan_session::bridge::host::{LaunchSpec, SessionHost, Spawned};
use loom::plan_session::bridge::TurnPins;
use loom::process::{IdentityStatus, ProcessIdentity};

/// One call the bridge made on its host.
#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    Spawn {
        id: String,
        socket: String,
        pid: u32,
    },
    Deliver {
        socket: String,
        text: String,
    },
    Interrupt {
        socket: String,
    },
    Teardown {
        socket: String,
    },
}

/// A `SessionHost` that records every call, reports the identities a test sets (VerifiedAlive
/// otherwise), keeps each socket file present from spawn to a successful teardown, and fails
/// teardown on request.
#[derive(Default)]
pub struct ScriptedHost {
    calls: Mutex<Vec<Call>>,
    identities: Mutex<HashMap<u32, IdentityStatus>>,
    fail_teardown: AtomicBool,
    next_pid: AtomicU32,
}

impl ScriptedHost {
    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    pub fn set_identity(&self, pid: u32, status: IdentityStatus) {
        self.identities.lock().unwrap().insert(pid, status);
    }

    pub fn fail_teardown(&self) {
        self.fail_teardown.store(true, Ordering::SeqCst);
    }

    /// Report `socket` as present (its file exists where tmux keeps it) or absent.
    pub fn set_socket(&self, socket: &str, present: bool) {
        let path = socket_dir().join(socket);
        if present {
            fs::write(&path, b"").unwrap();
        } else {
            let _ = fs::remove_file(&path);
        }
    }

    pub fn spawned(&self, id: &str) -> Option<(String, u32)> {
        self.calls().into_iter().find_map(|call| match call {
            Call::Spawn {
                id: spawned,
                socket,
                pid,
            } if spawned == id => Some((socket, pid)),
            _ => None,
        })
    }

    pub fn spawns(&self) -> usize {
        let calls = self.calls();
        calls
            .iter()
            .filter(|c| matches!(c, Call::Spawn { .. }))
            .count()
    }

    pub fn delivered_to(&self, to: &str) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::Deliver { socket, text } if socket == to => Some(text),
                _ => None,
            })
            .collect()
    }

    pub fn interrupts(&self, to: &str) -> usize {
        let calls = self.calls();
        calls
            .iter()
            .filter(|c| matches!(c, Call::Interrupt { socket } if socket == to))
            .count()
    }

    pub fn teardowns(&self, to: &str) -> usize {
        let calls = self.calls();
        calls
            .iter()
            .filter(|c| matches!(c, Call::Teardown { socket } if socket == to))
            .count()
    }

    fn push(&self, call: Call) {
        self.calls.lock().unwrap().push(call);
    }
}

impl SessionHost for ScriptedHost {
    fn spawn(&self, spec: &LaunchSpec) -> anyhow::Result<Spawned> {
        let pid = 3_900_000 + self.next_pid.fetch_add(1, Ordering::SeqCst);
        self.set_socket(&spec.socket, true);
        self.push(Call::Spawn {
            id: spec.id.clone(),
            socket: spec.socket.clone(),
            pid,
        });
        Ok(Spawned {
            identity: ProcessIdentity {
                pid,
                start_time: Some(42),
            },
        })
    }

    fn deliver(&self, socket: &str, _target: &str, text: &str) -> anyhow::Result<()> {
        self.push(Call::Deliver {
            socket: socket.to_owned(),
            text: text.to_owned(),
        });
        Ok(())
    }

    fn interrupt(&self, socket: &str, _target: &str) -> anyhow::Result<()> {
        self.push(Call::Interrupt {
            socket: socket.to_owned(),
        });
        Ok(())
    }

    fn teardown(&self, socket: &str) -> anyhow::Result<()> {
        self.push(Call::Teardown {
            socket: socket.to_owned(),
        });
        if self.fail_teardown.load(Ordering::SeqCst) {
            anyhow::bail!("scripted teardown failure for {socket}");
        }
        self.set_socket(socket, false);
        Ok(())
    }

    fn identity(&self, identity: ProcessIdentity) -> IdentityStatus {
        let identities = self.identities.lock().unwrap();
        identities
            .get(&identity.pid)
            .copied()
            .unwrap_or(IdentityStatus::VerifiedAlive)
    }
}

/// A `TurnPins` that records each `begin_turn` and `end_turn` key.
#[derive(Default)]
pub struct RecordingPins {
    begins: Mutex<Vec<String>>,
    ends: Mutex<Vec<String>>,
}

impl RecordingPins {
    pub fn begins(&self) -> Vec<String> {
        self.begins.lock().unwrap().clone()
    }

    pub fn ends(&self) -> Vec<String> {
        self.ends.lock().unwrap().clone()
    }
}

impl TurnPins for RecordingPins {
    fn begin_turn(&self, turn: &str) {
        self.begins.lock().unwrap().push(turn.to_owned());
    }

    fn end_turn(&self, turn: &str) -> Vec<(String, Option<String>)> {
        self.ends.lock().unwrap().push(turn.to_owned());
        Vec::new()
    }
}

/// `$TMUX_TMPDIR/tmux-<uid>`, with `TMUX_TMPDIR` pinned once for the whole test binary (before
/// any bridge exists) so every test and every bridge thread agree on it.
pub fn socket_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let base = tempfile::Builder::new()
            .prefix("bridge-runtime-tmux")
            .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
            .unwrap()
            .keep();
        std::env::set_var("TMUX_TMPDIR", &base);
        let uid = fs::metadata(&base).unwrap().uid();
        let dir = base.join(format!("tmux-{uid}"));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        dir
    })
}
