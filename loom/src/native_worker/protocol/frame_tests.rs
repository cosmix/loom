use super::{classify, read_frame, write_frame, FrameError, Incoming, RpcError};
use serde_json::{json, Value};
use std::io::Cursor;

#[test]
fn read_frame_parses_one_line() {
    let mut reader = Cursor::new(b"{\"id\":1}\n");
    assert_eq!(read_frame(&mut reader, 100).unwrap(), json!({"id": 1}));
}

#[test]
fn read_frame_reports_eof_on_empty_input() {
    assert!(matches!(
        read_frame(&mut Cursor::new(b""), 10),
        Err(FrameError::Eof)
    ));
}

#[test]
fn read_frame_oversized_then_next_frame_is_intact() {
    let input = format!("{}\n{{\"method\":\"initialized\"}}\n", "x".repeat(3000));
    let mut reader = Cursor::new(input);
    assert!(matches!(
        read_frame(&mut reader, 1024),
        Err(FrameError::Oversized)
    ));
    assert_eq!(
        read_frame(&mut reader, 1024).unwrap(),
        json!({"method": "initialized"})
    );
}

#[test]
fn read_frame_accepts_a_line_of_exactly_max_bytes() {
    assert_eq!(
        read_frame(&mut Cursor::new(b"true\n"), 4).unwrap(),
        json!(true)
    );
}

#[test]
fn read_frame_rejects_a_line_one_byte_over_max() {
    assert!(matches!(
        read_frame(&mut Cursor::new(b"false\n"), 4),
        Err(FrameError::Oversized)
    ));
}

#[test]
fn read_frame_malformed_json_is_malformed_and_next_line_still_reads() {
    let mut reader = Cursor::new(b"{oops}\nnull\n");
    assert!(matches!(
        read_frame(&mut reader, 20),
        Err(FrameError::Malformed(_))
    ));
    assert_eq!(read_frame(&mut reader, 20).unwrap(), Value::Null);
}

#[test]
fn read_frame_blank_line_is_malformed() {
    assert!(matches!(
        read_frame(&mut Cursor::new(b"\n"), 20),
        Err(FrameError::Malformed(message)) if message == "empty line"
    ));
}

#[test]
fn write_frame_emits_one_line_ending_in_newline() {
    let mut writer = Cursor::new(Vec::new());
    write_frame(&mut writer, &json!({"text": "a\nb"})).unwrap();
    assert_eq!(writer.into_inner(), b"{\"text\":\"a\\nb\"}\n");
}

#[test]
fn classify_response_ok() {
    assert_eq!(
        classify(json!({"id": 1, "result": {"ok": true}})).unwrap(),
        Incoming::Response {
            id: 1,
            result: Ok(json!({"ok": true})),
        }
    );
}

#[test]
fn classify_response_error() {
    assert_eq!(
        classify(json!({"id": 2, "error": {"code": -1, "message": "bad"}})).unwrap(),
        Incoming::Response {
            id: 2,
            result: Err(RpcError {
                code: -1,
                message: "bad".into(),
            }),
        }
    );
}

#[test]
fn classify_response_with_string_id_is_malformed() {
    assert!(matches!(
        classify(json!({"id": "1", "result": null})),
        Err(FrameError::Malformed(_))
    ));
}

#[test]
fn classify_response_without_result_or_error_is_malformed() {
    assert!(matches!(
        classify(json!({"id": 1})),
        Err(FrameError::Malformed(_))
    ));
}

#[test]
fn classify_notification_without_params_gets_null() {
    assert_eq!(
        classify(json!({"method": "ready"})).unwrap(),
        Incoming::Notification {
            method: "ready".into(),
            params: Value::Null,
        }
    );
}

#[test]
fn classify_server_request_keeps_string_id_verbatim() {
    assert_eq!(
        classify(json!({"id": "abc", "method": "approve", "params": {}})).unwrap(),
        Incoming::ServerRequest {
            id: json!("abc"),
            method: "approve".into(),
            params: json!({}),
        }
    );
}

#[test]
fn classify_server_request_keeps_numeric_id_verbatim() {
    assert_eq!(
        classify(json!({"id": -3, "method": "approve"})).unwrap(),
        Incoming::ServerRequest {
            id: json!(-3),
            method: "approve".into(),
            params: Value::Null,
        }
    );
}

#[test]
fn classify_non_object_is_malformed() {
    assert!(matches!(
        classify(json!([])),
        Err(FrameError::Malformed(message)) if message == "frame is not an object"
    ));
}

#[test]
fn classify_method_not_a_string_is_malformed() {
    assert!(matches!(
        classify(json!({"method": null, "id": 1})),
        Err(FrameError::Malformed(_))
    ));
}

#[test]
fn classify_response_rejects_invalid_error_objects() {
    for error in [
        json!(null),
        json!({}),
        json!({"code": "-1", "message": "bad"}),
        json!({"code": -1, "message": 1}),
    ] {
        assert!(matches!(
            classify(json!({"id": 1, "error": error})),
            Err(FrameError::Malformed(_))
        ));
    }
}
