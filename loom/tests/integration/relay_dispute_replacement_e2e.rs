//! End-to-end filing of a frozen-contract dispute that carries a replacement file.
//!
//! `loom stage dispute-contract --replacement <frozen-file>=<path>` (real
//! binary), the real `loom-relay.sh` hook, then `Orchestrator::drain_session_inboxes` through
//! admission to `request.md`. The fixture follows `relay_commit_e2e.rs`: a
//! repository with a registered worktree at `.worktrees/<stage>`, here holding
//! a plan v2 stage with one frozen contract. No socket is opened.

use chrono::Utc;
use loom::fs::inbox::{read_ledger, LedgerOutcome};
use loom::fs::stage_request::StageRequest;
use loom::models::dispute::{DisputeKind, FileReplacement, MAX_REPLACEMENT_BYTES};
use loom::orchestrator::adjudication::read_request;
use loom::process::sandbox_probe::skip_unless;
use loom::relay::{new_request_id, RelayLine, RequestKind, Ticket, MAX_TICKET_BYTES};
use serial_test::serial;

use super::helpers;

#[path = "relay_dispute_replacement_fixture.rs"]
mod fixture;
use fixture::{
    build_fixture, drain_once, run_hook, sha256_hex, Fixture, CONTRACT_FILE, CONTRACT_ID, FROZEN,
    REASON, STAGE,
};

/// File `text` as a replacement through the CLI, the hook and the drain, and
/// assert admission recorded exactly those bytes in dispute 1's `request.md`.
fn file_and_assert_recorded(fx: &Fixture, text: &str) {
    let replacement = fx.write_replacement(text);
    let filed = fx.run_dispute(&replacement);
    assert!(
        filed.status.success(),
        "loom stage dispute-contract failed: {}",
        String::from_utf8_lossy(&filed.stderr)
    );
    let stdout = String::from_utf8_lossy(&filed.stdout);
    let last_line = stdout.lines().next_back().expect("stdout carries a line");
    let line = RelayLine::parse(last_line).expect("last stdout line is a relay line");
    assert_eq!(line.kind, RequestKind::FileDispute);

    let hook = run_hook(fx, &fx.dispute_command(&replacement), &filed);
    assert!(
        hook.status.success(),
        "loom-relay.sh must always exit 0: stderr={}",
        String::from_utf8_lossy(&hook.stderr)
    );
    drain_once(fx);

    let ledger = read_ledger(&fx.work_dir, &fx.session_id).expect("read ledger");
    let last = ledger.iter().rev().find(|record| record.id == line.id);
    let last = last.unwrap_or_else(|| panic!("ledger has no row for request {}", line.id));
    assert_eq!(last.outcome, Some(LedgerOutcome::Applied), "{last:?}");

    let request = read_request(&fx.work_dir, STAGE, 1).expect("admission wrote request.md");
    let DisputeKind::Contract {
        contract_ids,
        replacements,
    } = request.kind
    else {
        panic!("not a contract dispute");
    };
    assert_eq!(contract_ids, [CONTRACT_ID]);
    let [recorded] = replacements.as_slice() else {
        panic!("one replacement is recorded: {replacements:?}");
    };
    assert_eq!(recorded.file, CONTRACT_FILE);
    assert_eq!(recorded.content, text, "the bytes survive the chain");
    assert_eq!(recorded.base_sha256, Some(sha256_hex(FROZEN)));
}

/// Byte size of the ticket the CLI writes for a dispute carrying `text`.
fn encoded_ticket_len(text: &str) -> usize {
    let kind = DisputeKind::Contract {
        contract_ids: vec![CONTRACT_ID.to_string()],
        replacements: vec![FileReplacement {
            file: CONTRACT_FILE.to_string(),
            content: text.to_string(),
            base_sha256: None,
        }],
    };
    let request = StageRequest::FileDispute {
        kind,
        reason: REASON.to_string(),
        evidence_commit: None,
    };
    Ticket {
        v: 1,
        id: new_request_id(),
        kind: RequestKind::FileDispute,
        created_at: Utc::now(),
        payload: serde_json::to_value(&request).expect("serialize the dispute"),
    }
    .encode()
    .len()
}

/// A replacement within the byte bound whose encoded ticket is just under
/// `MAX_TICKET_BYTES`: each quote costs two bytes once JSON-escaped.
fn replacement_near_ticket_limit() -> String {
    let target = MAX_TICKET_BYTES - 512;
    let (mut low, mut high) = (0usize, MAX_REPLACEMENT_BYTES);
    while low < high {
        let mid = (low + high).div_ceil(2);
        if encoded_ticket_len(&"\"".repeat(mid)) <= target {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    "\"".repeat(low)
}

fn skip_for_missing_tools(test_name: &str) -> bool {
    let present = ["jq", "bash", "git"]
        .iter()
        .all(|tool| helpers::tool_available(tool));
    skip_unless(present, test_name, "jq, bash or git was not found on PATH")
}

/// A 30 KiB replacement of ordinary text reaches `request.md` byte for byte.
#[test]
#[serial]
fn a_30_kib_replacement_is_recorded_byte_exact() {
    if skip_for_missing_tools("a_30_kib_replacement_is_recorded_byte_exact") {
        return;
    }
    let _terminal_env = helpers::EnvVarGuard::set("LOOM_TERMINAL", "xterm");
    let fx = build_fixture();
    let line = "#[test]\r\nfn rejects_x() { assert!(rejects(\"x\")); }   \r\n";
    let text = line.repeat(30 * 1024 / line.len());

    file_and_assert_recorded(&fx, &text);
}

/// The envelope the inbox and the daemon frame add must not push a ticket the
/// CLI accepted over a limit: this one is within 1 KiB of `MAX_TICKET_BYTES`.
#[test]
#[serial]
fn a_replacement_whose_ticket_is_within_1_kib_of_the_limit_is_admitted() {
    if skip_for_missing_tools("a_replacement_whose_ticket_is_within_1_kib_of_the_limit_is_admitted")
    {
        return;
    }
    let _terminal_env = helpers::EnvVarGuard::set("LOOM_TERMINAL", "xterm");
    let fx = build_fixture();
    let text = replacement_near_ticket_limit();
    let encoded = encoded_ticket_len(&text);
    assert!(
        encoded <= MAX_TICKET_BYTES && MAX_TICKET_BYTES - encoded < 1024,
        "the probe must land within 1 KiB of the limit: {encoded}"
    );

    file_and_assert_recorded(&fx, &text);
}

/// A replacement over the byte bound, or one under it that encodes over the
/// ticket bound, is refused by the CLI with no ticket written.
#[test]
#[serial]
fn an_oversize_replacement_is_refused_before_any_ticket_is_written() {
    if skip_for_missing_tools("an_oversize_replacement_is_refused_before_any_ticket_is_written") {
        return;
    }
    let fx = build_fixture();
    let by_bytes = "x".repeat(40 * 1024);
    let by_encoding = "\u{1}".repeat(31 * 1024);

    for (text, expected) in [(by_bytes, "32768"), (by_encoding, "65536")] {
        let output = fx.run_dispute(&fx.write_replacement(&text));

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected), "{stderr}");
        if expected == "65536" {
            assert!(stderr.contains("32768"), "both limits are named: {stderr}");
        }
        assert!(fx.tickets().is_empty(), "no ticket may be written");
    }
}
