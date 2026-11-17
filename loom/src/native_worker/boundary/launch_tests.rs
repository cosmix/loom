use super::{parse_info, receive_within, InfoError};

/// A report that has not arrived by the deadline may follow a cloned
/// namespace, so it is ambiguous and never the scratch-removing NotReported.
#[test]
fn a_late_report_is_ambiguous() {
    let (reader, writer) = std::io::pipe().expect("pipe");
    let result = receive_within(reader, std::time::Duration::from_millis(50));
    drop(writer);
    assert!(matches!(result, Err(InfoError::Ambiguous(_))), "{result:?}");
}

fn classify(input: &[u8]) -> Result<(i32, u64), InfoError> {
    parse_info(input)
}

/// Only a pipe that closed with nothing but whitespace on it proves
/// bubblewrap reported nothing; the scratch may then go.
#[test]
fn an_empty_report_is_not_reported() {
    for input in ["", " ", "\n\t \n"] {
        let result = classify(input.as_bytes());
        assert!(
            matches!(result, Err(InfoError::NotReported)),
            "{input:?}: {result:?}"
        );
    }
}

/// A partial, incomplete or malformed report may follow a cloned namespace:
/// it is ambiguous, so the scratch is kept.
#[test]
fn a_partial_or_malformed_report_is_ambiguous() {
    let inputs: [&[u8]; 6] = [
        br#"{"child-pid": 5"#,
        b"{}",
        br#"{"child-pid": 5}"#,
        br#"{"pid-namespace": 4026531836}"#,
        br#"{"child-pid": 4294967296, "pid-namespace": 1}"#,
        b"\xff\xfe garbage",
    ];
    for input in inputs {
        let result = classify(input);
        let shown = String::from_utf8_lossy(input);
        assert!(
            matches!(result, Err(InfoError::Ambiguous(_))),
            "{shown}: {result:?}"
        );
    }
}

/// A complete report names the first process and the namespace inode; only
/// the first JSON value is read.
#[test]
fn a_complete_report_names_the_namespace() {
    let input = br#"{"child-pid": 5, "pid-namespace": 4026531836} {"exit-code": 0}"#;
    assert_eq!(classify(input).unwrap(), (5, 4026531836));
}
