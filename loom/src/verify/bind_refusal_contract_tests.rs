//! Contract: a confined run with no loopback networking on macOS whose output
//! names a refused bind carries one note naming
//! `sandbox.network.allow_local_binding` and the dispute route, and no other
//! run carries it.

use std::time::Duration;

use crate::models::stage::CommandConfinement;
use crate::verify::criteria::{format_criterion_result, refuses_loopback, NetworkAccess};
use crate::verify::criteria::{CriterionResult, ProbeRun};
use crate::verify::environment::bind_refusal_hint;

/// The stage description's NOTE TEXT quoting `line`.
fn note(line: &str) -> String {
    format!(
        "refused bind? this run had no loopback networking (macOS confinement) and printed: \
         {line}; tests that listen or dial on loopback need \
         sandbox.network.allow_local_binding: true in the plan or stage, which an operator \
         sets; the run still fails, and if it fails only because of where completion runs, \
         dispute it"
    )
}

/// The lines bun prints when `Bun.serve({ port: 0 })` cannot listen.
const BUN_STDERR: &str = "error: Failed to start server. Is port 0 in use?\n  \
                          syscall: \"listen\",\n  errno: 0,\n  code: \"EADDRINUSE\"\n";

#[test]
fn bun_serve_refusal_gets_the_exact_note() {
    let code = "code: \"EADDRINUSE\"";
    assert_eq!(bind_refusal_hint("", BUN_STDERR), Some(note(code)));
    assert_eq!(
        bind_refusal_hint(&format!("  {code}\n"), ""),
        Some(note(code)),
        "the marker line on stdout alone"
    );
    assert_eq!(
        bind_refusal_hint("", "error: FailedToOpenSocket\n"),
        Some(note("error: FailedToOpenSocket"))
    );
    assert_eq!(
        bind_refusal_hint("Error: EAddrInUse\n", ""),
        Some(note("Error: EAddrInUse")),
        "the match ignores case and the quote keeps it"
    );
    assert_eq!(
        bind_refusal_hint(
            "code: \"EADDRINUSE\" (stdout)\n",
            "error: FailedToOpenSocket (stderr)\n"
        ),
        Some(note("error: FailedToOpenSocket (stderr)")),
        "stderr is scanned before stdout"
    );
    assert_eq!(
        bind_refusal_hint(
            "",
            "Error: kill EPERM\noperation not permitted\n  code: \"EADDRINUSE\"\n"
        ),
        Some(note(code)),
        "a denial line that names no socket code is not quoted"
    );
    assert_cut_on_a_char_boundary();
}

/// A marker line followed by 400 two-byte characters is quoted cut to 300
/// bytes at most, on a character boundary.
fn assert_cut_on_a_char_boundary() {
    let prefix = "code: \"EADDRINUSE\" ";
    assert_eq!(prefix.len(), 19);
    let line = format!("{prefix}{}", "é".repeat(400));
    // 19 + 2 * 140 = 299 bytes: the most whole characters within 300 bytes.
    let kept = format!("{prefix}{}", "é".repeat(140));
    assert!(kept.len() <= 300);
    assert_eq!(
        bind_refusal_hint("", &format!("{line}\n")),
        Some(note(&kept))
    );
}

#[test]
fn marker_matches_only_as_a_token() {
    assert_eq!(
        bind_refusal_hint("", "error: eaddrinuse_retries exhausted\n"),
        None
    );
    assert_eq!(bind_refusal_hint("let eaddrinuse_retries = 3;\n", ""), None);
    assert_eq!(
        bind_refusal_hint(
            "",
            "error: Failed to start server because configuration is missing\n"
        ),
        None
    );
    assert_eq!(
        bind_refusal_hint(
            "test result: FAILED. 0 passed; 1 failed; 0 ignored\n",
            "Error: kill EPERM\noperation not permitted\n"
        ),
        None
    );
    assert_eq!(bind_refusal_hint("", ""), None);
}

#[test]
fn loopback_refused_only_for_confined_denied_macos() {
    let networks = [
        ("denied", NetworkAccess::Denied),
        ("loopback", NetworkAccess::Loopback),
        ("provision", NetworkAccess::Provision),
    ];
    let confinements = [CommandConfinement::Confined, CommandConfinement::Inherit];
    for confinement in confinements {
        for (name, network) in networks {
            for macos in [true, false] {
                let expected = confinement == CommandConfinement::Confined
                    && network == NetworkAccess::Denied
                    && macos;
                assert!(
                    refuses_loopback(confinement, network, macos) == expected,
                    "{confinement:?}, {name}, macos {macos}: expected {expected}"
                );
            }
        }
    }
}

/// A failed criterion run whose stderr names a port clash.
fn clash(exit_code: Option<i32>, timed_out: bool) -> CriterionResult {
    CriterionResult::new(
        "bun test".to_string(),
        false,
        String::new(),
        "listen EADDRINUSE: address already in use 127.0.0.1:3000\n".to_string(),
        exit_code,
        Duration::from_millis(5),
        timed_out,
    )
}

#[test]
fn no_hint_without_the_denied_loopback() {
    let built = clash(Some(1), false);
    let plain = format_criterion_result(1, &built, &[]);
    assert!(!plain.contains("allow_local_binding"), "{plain}");

    let refused = CriterionResult {
        loopback_refused: true,
        ..built
    };
    let hinted = format_criterion_result(1, &refused, &[]);
    assert_eq!(hinted.matches("allow_local_binding").count(), 1, "{hinted}");
    let hint = format!(
        "\n    {}",
        note("listen EADDRINUSE: address already in use 127.0.0.1:3000")
    );
    assert!(hinted.contains(&hint), "{hinted}");
    assert_eq!(hinted.replacen(&hint, "", 1), plain, "one added line");

    for result in [clash(None, true), clash(Some(0), false)] {
        let refused = CriterionResult {
            loopback_refused: true,
            ..result
        };
        let rendered = format_criterion_result(1, &refused, &[]);
        assert!(!rendered.contains("allow_local_binding"), "{rendered}");
    }
}

#[test]
fn probe_run_carries_the_note() {
    let mut run = ProbeRun::finished("", "code: \"EADDRINUSE\"", Some(1));
    assert_eq!(run.bind_note(), None, "built with loopback_refused false");

    run.loopback_refused = true;
    let refused = run.bind_note();
    assert!(
        refused
            .as_deref()
            .is_some_and(|note| note.contains("allow_local_binding")),
        "{refused:?}"
    );
    assert_eq!(refused, Some(note("code: \"EADDRINUSE\"")));

    run.timed_out = true;
    assert_eq!(run.bind_note(), None, "a timed-out run");

    run.timed_out = false;
    run.exit_code = Some(0);
    assert_eq!(run.bind_note(), None, "a zero-exit run");
}
