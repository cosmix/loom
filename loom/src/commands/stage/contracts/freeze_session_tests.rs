//! A contract session's freeze that fails prints the `Host job: refused`
//! first line, then the failure's text.

use anyhow::anyhow;

use super::{refuse, session_refusal};

#[test]
fn a_failed_session_freeze_prints_the_refusal_first_line_then_the_problems() {
    let error = refuse(
        "s1",
        vec!["tests/a.rs does not exist".to_string()],
        "Revert it, then",
    )
    .unwrap_err();
    let mut out = Vec::new();

    let code = session_refusal(&mut out, &error).unwrap();

    let text = String::from_utf8(out).unwrap();
    assert_eq!(code, 1);
    assert!(text.starts_with("Host job: refused (freeze, "), "{text}");
    assert!(text.contains("Contracts not frozen:"), "{text}");
    assert!(text.contains("  - tests/a.rs does not exist"), "{text}");
}

#[test]
fn an_error_with_context_prints_the_whole_chain() {
    let error = anyhow!("the stage file is gone").context("Failed to load stage 's1'");
    let mut out = Vec::new();

    session_refusal(&mut out, &error).unwrap();

    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with("Host job: refused (freeze, "), "{text}");
    assert!(
        text.contains("Failed to load stage 's1': the stage file is gone"),
        "{text}"
    );
}
