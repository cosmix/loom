use super::*;
use serde_json::json;

const FIRST: &str = r#"{"timestamp":"2026-10-14T08:00:36.048Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":21255,"cached_input_tokens":12160,"cache_write_input_tokens":0,"output_tokens":110,"reasoning_output_tokens":0,"total_tokens":21365},"last_token_usage":{"input_tokens":21255,"cached_input_tokens":12160,"cache_write_input_tokens":0,"output_tokens":110,"reasoning_output_tokens":0,"total_tokens":21365},"model_context_window":475000}}}"#;
const SECOND: &str = r#"{"timestamp":"2026-10-14T08:00:46.390Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":43274,"cached_input_tokens":33280,"cache_write_input_tokens":0,"output_tokens":206,"reasoning_output_tokens":0,"total_tokens":43480},"last_token_usage":{"input_tokens":22019,"cached_input_tokens":21120,"cache_write_input_tokens":0,"output_tokens":96,"reasoning_output_tokens":0,"total_tokens":22115},"model_context_window":475000}}}"#;

const FIRST_TOTAL: CodexTotals = CodexTotals {
    input: 21255,
    cached_input: 12160,
    cache_write: 0,
    output: 110,
};

#[test]
fn parses_cumulative_samples_and_delta() {
    let first_timestamp = DateTime::parse_from_rfc3339("2026-10-14T08:00:36Z")
        .unwrap()
        .timestamp();
    assert_eq!(
        parse_line(FIRST),
        CodexLine::TokenCount {
            timestamp: first_timestamp,
            total: FIRST_TOTAL,
        }
    );
    let second_total = CodexTotals {
        input: 43274,
        cached_input: 33280,
        cache_write: 0,
        output: 206,
    };
    assert_eq!(
        parse_line(SECOND),
        CodexLine::TokenCount {
            timestamp: first_timestamp + 10,
            total: second_total,
        }
    );
    assert_eq!(
        second_total.delta_since(&FIRST_TOTAL),
        Some(CodexTotals {
            input: 22019,
            cached_input: 21120,
            cache_write: 0,
            output: 96,
        })
    );
}

#[test]
fn converts_sample_totals_to_disjoint_usage() {
    assert_eq!(
        FIRST_TOTAL.to_usage(),
        TokenUsage {
            input: 9095,
            cache_write: 0,
            cache_read: 12160,
            output: 110,
        }
    );
}

#[test]
fn ignores_null_info_ping() {
    assert_eq!(
        parse_line(r#"{"type":"event_msg","payload":{"type":"token_count","info":null}}"#),
        CodexLine::Other
    );
}

#[test]
fn parses_model() {
    assert_eq!(
        parse_line(r#"{"type":"turn_context","payload":{"model":"gpt-6.1-sol"}}"#),
        CodexLine::Model("gpt-6.1-sol".into())
    );
}

#[test]
fn parses_session_meta() {
    let line = r#"{"timestamp":"2026-10-14T08:00:28.716Z","type":"session_meta","payload":{"id":"01a1396b-2aa4-7d93-9e18-d54685ff45df","cwd":"<cwd>","originator":"codex-tui"}}"#;
    assert_eq!(
        parse_line(line),
        CodexLine::SessionMeta {
            thread_id: "01a1396b-2aa4-7d93-9e18-d54685ff45df".into(),
        }
    );
}

#[test]
fn classifies_torn_usage_as_unreadable() {
    assert_eq!(
        parse_line(r#"{"type":"event_msg","payload":{"type":"token_count","info":"#),
        CodexLine::Unreadable
    );
    assert_eq!(parse_line("not json"), CodexLine::Other);
}

#[test]
fn ignores_valid_user_message_mentioning_token_count() {
    let line =
        r#"{"type":"event_msg","payload":{"type":"user_message","message":"explain token_count"}}"#;
    assert_eq!(parse_line(line), CodexLine::Other);
}

#[test]
fn rejects_each_decreasing_field() {
    let previous = CodexTotals {
        input: 10,
        cached_input: 10,
        cache_write: 10,
        output: 10,
    };
    for current in [
        CodexTotals {
            input: 9,
            ..previous
        },
        CodexTotals {
            cached_input: 9,
            ..previous
        },
        CodexTotals {
            cache_write: 9,
            ..previous
        },
        CodexTotals {
            output: 9,
            ..previous
        },
    ] {
        assert_eq!(current.delta_since(&previous), None, "{current:?}");
    }
    assert_eq!(
        previous.delta_since(&previous),
        Some(CodexTotals::default())
    );
}

#[test]
fn defaults_absent_cache_fields_to_zero() {
    let line = json!({
        "timestamp": "1970-01-01T02:00:01+02:00",
        "type": "event_msg",
        "payload": {"type": "token_count", "info": {
            "total_token_usage": {"input_tokens": 12, "output_tokens": 3}
        }}
    });
    assert_eq!(
        parse_line(&line.to_string()),
        CodexLine::TokenCount {
            timestamp: 1,
            total: CodexTotals {
                input: 12,
                output: 3,
                ..CodexTotals::default()
            },
        }
    );
}

#[test]
fn rejects_missing_or_invalid_timestamp() {
    for timestamp in [Value::Null, json!("bad timestamp"), json!(123)] {
        let mut line: Value = serde_json::from_str(FIRST).unwrap();
        line["timestamp"] = timestamp;
        assert_eq!(parse_line(&line.to_string()), CodexLine::Unreadable);
    }
    let mut line: Value = serde_json::from_str(FIRST).unwrap();
    line.as_object_mut().unwrap().remove("timestamp");
    assert_eq!(parse_line(&line.to_string()), CodexLine::Unreadable);
}

#[test]
fn rejects_missing_or_nonnumeric_required_totals() {
    for usage in [
        Value::Null,
        json!(12),
        json!({}),
        json!({"input_tokens": 12}),
        json!({"output_tokens": 3}),
        json!({"input_tokens": "12", "output_tokens": 3}),
        json!({"input_tokens": 12, "output_tokens": -1}),
        json!({"input_tokens": 1.5, "output_tokens": 3}),
    ] {
        let mut line: Value = serde_json::from_str(FIRST).unwrap();
        line["payload"]["info"]["total_token_usage"] = usage;
        assert_eq!(parse_line(&line.to_string()), CodexLine::Unreadable);
    }
}

#[test]
fn conversion_subtracts_cache_write_and_saturates_without_overflow() {
    let total = CodexTotals {
        input: 10,
        cached_input: 3,
        cache_write: 2,
        output: 4,
    };
    assert_eq!(
        total.to_usage(),
        TokenUsage {
            input: 5,
            cache_write: 2,
            cache_read: 3,
            output: 4,
        }
    );
    let oversized = CodexTotals {
        input: u64::MAX,
        cached_input: u64::MAX,
        cache_write: u64::MAX,
        output: 4,
    };
    assert_eq!(oversized.to_usage().input, 0);
}
