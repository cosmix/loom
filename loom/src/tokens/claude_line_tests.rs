use super::{parse_line, ClaudeLine};
use crate::tokens::model::TokenUsage;
use serde_json::{json, Value};

#[test]
fn test_parse_line_captured_sample_returns_usage() {
    let line = r#"{"type":"assistant","timestamp":"2026-09-30T21:09:35.379Z","isSidechain":true,"agentId":"a04baa183f888b773","sessionId":"01ed9f4c-608e-4e2c-8edf-05d26adcd4d6","message":{"id":"msg_011CfaJtKtxUSKsEhtf2ECes","model":"claude-sonnet-5-5","usage":{"input_tokens":2,"cache_creation_input_tokens":14477,"cache_read_input_tokens":4542,"cache_creation":{"ephemeral_5m_input_tokens":14477,"ephemeral_1h_input_tokens":0},"output_tokens":20,"service_tier":"standard","inference_geo":"not_available"}}}"#;

    assert_eq!(
        parse_line(line),
        ClaudeLine::Usage {
            message_id: Some("msg_011CfaJtKtxUSKsEhtf2ECes".into()),
            timestamp: 1_790_802_575,
            model: Some("claude-sonnet-5-5".into()),
            effort: None,
            usage: TokenUsage {
                input: 2,
                cache_write: 14_477,
                cache_read: 4_542,
                output: 20,
            },
        }
    );
}

fn assistant_usage() -> Value {
    json!({
        "type": "assistant",
        "timestamp": "2026-09-30T21:09:35.379Z",
        "message": {"usage": {"input_tokens": 2, "output_tokens": 20}}
    })
}

#[test]
fn test_parse_line_missing_optional_fields_returns_defaults() {
    let line = assistant_usage().to_string();

    assert_eq!(
        parse_line(&line),
        ClaudeLine::Usage {
            message_id: None,
            timestamp: 1_790_802_575,
            model: None,
            effort: None,
            usage: TokenUsage {
                input: 2,
                cache_write: 0,
                cache_read: 0,
                output: 20,
            },
        }
    );
}

#[test]
fn test_parse_line_invalid_required_counts_returns_unreadable() {
    for field in ["input_tokens", "output_tokens"] {
        for invalid in [json!("20"), json!(-1), json!(1.5), Value::Null, json!(true)] {
            let mut line = assistant_usage();
            line["message"]["usage"][field] = invalid;

            assert_eq!(parse_line(&line.to_string()), ClaudeLine::Unreadable);
        }
        let mut line = assistant_usage();
        line["message"]["usage"]
            .as_object_mut()
            .unwrap()
            .remove(field);

        assert!(matches!(
            parse_line(&line.to_string()),
            ClaudeLine::Usage { .. }
        ));
    }
}

#[test]
fn test_parse_line_invalid_cache_count_returns_unreadable() {
    for field in ["cache_creation_input_tokens", "cache_read_input_tokens"] {
        let mut line = assistant_usage();
        line["message"]["usage"][field] = json!("20");

        assert_eq!(parse_line(&line.to_string()), ClaudeLine::Unreadable);
    }
}

#[test]
fn test_parse_line_zero_and_maximum_counts_are_preserved() {
    let mut line = assistant_usage();
    line["message"]["usage"] = json!({"input_tokens": 0, "output_tokens": u64::MAX});

    assert_eq!(
        parse_line(&line.to_string()),
        ClaudeLine::Usage {
            message_id: None,
            timestamp: 1_790_802_575,
            model: None,
            effort: None,
            usage: TokenUsage {
                input: 0,
                cache_write: 0,
                cache_read: 0,
                output: u64::MAX,
            },
        }
    );
}

