//! Tests for the spawn-prompt agent-type classifier and for the `<synthetic>`
//! model-sentinel filtering used when selecting a parent model and grouping
//! by agent/model. Split out to keep `agents.rs` legible (CLAUDE.md Rule 17).

use super::*;
use crate::commands::usage::transcript::{Entry, Request, TokenUsage, UserEntry};

fn user_entry(text: &str) -> UserEntry {
    UserEntry {
        timestamp: chrono::Utc::now(),
        tool_use_id: None,
        text: text.to_owned(),
    }
}

fn request(model: &str) -> Request {
    Request {
        message_id: None,
        timestamp: chrono::Utc::now(),
        model: model.to_owned(),
        usage: TokenUsage::default(),
        tool_uses: Vec::new(),
        thinking_chars: 0,
        text_chars: 0,
    }
}

fn transcript(scope: Scope, session_id: &str, prompt: Option<&str>, models: &[&str]) -> Transcript {
    Transcript {
        path: std::path::PathBuf::from("test.jsonl"),
        scope,
        project_slug: "project".to_owned(),
        session_id: session_id.to_owned(),
        agent_id: Some("agent-1".to_owned()),
        agent_type: None,
        first_user_entry: prompt.map(user_entry),
        entries: models
            .iter()
            .map(|m| Entry::Assistant(request(m)))
            .collect(),
    }
}

/// The hook ledger records what Claude actually started. It must win over a
/// prompt that happens to quote a different known type in its prose.
#[test]
fn authoritative_ledger_type_wins_over_prompt_inference() {
    let mut transcript = transcript(
        Scope::Subagent,
        "session-1",
        Some("Ask loom-software-engineer to review this later."),
        &[],
    );
    transcript.agent_type = Some("loom-senior-software-engineer".to_owned());

    assert_eq!(agent_type(&transcript), "loom-senior-software-engineer");
}

// --- Defect 1: agent-type classification ------------------------------

/// The exact scenario that used to mislabel every senior-engineer spawn: the
/// prompt carries CLAUDE.md's Rule 6c coordinator preamble, which writes
/// "(loom-software-engineer = sonnet)" into every coordinator prompt
/// regardless of the coordinator's real type. That mention alone must never
/// be reported as `loom-software-engineer`.
#[test]
fn coordinator_boilerplate_alone_is_not_reported_as_the_sonnet_tier() {
    let prompt = concat!(
        "COORDINATOR ROLE - YOU ARE A SUBAGENT COORDINATING WORKERS (ONE LEVEL ONLY):\n",
        "- Spawn workers via the Task tool BY AGENT TYPE ",
        "(loom-software-engineer = sonnet); include the WORKER PREAMBLE as the ",
        "first lines of EVERY worker prompt\n",
    );
    let real_senior_engineer = transcript(Scope::Subagent, "session-1", Some(prompt), &[]);
    // Pin the exact outcome, not just "anything but the wrong answer": with
    // no other identifying text in the prompt, `unknown` is the only correct
    // answer a text-only classifier can give here (see `agent_type`'s doc).
    assert_eq!(agent_type(&real_senior_engineer), "unknown");
}

/// A plain, non-annotated mention still resolves normally -- the cost-
/// annotation exclusion must not swallow the ordinary case.
#[test]
fn plain_mention_without_cost_annotation_is_identified() {
    let transcript = transcript(
        Scope::Subagent,
        "session-1",
        Some("You are spawned as loom-software-engineer for this task."),
        &[],
    );
    assert_eq!(agent_type(&transcript), "loom-software-engineer");
}

/// The senior-engineer name is identified correctly on its own, with no
/// shorter name present to interfere.
#[test]
fn senior_engineer_name_is_recognized_on_its_own() {
    let transcript = transcript(
        Scope::Subagent,
        "session-1",
        Some("You are spawned as loom-senior-software-engineer for this task."),
        &[],
    );
    assert_eq!(agent_type(&transcript), "loom-senior-software-engineer");
}

/// Two genuinely distinct type names both present (e.g. a brief that quotes
/// more than one agent from the Rule 7 table) is real ambiguity: guessing
/// either one is worse than admitting the type is unknown.
#[test]
fn two_distinct_type_mentions_are_reported_unknown_rather_than_guessed() {
    let transcript = transcript(
        Scope::Subagent,
        "session-1",
        Some("Escalate to loom-advisor if this loom-code-reviewer pass finds nothing."),
        &[],
    );
    assert_eq!(agent_type(&transcript), "unknown");
}

/// Delimiter anchoring: `loom-advisor` must not match as a prefix of the
/// longer, unrelated word `loom-advisory`.
#[test]
fn delimiter_anchoring_rejects_a_longer_identifier() {
    let transcript = transcript(
        Scope::Subagent,
        "session-1",
        Some("This is a loom-advisory-council decision, not one of the known agent types."),
        &[],
    );
    assert_eq!(agent_type(&transcript), "unknown");
}

/// `Explore` is deliberately not a candidate at all (see `KNOWN_AGENT_TYPES`'s
/// doc): ordinary task prose using the word must not be misread as an
/// identity mention, and must not collapse an otherwise-unambiguous mention
/// elsewhere in the same prompt down to `unknown`.
#[test]
fn ordinary_use_of_the_word_explore_does_not_interfere() {
    let transcript = transcript(
        Scope::Subagent,
        "session-1",
        Some("Explore the codebase first, then implement as a loom-software-engineer."),
        &[],
    );
    assert_eq!(agent_type(&transcript), "loom-software-engineer");
}

// --- Defect 2: synthetic model rows ------------------------------------

/// A parent session whose first request is a synthetic API-error row must
/// not latch `<synthetic>` as its model -- the first real model should win,
/// and a subagent's own real-model request should then score as a match.
#[test]
fn parent_models_skips_a_synthetic_first_request() {
    let main = transcript(
        Scope::Main,
        "session-1",
        None,
        &[SYNTHETIC_MODEL, "claude-sonnet-5"],
    );
    let parents = parent_models(&[main]);
    assert_eq!(
        parents.get("session-1").map(String::as_str),
        Some("claude-sonnet-5")
    );

    let sub = transcript(Scope::Subagent, "session-1", None, &["claude-sonnet-5"]);
    let subs: Vec<&Transcript> = vec![&sub];
    let matches = parent_matches(&subs, &parents);
    assert_eq!(matches.same, 1);
    assert_eq!(matches.different, 0);
}

/// A synthetic request inside a subagent transcript must not create a
/// phantom `<synthetic>` row in the agent/model breakdown.
#[test]
fn by_agent_model_excludes_synthetic_rows_from_grouping() {
    let sub = transcript(
        Scope::Subagent,
        "session-1",
        Some("You are spawned as loom-software-engineer for this task."),
        &[SYNTHETIC_MODEL, "claude-sonnet-5"],
    );
    let subs: Vec<&Transcript> = vec![&sub];
    let rows = by_agent_model(&subs);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].model, "claude-sonnet-5");
    assert_eq!(rows[0].requests, 1);
}

/// The under-500-output display also skips a synthetic first request when
/// picking which model to show for a tiny subagent.
#[test]
fn tiny_subagents_model_skips_a_synthetic_first_request() {
    let sub = transcript(
        Scope::Subagent,
        "session-1",
        None,
        &[SYNTHETIC_MODEL, "claude-sonnet-5"],
    );
    let subs: Vec<&Transcript> = vec![&sub];
    let tiny = tiny_subagents(&subs);
    assert_eq!(tiny.len(), 1);
    assert_eq!(tiny[0].model, "claude-sonnet-5");
}
