//! Tests for `hook_recipient_id`, `delivered_to_session` and
//! `discard_session_delivery` (A.16/A.21) — split out of `delivery.rs` to
//! keep that file under the maintainability line limit. Same fixtures, same
//! `PLAN`/`STAGE` constants, and the same `record`/`item` helpers as the
//! parent file, reached the ordinary Rust way: `use super::*;`.

use super::*;

// ── hook_recipient_id (A.16) ────────────────────────────────────────────

#[test]
fn hook_recipient_id_hashes_the_session_into_sixteen_lowercase_hex_chars() {
    let key = hook_recipient_id("delivery-stage", Some("session-abc-123"));

    let suffix = key
        .strip_prefix("prompt-delivery-stage-")
        .expect("scope and prefix survive verbatim");
    assert_eq!(suffix.len(), 16, "8 bytes, hex-encoded: {suffix}");
    assert!(
        suffix
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "must be lowercase hex only: {suffix}"
    );
}

#[test]
fn hook_recipient_id_is_stable_per_session_and_differs_across_sessions() {
    let first = hook_recipient_id("scope", Some("session-a"));
    let repeat = hook_recipient_id("scope", Some("session-a"));
    let other = hook_recipient_id("scope", Some("session-b"));

    assert_eq!(
        first, repeat,
        "the same session must always derive the same key"
    );
    assert_ne!(first, other, "two sessions must not collide on one key");
}

#[test]
fn hook_recipient_id_with_no_usable_session_id_shares_the_nosession_key() {
    assert_eq!(hook_recipient_id("scope", None), "prompt-scope-nosession");
    assert_eq!(
        hook_recipient_id("scope", Some("   ")),
        "prompt-scope-nosession",
        "whitespace-only counts as absent"
    );
    assert_eq!(
        hook_recipient_id("scope", Some("")),
        "prompt-scope-nosession"
    );
}

#[test]
fn hook_recipient_id_stays_filesystem_safe_for_a_hostile_session_id() {
    // The security property this key exists for: whatever bytes a hook's
    // stdin JSON carries as `session_id`, the derived key must still pass
    // `validate_recipient_id` by construction. Proven here indirectly, the
    // only way a black-box test can: `record_delivery` refuses anything that
    // fails validation, so a successful write IS the proof.
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".work");

    for hostile in [
        "../escape",
        "a/b",
        "a\\b",
        "session:1",
        "sessio\0n",
        "caf\u{e9}",
        "\u{4f1a}\u{8bdd}",
    ] {
        let recipient = hook_recipient_id("scope", Some(hostile));
        let written = record(&recipient, "epoch-a", &[]);
        record_delivery(&work_dir, PLAN, STAGE, &written)
            .unwrap_or_else(|e| panic!("recipient derived from {hostile:?} was refused: {e}"));
    }
}

// ── delivered_to_session (A.16) ─────────────────────────────────────────

#[test]
fn delivered_to_session_excludes_a_different_sessions_deliveries() {
    let session_a = hook_recipient_id("scope", Some("session-a"));
    let session_b = hook_recipient_id("scope", Some("session-b"));
    let records = vec![record(&session_a, "epoch-a", &[("first", "h1")])];

    let delivered = delivered_to_session(&records, "epoch-a", &session_b, None);

    assert!(
        delivered.is_empty(),
        "a fresh session must not inherit another session's deliveries"
    );
}

#[test]
fn delivered_to_session_suppresses_its_own_prior_deliveries() {
    let session_a = hook_recipient_id("scope", Some("session-a"));
    let records = vec![record(&session_a, "epoch-a", &[("first", "h1")])];

    let delivered = delivered_to_session(&records, "epoch-a", &session_a, None);

    assert!(delivered.contains(&("first".to_string(), "h1".to_string())));
}

#[test]
fn delivered_to_session_includes_the_named_spawn_records_items() {
    let records = vec![
        record("session-live", "epoch-a", &[("own", "h1")]),
        record("spawn-recipient", "epoch-a", &[("spawned", "h2")]),
        record("other-session", "epoch-a", &[("unrelated", "h3")]),
    ];

    let delivered =
        delivered_to_session(&records, "epoch-a", "session-live", Some("spawn-recipient"));

    assert_eq!(
        delivered,
        [
            ("own".to_string(), "h1".to_string()),
            ("spawned".to_string(), "h2".to_string())
        ]
        .into_iter()
        .collect(),
        "only the caller's own recipient and the named spawn recipient contribute"
    );
}

#[test]
fn delivered_to_session_with_a_spawn_recipient_that_never_ran_is_empty_not_an_error() {
    let records = vec![record("session-live", "epoch-a", &[("own", "h1")])];

    let delivered =
        delivered_to_session(&records, "epoch-a", "session-live", Some("never-spawned"));

    assert_eq!(
        delivered,
        [("own".to_string(), "h1".to_string())]
            .into_iter()
            .collect(),
        "a missing spawn record contributes nothing, and must not error"
    );
}

// ── discard_session_delivery (A.21) ─────────────────────────────────────

#[test]
fn discard_session_delivery_removes_only_the_named_recipients_record() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".work");
    let session_b_record = record("session-b", "epoch-a", &[("second", "h2")]);
    record_delivery(
        &work_dir,
        PLAN,
        STAGE,
        &record("session-a", "epoch-a", &[("first", "h1")]),
    )
    .unwrap();
    record_delivery(&work_dir, PLAN, STAGE, &session_b_record).unwrap();

    discard_session_delivery(&work_dir, PLAN, STAGE, "session-a").unwrap();

    assert_eq!(
        load_deliveries(&work_dir, PLAN, STAGE).unwrap(),
        vec![session_b_record],
        "the other session's record must survive untouched"
    );
}

#[test]
fn discard_session_delivery_on_an_absent_directory_is_a_silent_no_op() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".work");

    discard_session_delivery(&work_dir, PLAN, STAGE, "session-a").unwrap();

    assert!(load_deliveries(&work_dir, PLAN, STAGE).unwrap().is_empty());
    assert!(
        !delivery_dir(&work_dir, PLAN, STAGE).exists(),
        "resetting must never create the directory as a side effect"
    );
}

#[test]
fn discard_session_delivery_on_a_recipient_with_no_record_is_a_silent_no_op() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".work");
    record_delivery(
        &work_dir,
        PLAN,
        STAGE,
        &record("session-b", "epoch-a", &[("second", "h2")]),
    )
    .unwrap();

    discard_session_delivery(&work_dir, PLAN, STAGE, "session-a").unwrap();

    assert_eq!(load_deliveries(&work_dir, PLAN, STAGE).unwrap().len(), 1);
}
