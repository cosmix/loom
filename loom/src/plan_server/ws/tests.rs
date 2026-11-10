//! Hub routing and the connection loop, over real loopback sockets.

use std::io;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

use super::{run_connection, PlanHub, MAX_INBOUND_BYTES};
use crate::plan_server::ids::{BrowserId, ConnCtx, ConnId, PublicId};
use crate::plan_server::wire::message::{ClientMessage, PlanMessage};
use crate::plan_server::wire::server::Refused;
use crate::process::sandbox_probe::{loopback_bindable, skip_unless};

const WAIT: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq)]
enum Event {
    Message(ClientMessage),
    Bad(String),
}

struct Served {
    client: WebSocket<TcpStream>,
    events: Receiver<Event>,
    done: Receiver<io::Result<()>>,
}

fn browser() -> BrowserId {
    BrowserId::mint().expect("mint a browser id")
}

fn conn(id: u64, browser: &BrowserId) -> ConnCtx {
    ConnCtx {
        id: ConnId(id),
        browser: browser.clone(),
        public_id: PublicId::mint(),
    }
}

fn refused(command: &str) -> PlanMessage {
    PlanMessage::Refused(Refused {
        command: command.to_owned(),
        reason: "test".to_owned(),
    })
}

fn skip(test_name: &str) -> bool {
    skip_unless(
        loopback_bindable(),
        test_name,
        "loopback TCP is unavailable",
    )
}

/// Serve `conn` on a fresh loopback listener and connect a client to it.
fn serve(hub: &Arc<PlanHub>, conn: ConnCtx) -> Served {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("listener address").port();
    let (event_tx, events) = mpsc::channel();
    let (done_tx, done) = mpsc::channel();
    let hub = Arc::clone(hub);
    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept the client");
        let running = AtomicBool::new(true);
        let message_tx = event_tx.clone();
        let on_message = move |_: &ConnCtx, msg: ClientMessage| {
            let _ = message_tx.send(Event::Message(msg));
        };
        let on_bad_frame = move |_: &ConnCtx, reason: &str| {
            let _ = event_tx.send(Event::Bad(reason.to_owned()));
        };
        let result = run_connection(
            stream,
            conn,
            &hub,
            MAX_INBOUND_BYTES,
            &running,
            &on_message,
            &on_bad_frame,
        );
        let _ = done_tx.send(result);
    });
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(WAIT))
        .expect("client read timeout");
    stream
        .set_write_timeout(Some(WAIT))
        .expect("client write timeout");
    let request = format!("ws://127.0.0.1:{port}/ws")
        .into_client_request()
        .expect("client request");
    let (client, _) = tungstenite::client(request, stream).expect("WebSocket handshake");
    Served {
        client,
        events,
        done,
    }
}

fn next_message(client: &mut WebSocket<TcpStream>) -> PlanMessage {
    loop {
        match client.read().expect("read a frame") {
            Message::Text(text) => {
                return serde_json::from_str(text.as_str()).expect("a planning message")
            }
            Message::Ping(_) | Message::Pong(_) => {}
            other => panic!("unexpected frame: {other:?}"),
        }
    }
}

#[test]
fn open_frames_arrive_first_and_in_hook_order() {
    if skip("open_frames_arrive_first_and_in_hook_order") {
        return;
    }
    let hub = Arc::new(PlanHub::new());
    let weak = Arc::downgrade(&hub);
    hub.on_open(Box::new(move |_| {
        // Queued for this connection already: it is registered before the hooks run.
        if let Some(hub) = weak.upgrade() {
            hub.broadcast(&refused("broadcast"));
        }
        vec![refused("one")]
    }));
    hub.on_open(Box::new(|_| {
        vec![refused("two"), PlanMessage::ServerStopping]
    }));
    let mut served = serve(&hub, conn(1, &browser()));
    assert_eq!(next_message(&mut served.client), refused("one"));
    assert_eq!(next_message(&mut served.client), refused("two"));
    assert_eq!(
        next_message(&mut served.client),
        PlanMessage::ServerStopping
    );
    assert_eq!(next_message(&mut served.client), refused("broadcast"));
}

