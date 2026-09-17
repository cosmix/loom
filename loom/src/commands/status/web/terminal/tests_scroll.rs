use super::protocol::{parse_client_frame, scroll_input, ClientFrame};
use tungstenite::Message;

#[test]
fn scroll_frames_accept_only_bounded_page_counts() {
    for pages in [-5, -1, 1, 5] {
        let message = Message::text(format!(r#"{{"scroll":{{"pages":{pages}}}}}"#));
        assert_eq!(
            parse_client_frame(&message),
            Some(ClientFrame::Scroll(pages))
        );
    }
    for text in [
        r#"{"scroll":{"pages":0}}"#,
        r#"{"scroll":{"pages":6}}"#,
        r#"{"scroll":{"pages":-6}}"#,
        r#"{"scroll":{"pages":1.5}}"#,
        r#"{"scroll":{"pages":"\u001b[A"}}"#,
        r#"{"scroll":{"pages":1,"input":"run"}}"#,
        r#"{"scroll":{"pages":1},"input":"run"}"#,
        r#"{"scroll":{"pages":1},"resize":{"cols":80,"rows":24}}"#,
    ] {
        assert_eq!(parse_client_frame(&Message::text(text)), None, "{text}");
    }
}

#[test]
fn scroll_input_is_only_page_up_or_page_down() {
    assert_eq!(scroll_input(-2), b"\x1b[5~\x1b[5~");
    assert_eq!(scroll_input(2), b"\x1b[6~\x1b[6~");
    assert!(scroll_input(0).is_empty());
    assert_eq!(scroll_input(i8::MIN), b"\x1b[5~".repeat(5));
}
