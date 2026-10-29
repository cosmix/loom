//! Cost lines: their exact token vectors, and costing that survives a claim error.

use std::os::unix::fs::symlink;

use super::*;

/// The usage of one `bash()` record: input 10, cache creation 200, cache read 3000, output 40.
const ONE_BASH: TokenVector = TokenVector {
    fresh_input: Some(10),
    cache_write: Some(200),
    cache_read: Some(3_000),
    output: Some(40),
};

/// Make every claim call fail: a symlinked claims directory is refused.
fn break_claims(tmp: &TempDir, paths: &RuntimePaths) {
    let elsewhere = tmp.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).expect("directory");
    symlink(&elsewhere, &paths.claims).expect("symlinked claims directory");
}

#[test]
fn cost_lines_carry_the_transcripts_token_vector() {
    let (tmp, paths) = runtime();
    let mut attribution = Attribution::new(paths.clone());
    let triggers = [("plans/a", Trigger::Button), ("plans/b", Trigger::Button)];
    let triggers = triggers.map(|(id, trigger)| (id.to_owned(), trigger));
    attribution.summary_started("s1", BTreeMap::from(triggers));
    let one = [bash("t1", "loom plan summary brief plans/a")];
    let one = transcript(tmp.path(), "agent-x.jsonl", &one);
    let two = [
        bash("t2", "loom plan summary brief plans/b"),
        bash("t3", "ls"),
    ];
    let two = transcript(tmp.path(), "agent-y.jsonl", &two);
    let x = attribution.on_subagent_stopped("s1", "x", &one).expect("x");
    let y = attribution.on_subagent_stopped("s1", "y", &two).expect("y");
    let main = transcript(tmp.path(), "main.jsonl", &[bash("t4", "ls")]);
    attribution.note_main_transcript("s1", Some(main));
    let ended = attribution.on_session_ended("s1").expect("end");

    let twice = TokenVector {
        fresh_input: Some(20),
        cache_write: Some(400),
        cache_read: Some(6_000),
        output: Some(80),
    };
    assert_eq!(x.lines[0].tokens, ONE_BASH);
    assert_eq!(
        y.lines[0].tokens, twice,
        "two records of different messages"
    );
    assert_eq!(ended[0].tokens, ONE_BASH);
    let log = cost::read_log(&paths).expect("log");
    let logged: Vec<TokenVector> = log.iter().map(|line| line.tokens).collect();
    assert_eq!(logged, [ONE_BASH, twice, ONE_BASH]);
}

#[test]
fn a_claim_error_on_stop_still_costs_the_subagent() {
    let (tmp, paths) = runtime();
    break_claims(&tmp, &paths);
    let mut attribution = Attribution::new(paths.clone());
    let briefed = [bash("t1", "loom plan summary brief plans/a")];
    let agent = transcript(tmp.path(), "agent-x.jsonl", &briefed);

    let error = attribution
        .on_subagent_stopped("s1", "x", &agent)
        .expect_err("the claims cannot be listed");

    assert!(
        format!("{error:#}").contains("claims directory"),
        "{error:#}"
    );
    let log = cost::read_log(&paths).expect("log");
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].summaries, owned(&["plans/a"]));
    assert_eq!(log[0].tokens, ONE_BASH);
}

#[test]
fn a_claim_error_on_session_end_still_costs_and_forgets_the_session() {
    let (tmp, paths) = runtime();
    let mut attribution = Attribution::new(paths.clone());
    let triggers = [
        ("plans/a", Trigger::Background),
        ("plans/b", Trigger::Background),
    ];
    let triggers = triggers.map(|(id, trigger)| (id.to_owned(), trigger));
    attribution.summary_started("s1", BTreeMap::from(triggers));
    let briefed = [bash("t1", "loom plan summary brief plans/a")];
    let agent = transcript(tmp.path(), "agent-x.jsonl", &briefed);
    attribution
        .on_subagent_stopped("s1", "x", &agent)
        .expect("x");
    let main = transcript(tmp.path(), "main.jsonl", &[bash("t2", "ls")]);
    attribution.note_main_transcript("s1", Some(main));
    break_claims(&tmp, &paths);

    attribution
        .on_session_ended("s1")
        .expect_err("the claims cannot be cleared");

    let log = cost::read_log(&paths).expect("log");
    let costed: Vec<Vec<String>> = log.iter().map(|line| line.summaries.clone()).collect();
    assert_eq!(costed, [owned(&["plans/a"]), owned(&["plans/b"])]);
    assert_eq!(log[1].tokens, ONE_BASH, "the session's own transcript");
    assert!(attribution.sessions.is_empty() && attribution.by_agent.is_empty());
    attribution
        .on_session_ended("s1")
        .expect_err("the claims are still broken");
    assert_eq!(cost::read_log(&paths).expect("log").len(), 2, "costed once");
}
