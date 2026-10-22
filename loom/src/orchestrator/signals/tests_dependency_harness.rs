//! Offline harness over three recorded dependency/successor pairs, with no
//! model calls: could the packs built from the dependency's git change set,
//! at best, carry enough of the dependency files the successor's sessions
//! read to cover the `## Dependency Findings` section's cost?
//!
//! The gain is an upper bound (reads are recorded per file, so any source item
//! of a file is credited with every read of it), never a saving. Each pair
//! prints its stop-rule verdict (PASS when the replay-weighted gain covers the
//! section cost) without asserting it; the asserts are the change-set
//! reproduction checks. Each test clones the repository and builds a source graph, so all
//! three are ignored by default: run with `--ignored --nocapture`.

#[path = "tests_dependency_harness/fixture.rs"]
mod fixture;
#[path = "tests_dependency_harness/packs.rs"]
mod packs;

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crate::context::retrieve::StageQuery;
use crate::context::schema::{estimate_tokens, ContextPack};
use crate::models::stage::Stage;

use super::generate::{
    build_dependency_findings, DEPENDENCY_FINDINGS_HEADING, MAX_DEPENDENCY_FINDINGS_BYTES,
};
use fixture::{Pair, Role, Scratch};
use packs::Coverage;

/// One arm's paths and the packs production would build with them.
struct Arm {
    paths: Vec<String>,
    stage: ContextPack,
    /// Per transcript, in fixture order; `None` for a transcript with no pack.
    workers: Vec<Option<ContextPack>>,
}

impl Arm {
    fn build(scratch: &Scratch, pair: &Pair, successor: &Stage, paths: Vec<String>) -> Arm {
        let stage = packs::stage_pack(&scratch.repo, &scratch.work_dir, successor, &paths);
        let workers = pair
            .transcripts
            .iter()
            .map(|transcript| {
                let spawn = transcript.spawn.as_ref()?;
                packs::gets_worker_pack(transcript).then(|| {
                    packs::worker_pack(&scratch.repo, &scratch.work_dir, successor, spawn, &paths)
                })
            })
            .collect();
        Arm {
            paths,
            stage,
            workers,
        }
    }

    /// The pack a transcript's session would have carried: the stage pack is
    /// shared by the main and contract sessions.
    fn pack_for(&self, index: usize, role: Role) -> Option<&ContextPack> {
        match role {
            Role::Main | Role::Contract => Some(&self.stage),
            Role::Worker => self.workers[index].as_ref(),
        }
    }

    fn coverage(&self, pair: &Pair) -> BTreeMap<Role, Coverage> {
        let mut by_role: BTreeMap<Role, Coverage> = Role::ALL
            .iter()
            .map(|role| (*role, Coverage::default()))
            .collect();
        for (index, transcript) in pair.transcripts.iter().enumerate() {
            if let Some(pack) = self.pack_for(index, transcript.role) {
                let covered = packs::coverage(pack, &transcript.reads);
                by_role.entry(transcript.role).or_default().add(covered);
            }
        }
        by_role
    }
}

fn total(by_role: &BTreeMap<Role, Coverage>) -> Coverage {
    let mut sum = Coverage::default();
    by_role.values().for_each(|coverage| sum.add(*coverage));
    sum
}

/// Both arms; asserts the new arm reproduces the recorded merge's change set.
fn arms(scratch: &Scratch, pair: &Pair, successor: &Stage) -> (Arm, Arm) {
    let old_paths = packs::old_paths(&scratch.work_dir, successor);
    let new_paths = StageQuery::dependency_paths(&scratch.work_dir, successor);
    let new_set: BTreeSet<&String> = new_paths.iter().collect();
    let recorded: BTreeSet<&String> = pair.changed_paths.iter().collect();
    assert_eq!(
        new_set, recorded,
        "{}: the change set does not reproduce the recorded merge",
        pair.pair
    );
    let old = Arm::build(scratch, pair, successor, old_paths);
    let new = Arm::build(scratch, pair, successor, new_paths);
    (old, new)
}

fn print_diff(label: &str, old: &ContextPack, new: &ContextPack) {
    let diff = packs::diff(old, new);
    println!(
        "pack {label}: {} items old, {} new; gained {:?}; lost {:?}; knowledge lost {:?}",
        old.items.len(),
        new.items.len(),
        diff.gained,
        diff.lost,
        diff.knowledge_lost
    );
}

fn print_diffs(pair: &Pair, old: &Arm, new: &Arm) {
    print_diff("stage (main, contract)", &old.stage, &new.stage);
    for (index, transcript) in pair.transcripts.iter().enumerate() {
        if let (Some(before), Some(after)) = (&old.workers[index], &new.workers[index]) {
            print_diff(&format!("worker {}", transcript.id), before, after);
        }
    }
}

