//! Tests for the two-signal shutdown registry.

use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Barrier, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::shutdown::{Shutdown, ShutdownHook, SignalHandler};

const WAIT: Duration = Duration::from_secs(10);

type Log = Arc<Mutex<Vec<String>>>;

fn hook(name: &'static str, drain: impl FnOnce() + Send + 'static) -> ShutdownHook {
    ShutdownHook {
        name,
        drain: Box::new(drain),
        abort: Box::new(|| {}),
    }
}

fn recording(log: &Log) -> impl Fn(&str) + Send + Sync + 'static {
    let log = Arc::clone(log);
    move |line| log.lock().unwrap().push(line.to_owned())
}

fn lines(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

/// Join `handle` on a helper thread so a hung drain fails the test instead
/// of hanging it.
fn joins_within(handle: JoinHandle<()>, limit: Duration) -> bool {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = handle.join();
        let _ = tx.send(());
    });
    rx.recv_timeout(limit).is_ok()
}

fn wait_until(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(10));
    }
}

fn gated(name: &'static str) -> (ShutdownHook, mpsc::Sender<()>) {
    let (tx, rx): (_, Receiver<()>) = mpsc::channel();
    let drain = move || {
        let _ = rx.recv();
    };
    (hook(name, drain), tx)
}

#[test]
fn drains_run_concurrently() {
    let shutdown = Shutdown::new();
    let barrier = Arc::new(Barrier::new(2));
    for name in ["first", "second"] {
        let barrier = Arc::clone(&barrier);
        shutdown.register(hook(name, move || {
            barrier.wait();
        }));
    }
    assert!(!shutdown.is_stopping());
    let handle = shutdown.begin(|_| {});
    assert!(shutdown.is_stopping());
    assert!(joins_within(handle, WAIT), "both drains must run at once");
}

#[test]
fn report_names_the_running_hooks() {
    let shutdown = Shutdown::new();
    let (a, release_a) = gated("a");
    let (b, release_b) = gated("b");
    shutdown.register(a);
    shutdown.register(b);
    let log = Log::default();
    let handle = shutdown.begin(recording(&log));
    assert_eq!(lines(&log), ["waiting on: a, b"]);
    release_a.send(()).unwrap();
    wait_until("a to finish", || lines(&log).len() == 2);
    assert_eq!(lines(&log)[1], "finished: a");
    release_b.send(()).unwrap();
    assert!(joins_within(handle, WAIT));
    assert_eq!(
        lines(&log),
        ["waiting on: a, b", "finished: a", "finished: b"]
    );
}

#[test]
fn abort_all_runs_each_pending_abort_once_in_order() {
    let shutdown = Shutdown::new();
    let log = Log::default();
    let drained = Arc::new(AtomicUsize::new(0));
    for name in ["x", "y"] {
        let (log, drained) = (Arc::clone(&log), Arc::clone(&drained));
        shutdown.register(ShutdownHook {
            name,
            drain: Box::new(move || {
                drained.fetch_add(1, Ordering::SeqCst);
            }),
            abort: Box::new(move || log.lock().unwrap().push(name.to_owned())),
        });
    }
    shutdown.abort_all();
    shutdown.abort_all();
    assert_eq!(lines(&log), ["x", "y"]);
    assert!(shutdown.is_stopping());
    assert!(joins_within(shutdown.begin(|_| {}), WAIT));
    assert_eq!(
        drained.load(Ordering::SeqCst),
        0,
        "aborted drains never start"
    );
}

#[test]
fn abort_all_skips_finished_drains_and_stops_running_ones() {
    let shutdown = Shutdown::new();
    let aborted = Log::default();
    let (stop, stopped) = mpsc::channel::<()>();
    let record = |name: &'static str| {
        let aborted = Arc::clone(&aborted);
        Box::new(move || aborted.lock().unwrap().push(name.to_owned()))
    };
    shutdown.register(ShutdownHook {
        name: "quick",
        drain: Box::new(|| {}),
        abort: record("quick"),
    });
    let abort_slow = record("slow");
    shutdown.register(ShutdownHook {
        name: "slow",
        drain: Box::new(move || {
            let _ = stopped.recv();
        }),
        abort: Box::new(move || {
            abort_slow();
            let _ = stop.send(());
        }),
    });
    let log = Log::default();
    let handle = shutdown.begin(recording(&log));
    wait_until("quick to finish", || {
        lines(&log).contains(&"finished: quick".to_owned())
    });
    shutdown.abort_all();
    shutdown.abort_all();
    assert!(
        joins_within(handle, WAIT),
        "the abort must end the slow drain"
    );
    assert_eq!(lines(&aborted), ["slow"]);
}

#[test]
fn a_late_registration_drains_at_once() {
    let shutdown = Shutdown::new();
    let log = Log::default();
    assert!(joins_within(shutdown.begin(recording(&log)), WAIT));
    let (tx, rx) = mpsc::channel();
    shutdown.register(hook("late", move || tx.send(()).unwrap()));
    rx.recv_timeout(WAIT).expect("late drain ran");
    wait_until("late to finish", || lines(&log).len() == 2);
    assert_eq!(lines(&log), ["waiting on: late", "finished: late"]);
}

static EXIT_CODE: AtomicUsize = AtomicUsize::new(0);

fn fake_exit(code: i32) -> ! {
    EXIT_CODE.store(code as usize, Ordering::SeqCst);
    panic!("exit {code}");
}

#[test]
fn second_signal_aborts_then_exits_130() {
    let shutdown = Arc::new(Shutdown::new());
    let aborts = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&aborts);
    shutdown.register(ShutdownHook {
        name: "server",
        drain: Box::new(|| {}),
        abort: Box::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }),
    });
    let firsts = Arc::new(AtomicUsize::new(0));
    let first = Arc::clone(&firsts);
    let on_first = Box::new(move || {
        first.fetch_add(1, Ordering::SeqCst);
    });
    let handler = SignalHandler::new(Arc::clone(&shutdown), on_first, fake_exit);
    handler.on_signal();
    assert_eq!(firsts.load(Ordering::SeqCst), 1);
    assert_eq!(aborts.load(Ordering::SeqCst), 0);
    let second = panic::catch_unwind(AssertUnwindSafe(|| handler.on_signal()));
    assert!(second.is_err(), "the second signal exits");
    assert_eq!(EXIT_CODE.load(Ordering::SeqCst), 130);
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(firsts.load(Ordering::SeqCst), 1);
}
