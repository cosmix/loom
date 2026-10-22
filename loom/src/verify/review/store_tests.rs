//! The review-round store: `record_round` serializes concurrent recorders.

use super::*;
use std::sync::Barrier;
use std::time::Duration;
use tempfile::TempDir;

const STAGE: &str = "s1";
const AGENT: &str = "reviewer";
const FINGERPRINT: &str = "sha256:0000";

fn round(number: u32) -> ReviewRound {
    ReviewRound {
        version: RECORD_VERSION,
        round: number,
        agent_id: AGENT.to_string(),
        harvested_at: Utc::now(),
        fingerprint: FINGERPRINT.to_string(),
        files: BTreeMap::new(),
        malformed: None,
        findings: Vec::new(),
        resolved: Vec::new(),
        unresolved: Vec::new(),
        suggestion_memory_ids: Vec::new(),
    }
}

/// The review harvest's rule: a recorded round of the same agent at the same
/// fingerprint is the review being recorded again.
fn decide(number: u32, recorded: &[ReviewRound]) -> Result<RoundDecision> {
    // Widen the window between reading the rounds and writing one, so a
    // `record_round` that released its lock in between would let several
    // recorders see no round and each write one.
    std::thread::sleep(Duration::from_millis(20));
    let repeat = recorded
        .iter()
        .find(|r| r.agent_id == AGENT && r.fingerprint == FINGERPRINT);
    Ok(match repeat {
        Some(existing) => RoundDecision::Duplicate(existing.round),
        None => RoundDecision::Record(Box::new(round(number))),
    })
}

#[test]
fn concurrent_recorders_of_one_review_write_one_round() {
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path();
    let recorders = 4;
    let start = Barrier::new(recorders);

    let decisions: Vec<RoundDecision> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..recorders)
            .map(|_| {
                scope.spawn(|| {
                    start.wait();
                    record_round(work_dir, STAGE, decide).unwrap()
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });

    let written: Vec<u32> = decisions
        .iter()
        .filter_map(|d| match d {
            RoundDecision::Record(round) => Some(round.round),
            RoundDecision::Duplicate(_) => None,
        })
        .collect();
    let duplicates = decisions
        .iter()
        .filter(|d| matches!(d, RoundDecision::Duplicate(1)))
        .count();
    assert_eq!(written, vec![1], "{decisions:?}");
    assert_eq!(duplicates, recorders - 1, "{decisions:?}");
    let rounds = load_rounds(work_dir, STAGE).unwrap();
    let recorded: Vec<(u32, &str)> = rounds
        .iter()
        .map(|r| (r.round, r.agent_id.as_str()))
        .collect();
    assert_eq!(recorded, vec![(1, AGENT)]);
}
