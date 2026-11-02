//! Context-owned module workers, including asynchronous HTTP jobs.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use anyhow::{bail, Result};

struct Worker {
    name: &'static str,
    handle: JoinHandle<()>,
    stop: Option<Arc<AtomicBool>>,
}

#[derive(Default)]
struct State {
    workers: Vec<Worker>,
    failed: Vec<&'static str>,
}

#[derive(Default)]
pub(super) struct Background(Mutex<State>);

impl Background {
    pub(super) fn own(
        &self,
        name: &'static str,
        handle: JoinHandle<()>,
        stop: Option<Arc<AtomicBool>>,
    ) {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut index = 0;
        while index < state.workers.len() {
            if state.workers[index].handle.is_finished() {
                let worker = state.workers.swap_remove(index);
                if worker.handle.join().is_err() {
                    state.failed.push(worker.name);
                }
            } else {
                index += 1;
            }
        }
        state.workers.push(Worker { name, handle, stop });
    }

    /// Called after request and shutdown-hook admission has ended. A running
    /// worker can register a descendant; repeat until all such parents finish.
    pub(super) fn stop_and_join(&self) -> Result<()> {
        loop {
            let workers = {
                let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
                if state.workers.is_empty() {
                    if !state.failed.is_empty() {
                        bail!("planning workers panicked: {}", state.failed.join(", "));
                    }
                    return Ok(());
                }
                std::mem::take(&mut state.workers)
            };
            for worker in &workers {
                if let Some(stop) = &worker.stop {
                    stop.store(true, Ordering::SeqCst);
                }
            }
            for worker in workers {
                if worker.handle.join().is_err() {
                    self.0
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .failed
                        .push(worker.name);
                }
            }
        }
    }
}
