//! Delivery of the outbox to the listeners: one thread at a time, in order.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;
use std::thread;

use super::commands::Out;
use super::recovery::input_unsaved;
use super::{guard, Bridge, BridgeEvent, Listener};

impl Bridge {
    /// Hand the outbox to the listeners; returns at once while another call is doing it.
    pub(super) fn flush(&self) {
        while self.claim_flush(false) {
            self.drain_outbox();
            // A step that queued while this call was finishing found the flag still set.
            if !self.release_flush() {
                return;
            }
        }
    }

    /// Publish `event` after everything already queued, on this thread, waiting for any
    /// other flushing thread to finish first. Recovery needs the listeners to have run
    /// before it returns, and the event must not be taken by another thread's flush.
    pub(super) fn flush_event(&self, event: BridgeEvent) {
        self.claim_flush(true);
        self.lock().outbox.push(Out::Event(event));
        self.drain_outbox();
        if self.release_flush() {
            self.flush();
        }
    }

    /// Take the right to publish. With `wait`, sleeps a tick between attempts until it
    /// holds it; without, gives up while another thread holds it.
    fn claim_flush(&self, wait: bool) -> bool {
        loop {
            let claimed =
                self.flushing
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
            if claimed.is_ok() {
                return true;
            }
            if !wait {
                return false;
            }
            thread::sleep(self.config.tick);
        }
    }

    /// Give the right back; whether the outbox filled again meanwhile.
    fn release_flush(&self) -> bool {
        self.flushing.store(false, Ordering::SeqCst);
        !self.lock().outbox.is_empty()
    }

    fn drain_outbox(&self) {
        loop {
            let batch = std::mem::take(&mut self.lock().outbox);
            if batch.is_empty() {
                return;
            }
            self.publish(batch);
        }
    }

    fn publish(&self, batch: Vec<Out>) {
        let events = guard(&self.listeners).clone();
        let deltas = guard(&self.chat_listeners).clone();
        for out in batch {
            match out {
                Out::Event(event) => {
                    let returned = matches!(event, BridgeEvent::InputReturned { .. });
                    for listener in &events {
                        // A listener that died may not have saved the texts it was given.
                        if hear(listener, &event) && returned {
                            input_unsaved();
                        }
                    }
                }
                Out::Delta(delta) => {
                    deltas.iter().for_each(|listener| {
                        hear(listener, &*delta);
                    });
                }
            }
        }
    }
}

/// Call one listener; whether it panicked. A panic stays here: unwinding into the flush
/// would drop the rest of the batch for every listener and stop the flushing thread
/// (recovery would never finish).
fn hear<T>(listener: &Listener<T>, value: &T) -> bool {
    let panicked = catch_unwind(AssertUnwindSafe(|| listener(value))).is_err();
    if panicked {
        tracing::error!("a session bridge listener panicked");
    }
    panicked
}
