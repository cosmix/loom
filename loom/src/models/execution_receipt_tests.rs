use chrono::{TimeZone, Utc};
use serde_json::json;

use super::*;

fn valid() -> serde_json::Value {
    json!({
        "schema_version": 1,
        "provider": "claude",
        "observed_at": "2026-09-12T20:12:13Z",
        "request_id": "request-1",
        "usage": {
            "input_tokens": 10,
            "cache_creation_input_tokens": null,
            "cache_read_input_tokens": null,
            "output_tokens": 4,
            "thinking_output_tokens": null,
            "cache_write_5m_input_tokens": null,
            "cache_write_1h_input_tokens": null
        }
    })
}

#[test]
fn decoder_preserves_unknown_optional_usage() {
    let receipt = decode(&valid().to_string()).expect("valid receipt");

    assert_eq!(receipt.provider, ReceiptProvider::Claude);
    assert_eq!(
        receipt.observed_at,
        Utc.with_ymd_and_hms(2026, 9, 12, 20, 12, 13)
            .single()
            .expect("valid fixture timestamp")
    );
    assert!(receipt.usage.cache_creation_input_tokens.is_none());
    assert!(receipt.usage.thinking_output_tokens.is_none());
}

#[test]
fn decoder_rejects_unsupported_version_separately() {
    let mut value = valid();
    value["schema_version"] = json!(2);

    let error = decode(&value.to_string()).expect_err("unsupported version");
    assert!(error.is_unsupported_version());
}

#[test]
fn decoder_rejects_missing_identity_and_timestamp() {
    for field in ["provider", "observed_at"] {
        let mut value = valid();
        value.as_object_mut().expect("object fixture").remove(field);
        assert!(decode(&value.to_string()).is_err(), "field: {field}");
    }
}

#[test]
fn decoder_rejects_negative_fractional_and_excess_thinking() {
    for invalid in [json!(-1), json!(1.5)] {
        let mut value = valid();
        value["usage"]["input_tokens"] = invalid;
        assert!(decode(&value.to_string()).is_err());
    }
    let mut value = valid();
    value["usage"]["thinking_output_tokens"] = json!(5);
    assert!(decode(&value.to_string()).is_err());
}

#[test]
fn decoder_preserves_inconsistent_cache_ttl_split_for_normalization() {
    let mut value = valid();
    value["usage"]["cache_creation_input_tokens"] = json!(10);
    value["usage"]["cache_write_5m_input_tokens"] = json!(4);
    value["usage"]["cache_write_1h_input_tokens"] = json!(5);

    let receipt = decode(&value.to_string()).expect("decoder accepts telemetry discrepancy");
    assert_eq!(receipt.usage.cache_write_5m_input_tokens, Some(4));
    assert_eq!(receipt.usage.cache_write_1h_input_tokens, Some(5));
}
