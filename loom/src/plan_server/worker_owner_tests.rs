use super::*;
use crate::plan_server::lock::{self, Acquired};

const WAIT: Duration = Duration::from_secs(10);

fn context(root: &std::path::Path) -> Arc<PlanContext> {
    Arc::new(
        PlanContext::new(
            root.to_path_buf(),
            crate::plan_server::prereq::ReadOnly::from_problems(Vec::new()),
            crate::plan_server::context::BindInfo::new("127.0.0.1:0".parse().unwrap(), "token"),
            crate::planning::watch::OwnWrites::default(),
        )
        .unwrap(),
    )
}

fn ownership(path: &std::path::Path) -> Arc<Ownership> {
    let Acquired::Held(lock) = lock::acquire(path).unwrap() else {
        panic!("new singleton should be free")
    };
    let owner = Arc::new(Ownership::default());
    owner.retain(lock);
    owner
}

fn wait_for_singleton(path: &std::path::Path) {
    let started = std::time::Instant::now();
    while matches!(lock::acquire(path).unwrap(), Acquired::Running(_)) {
        assert!(
            started.elapsed() < WAIT,
            "cleanup must release the singleton"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn failed_start_retains_registered_worker_listener_and_singleton() {
    if crate::plan_server::tests::support::skip(
        "failed_start_retains_registered_worker_listener_and_singleton",
    ) {
        return;
    }
    let temp = crate::plan_server::tests::support::project();
    let path = temp.path().join("server.lock");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let local = listener.local_addr().unwrap();
    let running = AtomicBool::new(true);
    let stop = AtomicBool::new(false);
    let (entered_tx, entered) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let started = Workers::launch(
        listener,
        ownership(&path),
        context(temp.path()),
        &running,
        &stop,
        |_, registry| {
            registry.background(thread::spawn(move || {
                entered_tx.send(()).unwrap();
                wait.recv_timeout(WAIT).unwrap();
            }));
            entered.recv_timeout(WAIT).unwrap();
            Err(anyhow!("injected tick spawn failure"))
        },
    );
    let Err(error) = started else {
        panic!("startup must fail")
    };
    assert!(error.to_string().contains("injected tick"));
    drop(error);
    assert!(!running.load(Ordering::SeqCst));
    assert!(stop.load(Ordering::SeqCst));
    assert!(TcpListener::bind(local).is_err());
    assert!(matches!(
        lock::acquire(&path).unwrap(),
        Acquired::Running(_)
    ));
    release.send(()).unwrap();
    wait_for_singleton(&path);
    TcpListener::bind(local).expect("failed-start cleanup releases the listener");
}

#[test]
fn pending_cleanup_keeps_the_eventual_failure_observable() {
    if crate::plan_server::tests::support::skip(
        "pending_cleanup_keeps_the_eventual_failure_observable",
    ) {
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let temp = crate::plan_server::tests::support::project();
    let (release, wait) = mpsc::channel();
    let workers = Workers::launch(
        listener,
        Arc::new(Ownership::default()),
        context(temp.path()),
        &AtomicBool::new(false),
        &AtomicBool::new(true),
        |_, registry| {
            registry.accept(thread::spawn(|| Err(anyhow!("accept loop failed"))));
            registry.background(thread::spawn(move || wait.recv_timeout(WAIT).unwrap()));
            Ok(())
        },
    )
    .unwrap();
    let error = workers.stop().unwrap_err();
    let pending = error.downcast_ref::<ShutdownPending>().unwrap();
    assert!(!pending.is_complete());
    release.send(()).unwrap();
    assert!(pending.wait(WAIT));
    assert!(pending
        .completion_error()
        .unwrap()
        .contains("accept loop failed"));
}

struct Descendant {
    create: mpsc::Sender<()>,
    registered: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
}

fn register_parent(ctx: &Arc<PlanContext>) -> Descendant {
    let (create, start) = mpsc::channel();
    let (registered_tx, registered) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let parent_ctx = Arc::clone(ctx);
    ctx.own_worker(
        "parent",
        thread::spawn(move || {
            start.recv_timeout(WAIT).unwrap();
            parent_ctx.own_worker(
                "descendant",
                thread::spawn(move || wait.recv_timeout(WAIT).unwrap()),
                None,
            );
            registered_tx.send(()).unwrap();
        }),
        None,
    );
    Descendant {
        create,
        registered,
        release,
    }
}

#[test]
fn descendants_registered_during_cleanup_keep_ownership() {
    if crate::plan_server::tests::support::skip(
        "descendants_registered_during_cleanup_keep_ownership",
    ) {
        return;
    }
    let temp = crate::plan_server::tests::support::project();
    let path = temp.path().join("server.lock");
    let ctx = context(temp.path());
    let descendant = register_parent(&ctx);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let local = listener.local_addr().unwrap();
    let workers = Workers::launch(
        listener,
        ownership(&path),
        ctx,
        &AtomicBool::new(false),
        &AtomicBool::new(true),
        |_, _| Ok(()),
    )
    .unwrap();
    let error = workers.stop().unwrap_err();
    let pending = error.downcast_ref::<ShutdownPending>().unwrap();
    descendant.create.send(()).unwrap();
    descendant.registered.recv_timeout(WAIT).unwrap();
    assert!(!pending.is_complete());
    assert!(TcpListener::bind(local).is_err());
    assert!(matches!(
        lock::acquire(&path).unwrap(),
        Acquired::Running(_)
    ));
    descendant.release.send(()).unwrap();
    assert!(pending.wait(WAIT));
    TcpListener::bind(local).expect("all descendant owners released");
    assert!(matches!(lock::acquire(&path).unwrap(), Acquired::Held(_)));
}

#[test]
fn failed_hook_spawn_cannot_detach_its_registered_job() {
    if crate::plan_server::tests::support::skip(
        "failed_hook_spawn_cannot_detach_its_registered_job",
    ) {
        return;
    }
    let temp = crate::plan_server::tests::support::project();
    let ctx = context(temp.path());
    let (release, wait) = mpsc::channel();
    let (finished, completion) = mpsc::channel();
    ctx.own_worker(
        "run start",
        thread::spawn(move || {
            wait.recv_timeout(WAIT).unwrap();
            let _ = finished.send(());
        }),
        None,
    );
    ctx.shutdown()
        .register(crate::plan_server::shutdown::ShutdownHook {
            name: "run start",
            drain: Box::new(move || {
                let _ = completion.recv();
            }),
            abort: Box::new(|| {}),
        });
    ctx.shutdown().begin_with_failed_spawns();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let local = listener.local_addr().unwrap();
    let workers = Workers::launch(
        listener,
        Arc::new(Ownership::default()),
        ctx,
        &AtomicBool::new(false),
        &AtomicBool::new(true),
        |_, _| Ok(()),
    )
    .unwrap();
    let error = workers.stop().unwrap_err();
    let pending = error.downcast_ref::<ShutdownPending>().unwrap();
    assert!(!pending.is_complete());
    assert!(TcpListener::bind(local).is_err());
    release.send(()).unwrap();
    assert!(pending.wait(WAIT));
    TcpListener::bind(local).expect("job stays owned despite hook spawn failure");
}
