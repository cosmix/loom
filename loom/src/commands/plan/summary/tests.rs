use super::*;

#[test]
fn exit_codes_distinguish_refusals_from_other_errors() {
    let held = claims::ClaimHeld {
        summary: "plans/x".into(),
        holder: Some("s1".into()),
        release: false,
    };
    assert_eq!(exit_code(&claim_error(anyhow::Error::new(held.clone()))), 1);
    let wrapped = anyhow::Error::new(held).context("while claiming");
    assert_eq!(exit_code(&claim_error(wrapped)), 1, "typed through context");
    // Text alone is no refusal: only the typed error maps to exit code 1.
    assert_eq!(
        exit_code(&claim_error(anyhow::anyhow!(
            "plans/x is claimed by another session"
        ))),
        2
    );
    assert_eq!(
        exit_code(&claim_error(anyhow::anyhow!("cannot read claims"))),
        2
    );
    assert_eq!(
        exit_code(&CommandError::Rejected("invalid draft".into())),
        1
    );
}

#[test]
fn draft_errors_include_path_and_each_line() {
    let errors = vec![
        FormatError {
            line: 2,
            message: "first problem".into(),
        },
        FormatError {
            line: 5,
            message: "second problem".into(),
        },
    ];
    assert_eq!(
        draft_errors(Path::new("/runtime/drafts/plans/x.md"), &errors),
        "/runtime/drafts/plans/x.md:2: first problem\n/runtime/drafts/plans/x.md:5: second problem"
    );
}

#[test]
fn draft_errors_neutralize_terminal_escapes() {
    let errors = vec![FormatError {
        line: 1,
        message: "link target \u{1b}]52;c;x\u{7} is bad".into(),
    }];
    let text = draft_errors(Path::new("/tmp/draft.md"), &errors);
    assert!(!text.contains('\u{1b}'));
    assert!(!text.contains('\u{7}'));
    assert!(text.contains("is bad"));
}
