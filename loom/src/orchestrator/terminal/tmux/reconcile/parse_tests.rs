//! Pure unit tests for [`super::parse_pane_socket`] and
//! [`super::parse_pane_list`] — no tmux server needed. Density matches
//! `commands/attach/tests/overview.rs`: every test names the real regression
//! it prevents.

use super::*;

// ---------------------------------------------------------------------
// parse_pane_socket
// ---------------------------------------------------------------------

#[test]
fn parse_pane_socket_parses_the_real_tmux_3_6a_rendering() {
    // THE REAL SHAPE: verified empirically against tmux 3.6a (see
    // `parse_pane_socket`'s doc comment). A parser tied to a DIFFERENT
    // rendering would silently stop attributing every viewer pane.
    let start_command = r#"sh -c "unset TMUX; exec tmux -L loom-session-abc12345-1754900000 attach-session -t some-key""#;
    assert_eq!(
        parse_pane_socket(start_command),
        Some("loom-session-abc12345-1754900000".to_string())
    );
}

#[test]
fn parse_pane_socket_ignores_a_plain_login_shell_pane() {
    // An operator's own split running their login shell has no `attach-session`
    // token at all and must never be attributed to loom.
    assert_eq!(parse_pane_socket("zsh"), None);
}

#[test]
fn parse_pane_socket_ignores_a_pane_with_no_attach_session() {
    assert_eq!(parse_pane_socket(r#"sh -c "sleep 30""#), None);
}

#[test]
fn parse_pane_socket_ignores_attach_session_without_dash_l() {
    // No explicit socket at all — rule 2 must reject it rather than guess one.
    assert_eq!(
        parse_pane_socket(r#"sh -c "tmux attach-session -t work""#),
        None
    );
}

#[test]
fn parse_pane_socket_strips_surrounding_quotes_from_the_socket_token() {
    assert_eq!(
        parse_pane_socket(r#"sh -c "exec tmux -L 'loom-session-quoted' attach-session -t k""#),
        Some("loom-session-quoted".to_string())
    );
}

#[test]
fn parse_pane_socket_rejects_a_one_sided_quote_fragment_and_a_non_identifier() {
    // A quoted socket containing whitespace splits at tokenization, handing
    // back the fragment `'loom-a` — attributing it would make Rule 1 re-add
    // the real socket on EVERY tick (unbounded pane growth). Rule 3 rejects
    // the asymmetric fragment; rule 5's charset check rejects anything that
    // is not a plain loom identifier even when the quoting is intact.
    assert_eq!(
        parse_pane_socket("tmux -L 'loom-a b' attach-session -t k"),
        None
    );
    assert_eq!(
        parse_pane_socket(r#"tmux -L "loom-x;rm" attach-session -t k"#),
        None
    );
}

#[test]
fn parse_pane_socket_rejects_a_non_loom_socket() {
    // OPERATOR-PANE PROTECTION: if the operator splits their own pane and
    // runs a plain `tmux attach-session` against THEIR OWN socket, rules 1-3
    // alone would attribute it as a loom pane, and `reconcile_steps` would
    // then be free to kill it as a stray duplicate or an orphan. Requiring
    // the `loom-` prefix is what keeps that pane untouchable.
    assert_eq!(
        parse_pane_socket("tmux -L my-own-socket attach-session -t work"),
        None
    );
}

// ---------------------------------------------------------------------
// parse_pane_list
// ---------------------------------------------------------------------

#[test]
fn parse_pane_list_splits_on_real_tab_bytes_and_skips_short_lines() {
    // `PANE_FORMAT` renders id/dead/start_command TAB-separated with no
    // shell in between — a parser that split on plain whitespace would
    // shatter every multi-word start command. The trailing short line
    // simulates a row caught mid-write and must be skipped, not panic.
    let stdout =
        "%1\t0\tzsh\n%2\t1\tsh -c \"exec tmux -L loom-session-a attach-session -t k\"\n%3\n";

    let panes = parse_pane_list(stdout);

    assert_eq!(
        panes.len(),
        2,
        "the malformed trailing line must be skipped"
    );
    assert_eq!(panes[0].id, "%1");
    assert!(!panes[0].dead);
    assert_eq!(panes[0].session_socket, None);
    assert_eq!(panes[1].id, "%2");
    assert!(panes[1].dead);
    assert_eq!(panes[1].session_socket, Some("loom-session-a".to_string()));
}
