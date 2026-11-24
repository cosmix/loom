use super::*;

#[test]
fn a_same_length_rewrite_is_counted_from_the_new_content() {
    let fx = Fixture::new();
    let now = unix_now();
    let first = body(&[line("m1", now - 90, 100), line("m2", now - 80, 100)]);
    let transcript = fx.add_session("alpha", SESSION_UUID, &first);
    assert_eq!(alpha_total(&fx, now).cache_write, 200);

    let rewrite = body(&[line("m1", now - 90, 200), line("m2", now - 80, 300)]);
    assert_eq!(first.len(), rewrite.len(), "a rewrite of equal length");
    fs::write(&transcript, &rewrite).expect("rewrite in place");

    assert_eq!(alpha_total(&fx, now).cache_write, 500);
}

#[test]
fn a_longer_rewrite_between_scans_matches_a_fresh_scan() {
    let fx = Fixture::new();
    let now = unix_now();
    let first = body(&[line("m1", now - 90, 100), line("m2", now - 80, 100)]);
    let transcript = fx.add_session("alpha", SESSION_UUID, &first);
    fx.scan(now);

    let rewrite = body(&[
        line("m1", now - 90, 7),
        line("m3", now - 85, 11),
        line("m2", now - 80, 13),
        line("m4", now - 70, 17),
    ]);
    assert!(rewrite.len() > first.len());
    fs::write(&transcript, &rewrite).expect("rewrite in place");

    let fresh = Fixture::new();
    fresh.add_session("alpha", SESSION_UUID, &rewrite);
    assert_eq!(alpha_total(&fx, now), alpha_total(&fresh, now));
    assert_eq!(alpha_total(&fx, now).cache_write, 48);
}

#[test]
fn appended_lines_are_added_without_a_restart() {
    let fx = Fixture::new();
    let now = unix_now();
    let transcript = fx.add_session("alpha", SESSION_UUID, &body(&[line("m1", now - 90, 100)]));
    fx.scan(now);

    append(&transcript, &body(&[line("m2", now - 80, 50)]));

    assert_eq!(alpha_total(&fx, now).cache_write, 150);
    let saved = Checkpoint::load(&fx.tokens_dir());
    let cursor = &saved.stages["alpha"][&format!("main-{SESSION_UUID}")].file;
    assert_eq!(
        cursor.offset,
        fs::metadata(&transcript).expect("stat").len()
    );
}

#[test]
fn a_revised_message_after_shedding_is_not_counted_twice() {
    let fx = Fixture::new();
    let now = unix_now();
    let main = fx.add_session("alpha", SESSION_UUID, &bulky_body("main", now));
    let main_key = format!("main-{SESSION_UUID}");
    pass_at(&fx, now);
    let sub = fx.add_subagent(SESSION_UUID, "agent-a1.jsonl", &bulky_body("sub", now));
    pass_at(&fx, now);
    assert_eq!(saved_table_len(&fx, &main_key), 0, "the scenario: shed");
    let saved = Checkpoint::load(&fx.tokens_dir());
    assert!(saved.stages["alpha"][&main_key].file.table_shed);

    let id = format!("main{0:0>1200}", 0);
    append(
        &main,
        &body(&[claude_line(&id, now - 59, OPUS, &usage(0, 7, 0, 0))]),
    );
    let total = stage(&pass_at(&fx, now).flow, "alpha").total;

    let fresh = Fixture::new();
    fresh.add_session(
        "alpha",
        SESSION_UUID,
        &fs::read_to_string(&main).expect("main"),
    );
    fresh.add_subagent(
        SESSION_UUID,
        "agent-a1.jsonl",
        &fs::read_to_string(&sub).expect("sub"),
    );
    assert_eq!(total, alpha_total(&fresh, now));
    assert_eq!(total.cache_write, 2 * MESSAGE_TABLE_CAP as u64 + 6);
}