/// The section signal generation would render with the new stage pack.
fn findings_section(scratch: &Scratch, successor: &Stage, pack: &ContextPack) -> String {
    // `retrieve_stage_pack` hands generation `None` for an empty pack.
    let pack = (!pack.items.is_empty()).then_some(pack);
    let section = build_dependency_findings(&scratch.work_dir, successor, pack)
        .expect("an accepted dependency with a journal renders a findings section");
    assert!(
        section.starts_with(&format!("## {DEPENDENCY_FINDINGS_HEADING}")),
        "{section}"
    );
    assert!(
        section.len() <= MAX_DEPENDENCY_FINDINGS_BYTES,
        "section is {} bytes, over the {MAX_DEPENDENCY_FINDINGS_BYTES}-byte cap",
        section.len()
    );
    section
}

/// Every number one pair reports.
struct Measurement {
    pair: String,
    changed_paths: usize,
    old_paths: usize,
    old: BTreeMap<Role, Coverage>,
    new: BTreeMap<Role, Coverage>,
    section_bytes: usize,
    section_tokens: u64,
    main_calls: u64,
    elapsed: Duration,
}

impl Measurement {
    fn cost(&self) -> u64 {
        self.section_tokens * self.main_calls
    }

    fn gain(&self) -> (i64, i64) {
        let (old, new) = (total(&self.old), total(&self.new));
        let first = new.first_arrival as i64 - old.first_arrival as i64;
        let replay = new.replay_weighted as i64 - old.replay_weighted as i64;
        (first, replay)
    }

    fn passes(&self) -> bool {
        self.gain().1 >= self.cost() as i64
    }

    fn verdict(&self) -> &'static str {
        if self.passes() {
            "stop rule PASS: the replay-weighted upper bound covers the section cost; a necessary condition, not a saving (the frozen-successor replay is the savings gate)"
        } else {
            "stop rule FAIL: the replay-weighted upper bound does not cover the section cost"
        }
    }

    fn coverage_lines(&self) -> String {
        let line = |c: &Coverage| {
            format!(
                "first-arrival {} replay-weighted {} files {} (only via stage-dependency {})",
                c.first_arrival, c.replay_weighted, c.files, c.via_dependency_only
            )
        };
        let mut lines = String::new();
        for role in Role::ALL {
            lines.push_str(&format!(
                "covered {role:?}: old {} | new {}\n",
                line(&self.old[&role]),
                line(&self.new[&role])
            ));
        }
        lines.push_str(&format!(
            "covered total: old {} | new {}\n",
            line(&total(&self.old)),
            line(&total(&self.new))
        ));
        lines
    }

    fn summary(&self) -> String {
        let (first, replay) = self.gain();
        format!(
            "== {} ==\nchanged paths {} (old arm {} paths)\n{}section {} bytes, {} tokens; main calls {}; cost {} tokens\nupper-bound gain: first-arrival {first}, replay-weighted {replay}\nverdict: {}\nwall time {:.1} s",
            self.pair,
            self.changed_paths,
            self.old_paths,
            self.coverage_lines(),
            self.section_bytes,
            self.section_tokens,
            self.main_calls,
            self.cost(),
            self.verdict(),
            self.elapsed.as_secs_f64()
        )
    }
}

fn run_pair(name: &str) -> Measurement {
    let started = Instant::now();
    let pair = fixture::load(name);
    let scratch = Scratch::prepare(&pair);
    scratch.build_graph();
    let successor = pair.successor.stage();
    let (old, new) = arms(&scratch, &pair, &successor);
    print_diffs(&pair, &old, &new);
    let section = findings_section(&scratch, &successor, &new.stage);
    let main_calls = pair
        .transcripts
        .iter()
        .filter(|transcript| transcript.role == Role::Main)
        .map(|transcript| transcript.api_calls)
        .sum();
    let measurement = Measurement {
        pair: pair.pair.clone(),
        changed_paths: pair.changed_paths.len(),
        old_paths: old.paths.len(),
        old: old.coverage(&pair),
        new: new.coverage(&pair),
        section_bytes: section.len(),
        section_tokens: estimate_tokens(&section) as u64,
        main_calls,
        elapsed: started.elapsed(),
    };
    println!("{}", measurement.summary());
    measurement
}

#[test]
#[ignore = "offline measurement over recorded runs; run with --ignored --nocapture"]
fn guard_core_guard_integration_is_reported() {
    run_pair("guard-core--guard-integration");
}

#[test]
#[ignore = "offline measurement over recorded runs; run with --ignored --nocapture"]
fn token_core_tui_tokens_is_reported() {
    run_pair("token-core--tui-tokens");
}

#[test]
#[ignore = "offline measurement over recorded runs; run with --ignored --nocapture"]
fn token_core_web_tokens_is_reported() {
    run_pair("token-core--web-tokens");
}
