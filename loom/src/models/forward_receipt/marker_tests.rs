use super::*;

const COMPANION_START: &str =
    r#"LOOM-FORWARD-START {"v":1,"backend":"companion","job_id":"job-1"}"#;
const COMPANION_END: &str = r#"LOOM-FORWARD-END {"v":1,"backend":"companion","job_id":"job-1","outcome":"succeeded","exit_code":0}"#;
const DIRECT_START: &str =
    r#"LOOM-FORWARD-START {"v":1,"backend":"direct","thread_id":"thread-1"}"#;
const DIRECT_END: &str = r#"LOOM-FORWARD-END {"v":1,"backend":"direct","thread_id":"thread-1","outcome":"failed","exit_code":7}"#;

#[test]
fn decodes_started_and_finished_channels() {
    let started = decode_marker_channel(&format!("{COMPANION_START}\n{SEPARATOR}\nprovider"));
    assert!(matches!(
        started,
        MarkerChannel::Started(StartMarker {
            backend: ForwardBackend::Companion,
            backend_id,
        }) if backend_id == "job-1"
    ));

    let finished = decode_marker_channel(&format!("{DIRECT_START}\n{DIRECT_END}\n{SEPARATOR}"));
    assert!(matches!(
        finished,
        MarkerChannel::Finished(
            StartMarker {
                backend: ForwardBackend::Direct,
                ..
            },
            EndMarker {
                outcome: ForwardState::Failed,
                exit_code: 7,
                ..
            }
        )
    ));
}

#[test]
fn ignores_marker_spoofing_after_separator() {
    let text = format!("{COMPANION_START}\n{SEPARATOR}\n{COMPANION_END}\nprovider output");
    assert!(matches!(
        decode_marker_channel(&text),
        MarkerChannel::Started(_)
    ));
}

#[test]
fn treats_non_starting_text_as_absent() {
    assert_eq!(decode_marker_channel(""), MarkerChannel::Absent);
    let garbage = format!("provider prefix\n{COMPANION_START}\n{SEPARATOR}");
    assert_eq!(decode_marker_channel(&garbage), MarkerChannel::Absent);
}

#[test]
fn rejects_duplicate_markers_and_end_before_start() {
    let starts = format!("{COMPANION_START}\n{COMPANION_START}\n{SEPARATOR}");
    assert!(matches!(
        decode_marker_channel(&starts),
        MarkerChannel::Invalid(_)
    ));
    let ends = format!("{COMPANION_START}\n{COMPANION_END}\n{COMPANION_END}\n{SEPARATOR}");
    assert!(matches!(
        decode_marker_channel(&ends),
        MarkerChannel::Invalid(_)
    ));
    assert!(matches!(
        decode_marker_channel(COMPANION_END),
        MarkerChannel::Invalid("end before start")
    ));
}

#[test]
fn rejects_mismatched_backend_ids_and_extra_lines() {
    let wrong_id = r#"LOOM-FORWARD-END {"v":1,"backend":"companion","job_id":"job-2","outcome":"succeeded","exit_code":0}"#;
    let mismatch = format!("{COMPANION_START}\n{wrong_id}\n{SEPARATOR}");
    assert!(matches!(
        decode_marker_channel(&mismatch),
        MarkerChannel::Invalid(_)
    ));
    let wrong_backend = r#"LOOM-FORWARD-END {"v":1,"backend":"direct","thread_id":"job-1","outcome":"succeeded","exit_code":0}"#;
    let mismatch = format!("{COMPANION_START}\n{wrong_backend}\n{SEPARATOR}");
    assert!(matches!(
        decode_marker_channel(&mismatch),
        MarkerChannel::Invalid(_)
    ));
    let extra = format!("{COMPANION_START}\nuntrusted\n{SEPARATOR}");
    assert!(matches!(
        decode_marker_channel(&extra),
        MarkerChannel::Invalid(_)
    ));
}

#[test]
fn rejects_invalid_marker_json_and_schema() {
    let invalid = [
        r#"LOOM-FORWARD-START {bad}"#,
        r#"LOOM-FORWARD-START {"v":2,"backend":"companion","job_id":"job-1"}"#,
        r#"LOOM-FORWARD-START {"v":1,"backend":"companion","job_id":"-bad"}"#,
        r#"LOOM-FORWARD-START {"v":1,"backend":"companion","job_id":"job-1","extra":true}"#,
    ];
    for start in invalid {
        assert!(
            matches!(decode_marker_channel(start), MarkerChannel::Invalid(_)),
            "{start}"
        );
    }
    let bad_outcome = r#"LOOM-FORWARD-END {"v":1,"backend":"companion","job_id":"job-1","outcome":"running","exit_code":0}"#;
    let text = format!("{COMPANION_START}\n{bad_outcome}\n{SEPARATOR}");
    assert!(matches!(
        decode_marker_channel(&text),
        MarkerChannel::Invalid(_)
    ));
}

#[test]
fn incomplete_prefix_remains_streaming() {
    assert!(matches!(
        decode_marker_channel(COMPANION_START),
        MarkerChannel::Streaming(_)
    ));
    let with_end = format!("{COMPANION_START}\n{COMPANION_END}");
    assert!(matches!(
        decode_marker_channel(&with_end),
        MarkerChannel::Streaming(_)
    ));
    let partial_end = format!("{COMPANION_START}\nLOOM-FORWARD-END {{\"v\":1");
    assert!(matches!(
        decode_marker_channel(&partial_end),
        MarkerChannel::Streaming(_)
    ));
    let partial_separator = format!("{COMPANION_START}\n--- LOOM-FORWARD");
    assert!(matches!(
        decode_marker_channel(&partial_separator),
        MarkerChannel::Streaming(_)
    ));
}

#[test]
fn considers_only_the_bounded_prefix() {
    let padding = "x".repeat(MAX_PREFIX_BYTES);
    let text = format!("{COMPANION_START}\n{padding}\n{SEPARATOR}");
    assert!(matches!(
        decode_marker_channel(&text),
        MarkerChannel::Invalid(_)
    ));
}
