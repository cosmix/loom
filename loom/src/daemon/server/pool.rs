//! Fixed-size, bounded worker pool for daemon client handling.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

type Job = Box<dyn FnOnce() + Send + 'static>;

/// A fixed worker set backed by a bounded, nonblocking submission queue.
pub(super) struct WorkerPool {
    sender: Option<SyncSender<Job>>,
    workers: Vec<JoinHandle<()>>,
}

impl WorkerPool {
    pub(super) fn new(worker_count: usize, queue_capacity: usize) -> Self {
        assert!(worker_count > 0, "worker count must be positive");
        let (sender, receiver) = mpsc::sync_channel(queue_capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let workers = (0..worker_count)
            .map(|index| spawn_worker(index, Arc::clone(&receiver)))
            .collect();
        Self {
            sender: Some(sender),
            workers,
        }
    }

    /// Submit work without waiting for queue space.
    ///
    /// `false` means the bounded queue is full or the pool is shutting down.
    pub(super) fn try_execute<F>(&self, job: F) -> bool
    where
        F: FnOnce() + Send + 'static,
    {
        let Some(sender) = &self.sender else {
            return false;
        };
        match sender.try_send(Box::new(job)) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
        }
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        self.sender.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn spawn_worker(index: usize, receiver: Arc<Mutex<Receiver<Job>>>) -> JoinHandle<()> {
    thread::Builder::new()
        .name(format!("loom-daemon-client-{index}"))
        .spawn(move || worker_loop(&receiver))
        .expect("failed to spawn daemon client worker")
}

fn worker_loop(receiver: &Mutex<Receiver<Job>>) {
    loop {
        let result = receiver
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .recv();
        let Ok(job) = result else {
            break;
        };
        let _ = catch_unwind(AssertUnwindSafe(job));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc, Condvar};
    use std::time::Duration;

    #[test]
    fn queue_saturation_rejects_immediately_and_accepted_jobs_run() {
        let pool = WorkerPool::new(1, 1);
        let blocker = Arc::new((Mutex::new(false), Condvar::new()));
        let worker_blocker = Arc::clone(&blocker);
        let (started_tx, started_rx) = mpsc::channel();
        assert!(pool.try_execute(move || {
            started_tx.send(()).unwrap();
            let (lock, wake) = &*worker_blocker;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = wake.wait(released).unwrap();
            }
        }));
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        let completed = Arc::new(AtomicUsize::new(0));
        let queued_completed = Arc::clone(&completed);
        assert!(pool.try_execute(move || {
            queued_completed.fetch_add(1, Ordering::SeqCst);
        }));
        assert!(!pool.try_execute(|| panic!("saturated job must not run")));

        let (lock, wake) = &*blocker;
        *lock.lock().unwrap() = true;
        wake.notify_one();
        drop(pool);
        assert_eq!(completed.load(Ordering::SeqCst), 1);
    }
}
