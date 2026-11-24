//! The environment markers: which line names a denial, how long it may be,
//! and when a run's own verdict wins over the marker words in its output.
//! Assertion messages are fixed text without marker words.

use super::environment::{environment_marker, refused_bind_note, run_denial, sandbox_denial};

#[test]
fn the_first_matching_line_is_named_trimmed() {
    let output = "compiling\n   sandbox-exec: sandbox_apply: Operation not permitted  \nbwrap: x\n";
    assert_eq!(
        sandbox_denial(output).as_deref(),
        Some("sandbox-exec: sandbox_apply: Operation not permitted"),
        "the first denial line, trimmed"
    );
    assert_eq!(sandbox_denial("all good\nnothing here\n"), None);
    assert_eq!(sandbox_denial(""), None);
}

#[test]
fn every_denial_shape_is_recognised_whatever_its_case() {
    for line in [
        "error: CouldntReadCurrentDirectory",
        "ERROR: COULDNTREADCURRENTDIRECTORY",
        "Error: kill EPERM",
        "error: kill eperm",
        "EPERM: operation not permitted, open 'x'",
        "bwrap: Can't mount proc",
        "BWRAP: no",
        "sandbox_apply: refused",
        "sandbox-exec: Deny(1) file-write-data",
    ] {
        assert!(
            sandbox_denial(line).is_some(),
            "a denial line is not recognised: {line}"
        );
    }
}

#[test]
fn eperm_is_a_token_not_a_substring() {
    for line in [
        "teleperm started",
        "EPERMS counted",
        "my_eperm_flag = 1",
        "xEPERM",
    ] {
        assert_eq!(
            sandbox_denial(line),
            None,
            "a longer word is no denial: {line}"
        );
    }
    assert!(sandbox_denial("kill(2) failed: EPERM.").is_some());
    assert!(sandbox_denial("os.EPERM raised").is_some());
}

#[test]
fn a_long_line_is_cut_on_a_character_boundary() {
    let long = format!("Error: kill EPERM {}", "é".repeat(400));
    let named = sandbox_denial(&long).unwrap_or_default();
    assert!(
        named.len() <= 300 && named.len() > 290,
        "cut near 300 bytes"
    );
    assert!(
        long.starts_with(&named),
        "the cut is a prefix of the line on whole characters"
    );
    let odd = format!("Error: kill EPERM {}", "€".repeat(200));
    let named = sandbox_denial(&odd).unwrap_or_default();
    assert!(named.len() <= 300 && odd.starts_with(&named));
}

#[test]
fn the_network_and_disk_markers_stay_in_the_shared_marker() {
    assert_eq!(
        environment_marker("", "Could not resolve host: x"),
        Some("could not resolve host")
    );
    assert_eq!(
        environment_marker("No space left on device", ""),
        Some("no space left on device")
    );
    assert_eq!(environment_marker("test result: ok", ""), None);
}

#[test]
fn a_run_with_a_parsed_summary_or_build_failure_names_no_denial() {
    let counted = "running 1 test\ntest t::k ... FAILED\n\nfailures:\n\n---- t::k stdout ----\n\
        assertion failed: kill returned EPERM\n\nfailures:\n    t::k\n\n\
        test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n";
    assert_eq!(run_denial("cargo test", counted, "", Some(101)), None);
    let build = "error[E0425]: cannot find value `EPERM` in this scope\n\
        error: could not compile `demo` (lib test) due to 1 previous error\n";
    assert_eq!(run_denial("cargo test", "", build, Some(101)), None);
    let refused = "bun test v1.2.0\nerror: CouldntReadCurrentDirectory\n";
    assert_eq!(
        run_denial("bun test", refused, "", Some(1)).as_deref(),
        Some("error: CouldntReadCurrentDirectory")
    );
    assert_eq!(run_denial("true", "ok\n", "", Some(1)), None);
}

#[test]
fn a_refused_bind_note_needs_a_refused_loopback_and_a_failing_finished_run() {
    let printed = "code: \"EADDRINUSE\"\n";
    let note = |refused, timed_out, exit| refused_bind_note(refused, timed_out, exit, "", printed);
    assert_eq!(note(false, false, Some(1)), None, "loopback was available");
    assert_eq!(note(true, true, None), None, "a timed-out run");
    assert_eq!(note(true, false, Some(0)), None, "a zero-exit run");
    let held = note(true, false, Some(1));
    assert!(
        held.as_deref()
            .is_some_and(|text| text.contains("allow_local_binding")),
        "{held:?}"
    );
}

#[test]
fn a_counted_failing_run_still_gets_the_refused_bind_note() {
    let counted = "code: \"EADDRINUSE\"\nrunning 1 test\ntest t::k ... FAILED\n\n\
        test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n";
    assert!(refused_bind_note(true, false, Some(101), counted, "").is_some());
}