#[test]
fn send_to_browser_reaches_its_tabs_and_send_to_others_every_other_tab() {
    if skip("send_to_browser_reaches_its_tabs_and_send_to_others_every_other_tab") {
        return;
    }
    let hub = Arc::new(PlanHub::new());
    hub.on_open(Box::new(|_| vec![refused("hello")]));
    let (a, b) = (browser(), browser());
    let mut tabs: Vec<Served> = [(1, &a), (2, &a), (3, &b)]
        .into_iter()
        .map(|(id, owner)| serve(&hub, conn(id, owner)))
        .collect();
    for tab in &mut tabs {
        assert_eq!(next_message(&mut tab.client), refused("hello"));
    }
    assert_eq!(hub.connections_of(&a), 2);
    assert_eq!(hub.connections_of(&b), 1);
    hub.send_to_browser(&a, &refused("for a"));
    hub.send_to_others(&a, &refused("for all but a"));
    hub.send_to_holder(None, &refused("no holder"));
    hub.broadcast(&refused("marker"));
    for tab in &mut tabs[..2] {
        assert_eq!(next_message(&mut tab.client), refused("for a"));
        assert_eq!(next_message(&mut tab.client), refused("marker"));
    }
    assert_eq!(next_message(&mut tabs[2].client), refused("for all but a"));
    assert_eq!(next_message(&mut tabs[2].client), refused("marker"));
}

#[test]
fn an_oversized_frame_closes_the_connection() {
    if skip("an_oversized_frame_closes_the_connection") {
        return;
    }
    let hub = Arc::new(PlanHub::new());
    let closes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&closes);
    hub.on_close(Box::new(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    }));
    let owner = browser();
    let mut served = serve(&hub, conn(1, &owner));
    let _ = served.client.send(Message::text("x".repeat(300 * 1024)));
    let result = served.done.recv_timeout(WAIT).expect("the loop ended");
    assert!(result.is_ok(), "{result:?}");
    loop {
        match served.client.read() {
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }
    assert_eq!(hub.connections_of(&owner), 0);
    assert_eq!(closes.load(Ordering::SeqCst), 1);
    assert!(served.events.try_recv().is_err(), "nothing was dispatched");
}

#[test]
fn a_bad_frame_is_reported_and_the_connection_stays_open() {
    if skip("a_bad_frame_is_reported_and_the_connection_stays_open") {
        return;
    }
    let hub = Arc::new(PlanHub::new());
    let mut served = serve(&hub, conn(1, &browser()));
    let next_event = |served: &Served| served.events.recv_timeout(WAIT).expect("an event");
    served
        .client
        .send(Message::text("not json"))
        .expect("send text");
    assert!(matches!(next_event(&served), Event::Bad(reason) if !reason.is_empty()));
    served
        .client
        .send(Message::binary(vec![1, 2, 3]))
        .expect("send binary");
    assert!(matches!(next_event(&served), Event::Bad(_)));
    let reading = r#"{"type":"reading","path":null}"#;
    served
        .client
        .send(Message::text(reading))
        .expect("send reading");
    assert_eq!(
        next_event(&served),
        Event::Message(ClientMessage::Reading { path: None })
    );
}

#[test]
fn a_full_queue_drops_only_the_slow_connection() {
    let hub = PlanHub::new();
    let (slow, fast) = (conn(1, &browser()), conn(2, &browser()));
    let slow_rx = hub.register(&slow);
    let fast_rx = hub.register(&fast);
    for n in 0..=64 {
        hub.broadcast(&refused(&n.to_string()));
        while fast_rx.try_recv().is_ok() {}
    }
    assert_eq!(hub.connections_of(&slow.browser), 0);
    assert_eq!(hub.connections_of(&fast.browser), 1);
    assert_eq!(slow_rx.try_iter().count(), 64);
    assert_eq!(slow_rx.try_recv(), Err(TryRecvError::Disconnected));
    hub.broadcast(&refused("after"));
    assert_eq!(
        fast_rx
            .try_recv()
            .expect("the fast connection still receives")
            .as_str(),
        refused("after").to_frame()
    );
}
