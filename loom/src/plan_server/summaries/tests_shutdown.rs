//! The summaries shutdown hook: the drain stops the jobs and waits, the abort ends that wait.

use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::sync::{Condvar, Mutex};

use super::scheduler::SessionSpawner;
use super::*;
use crate::plan_server::context::BindInfo;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::tests::support::project;
use crate::planning::watch::OwnWrites;

/// Blocks in `force_stopped` while its wait is not aborted and records each call's grace.
#[derive(Default)]
struct Blocking {
    stop_alls: AtomicUsize,
    graces: Mutex<Vec<Duration>>,
    entered: Condvar,
}

impl SessionSpawner for Blocking {
    fn start_summary(&self, _ids: &[String]) -> anyhow::Result<String> {
        anyhow::bail!("no start in this test")
    }

    fn stop(&self, _session: &str, _release_claims: bool) {}

    fn stop_all(&self) {
        self.stop_alls.fetch_add(1, Ordering::SeqCst);
    }

    fn force_stopped(&self, grace: Duration, aborted: &AtomicBool) {
        self.graces.lock().unwrap().push(grace);
        self.entered.notify_all();
        while !aborted.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
        }
    }
}

fn context(root: &Path) -> Arc<PlanContext> {
    let local = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
    let bind = BindInfo::new(local, "test");
    let read_only = ReadOnly::from_problems(Vec::new());
    let ctx = PlanContext::new(root.to_path_buf(), read_only, bind, OwnWrites::default());
    Arc::new(ctx.expect("context"))
}

#[test]
fn the_drain_waits_on_the_jobs_and_the_abort_ends_the_wait() {
    let root = project();
    let ctx = context(root.path());
    let spawner = Arc::new(Blocking::default());
    let rebuilds = Rebuilds::with_spawner(&ctx, spawner.clone());
    register_shutdown(&ctx, &rebuilds, Arc::new(AtomicBool::new(false)));

    let drained = ctx.shutdown().begin(|_| {});
    let graces = spawner.graces.lock().unwrap();
    let graces = spawner
        .entered
        .wait_while(graces, |graces| graces.is_empty())
        .unwrap();
    assert_eq!(
        *graces,
        [FORCE_AFTER],
        "the drain gives the jobs their grace"
    );
    drop(graces);
    assert_eq!(spawner.stop_alls.load(Ordering::SeqCst), 1);

    ctx.shutdown().abort_all();
    drained.join().expect("the drain returned on the abort");
    assert_eq!(
        *spawner.graces.lock().unwrap(),
        [FORCE_AFTER, Duration::ZERO],
        "the abort forces at once"
    );
}

/// A quota poll in flight cannot be interrupted, so the server's stop must not wait on it.
#[test]
fn a_quota_poll_in_flight_does_not_hold_up_stop() {
    use crate::plan_server::lock::Ownership;
    use crate::plan_server::worker_owner::Workers;
    use std::sync::mpsc;

    if crate::plan_server::tests::support::skip("a_quota_poll_in_flight_does_not_hold_up_stop") {
        return;
    }
    let root = project();
    let ctx = context(root.path());
    let (release, polling) = mpsc::channel::<()>();
    let stop = start_quota_poller(&ctx, move |_, _| {
        thread::spawn(move || {
            let _ = polling.recv_timeout(Duration::from_secs(30));
        })
    });
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let workers = Workers::launch(
        listener,
        Arc::new(Ownership::default()),
        ctx,
        &AtomicBool::new(false),
        &AtomicBool::new(true),
        |_, _| Ok(()),
    )
    .unwrap();
    let stopped = workers.stop();
    drop(release);
    stopped.expect("stop returns while the poll is still in flight");
    assert!(stop.load(Ordering::SeqCst), "the poller was told to stop");
}
