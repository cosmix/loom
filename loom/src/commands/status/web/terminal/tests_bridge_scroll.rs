use super::{join_within, skip_bridge_test, start_bridge, wait_for_binary};
use crate::commands::status::web::terminal::protocol::Mode;
use std::time::Duration;
use tungstenite::Message;

#[test]
fn bridge_drops_input_in_view_mode() {
    if skip_bridge_test("bridge_drops_input_in_view_mode") {
        return;
    }
    let mut fixture = start_bridge("read l; printf 'echo:%s\\n' \"$l\"", Mode::View);
    fixture
        .socket
        .send(Message::binary(b"ping\n".to_vec()))
        .expect("send ignored terminal input");
    assert!(!wait_for_binary(
        &mut fixture.socket,
        b"echo:",
        Duration::from_secs(1)
    ));
    fixture.socket.close(None).expect("close bridge client");
    join_within(fixture.server, Duration::from_secs(3));
}

#[test]
fn bridge_passes_only_page_keys_from_viewer_scroll_messages() {
    if skip_bridge_test("bridge_passes_only_page_keys_from_viewer_scroll_messages") {
        return;
    }
    let mut fixture = start_bridge(
        "stty raw -echo; printf ready; dd bs=1 count=8 2>/dev/null | od -An -tx1",
        Mode::View,
    );
    assert!(wait_for_binary(
        &mut fixture.socket,
        b"ready",
        Duration::from_secs(3)
    ));
    for message in [
        Message::binary(b"ignored\n".to_vec()),
        Message::text(r#"{"scroll":{"pages":-1}}"#),
        Message::text(r#"{"scroll":{"pages":1}}"#),
    ] {
        fixture.socket.send(message).expect("send viewer message");
    }
    assert!(wait_for_binary(
        &mut fixture.socket,
        b"1b 5b 35 7e 1b 5b 36 7e",
        Duration::from_secs(3),
    ));
    let _ = fixture.socket.close(None);
    join_within(fixture.server, Duration::from_secs(3));
}