#[test]
fn test_parse_line_missing_or_invalid_timestamp_returns_unreadable() {
    for invalid in [Value::Null, json!(123), json!("invalid")] {
        let mut line = assistant_usage();
        line["timestamp"] = invalid;

        assert_eq!(parse_line(&line.to_string()), ClaudeLine::Unreadable);
    }
    let mut line = assistant_usage();
    line.as_object_mut().unwrap().remove("timestamp");

    assert_eq!(parse_line(&line.to_string()), ClaudeLine::Unreadable);
}

#[test]
fn test_parse_line_timestamp_offset_is_converted_to_unix_seconds() {
    let mut line = assistant_usage();
    line["timestamp"] = json!("2026-10-01T00:09:35.379+03:00");

    assert!(matches!(
        parse_line(&line.to_string()),
        ClaudeLine::Usage {
            timestamp: 1_790_802_575,
            ..
        }
    ));
}

#[test]
fn test_parse_line_torn_usage_json_returns_unreadable() {
    assert_eq!(
        parse_line(r#"{"type":"assistant","message":{"usage":{"input_tokens":2"#),
        ClaudeLine::Unreadable
    );
}

#[test]
fn test_parse_line_non_usage_records_return_other() {
    for line in [
        r#"{"type":"user","message":{"content":"usage"}}"#,
        r#"{"type":"user","message":{"usage":{"input_tokens":2}}}"#,
        r#"{"type":"assistant","message":{"content":"usage"}}"#,
        r#"{"type":"assistant","message":{"usage":null}}"#,
        r#"{"type":"assistant","message":{"usage":[]}}"#,
        r#"{"type":"tool_result","usage":{}}"#,
        r#"{"type":"summary","summary":"usage"}"#,
        r#"{"type":"assistant""#,
        r#"{"usage":"mentioned at top level"}"#,
        r#""usage""#,
        "null",
        "[]",
        "",
    ] {
        assert_eq!(parse_line(line), ClaudeLine::Other, "{line}");
    }
}

#[test]
fn test_parse_line_null_cache_counts_are_zero() {
    let mut line = assistant_usage();
    line["message"]["usage"]["cache_creation_input_tokens"] = Value::Null;
    line["message"]["usage"]["cache_read_input_tokens"] = Value::Null;

    assert_eq!(
        parse_line(&line.to_string()),
        ClaudeLine::Usage {
            message_id: None,
            timestamp: 1_790_802_575,
            model: None,
            effort: None,
            usage: TokenUsage {
                input: 2,
                cache_write: 0,
                cache_read: 0,
                output: 20,
            },
        }
    );
}

fn effort_of(extra: Value) -> Option<String> {
    let mut line = assistant_usage();
    for (key, value) in extra.as_object().unwrap() {
        line[key] = value.clone();
    }
    match parse_line(&line.to_string()) {
        ClaudeLine::Usage { effort, .. } => effort,
        other => panic!("expected usage, got {other:?}"),
    }
}

#[test]
fn test_parse_line_per_turn_effort_wins_over_effort() {
    let both = json!({"perTurnEffort": "xhigh", "effort": "low"});
    assert_eq!(effort_of(both).as_deref(), Some("xhigh"));
}

#[test]
fn test_parse_line_effort_alone_is_read() {
    assert_eq!(
        effort_of(json!({"effort": "high"})).as_deref(),
        Some("high")
    );
}

#[test]
fn test_parse_line_without_effort_is_none() {
    assert_eq!(effort_of(json!({})), None);
}

#[test]
fn test_parse_line_numeric_per_turn_effort_is_absent() {
    assert_eq!(effort_of(json!({"perTurnEffort": 3})), None);
}

#[test]
fn test_parse_line_non_numeric_cache_counts_stay_unreadable() {
    for field in ["cache_creation_input_tokens", "cache_read_input_tokens"] {
        for invalid in [json!(-1), json!(1.5), json!(true), json!([1]), json!({})] {
            let mut line = assistant_usage();
            line["message"]["usage"][field] = invalid;

            assert_eq!(parse_line(&line.to_string()), ClaudeLine::Unreadable);
        }
    }
}
