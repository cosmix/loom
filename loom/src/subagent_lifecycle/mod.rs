//! Authoritative worker lifecycle evidence shared by Claude and Codex adapters.
//!
//! ## Hook-written JSONL contract (schema version 1)
//!
//! `subagent-stop.sh` writes exactly one compact JSON object plus `\n`:
//! `{"version":1,"event_id":"sha256:<hex>","producer":"claude_subagent_stop",`
//! `"identity":{"kind":"claude_subagent","stage_id":"...",`
//! `"loom_session_id":"...","parent_session_id":"...","agent_id":"...",`
//! `"agent_type":"...","transcript_path":"/absolute/.../agent-<id>.jsonl"},`
//! `"observed_at":"<RFC3339 UTC>","state":"completed",`
//! `"evidence":{"transcript_bytes":<decimal>,`
//! `"final_record_sha256":"sha256:<lowercase hex>"}}`.
//!
//! `teammate-idle.sh` writes exactly
//! `{"version":1,"event_id":"sha256:<hex>","producer":"claude_teammate_idle",`
//! `"identity":{"kind":"claude_teammate","stage_id":"...",`
//! `"loom_session_id":"...","parent_session_id":"...","team_name":"...",`
//! `"teammate_name":"..."},"observed_at":"<RFC3339 UTC>","state":"idle",`
//! `"evidence":{"transcript_path":"/absolute/<parent UUID>.jsonl",`
//! `"transcript_bytes":<decimal>,"final_record_sha256":"sha256:<lowercase hex>"}}`
//! plus `\n`. The evidence describes the parent transcript. Idle is nonterminal.
//!
//! A transcript digest is SHA-256 over the exact final, newline-terminated JSONL
//! record (blank trailing records are rejected), excluding its line ending,
//! rendered as `sha256:<lowercase hex>`.
//! `transcript_bytes` is the exact `wc -c` byte count after that newline exists.
//! The stop event canonical byte string is emitted by Bash with:
//! `printf 'loom.lifecycle.claude_subagent_stop.v1\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s'`
//! and arguments, in order: stage id, Loom session id, Claude parent UUID,
//! agent id, agent type, absolute normalized worker transcript path, decimal
//! transcript byte count, and final-record digest. The idle canonical string
//! is emitted with
//! `printf 'loom.lifecycle.claude_teammate_idle.v1\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s'`.
//! Its arguments are: stage id, Loom session id, Claude parent UUID, team name,
//! teammate name, absolute
//! normalized parent transcript path, decimal byte count, final-record digest,
//! and observed-at normalized as UTC with exactly three fractional digits
//! (`YYYY-MM-DDTHH:MM:SS.mmmZ`). `event_id` is `sha256:` followed by lowercase
//! `sha256sum` output over those exact canonical bytes.
//!
//! Both hooks append to
//! `$LOOM_WORK_DIR/subagents/<stage_id>/lifecycle.jsonl`. They serialize with
//! the sibling directory lock `lifecycle.jsonl.lock`: prepare a mode-0600
//! same-directory claim containing `pid=<decimal>\ncreated=<unix-seconds>\n`,
//! atomically `mkdir -m 700` the lock, hard-link the claim to `<lock>/owner`,
//! then remove the claim. A contender retries for at most two seconds. A lock
//! at least 30 seconds old may be reclaimed when it is still empty (using the
//! directory mtime), or when `owner` is a plain, well-formed file and its PID is
//! no longer alive; malformed or live ownership fails closed. The owner removes
//! only its own plain `owner` and then `rmdir`s the lock. Descendants of the
//! resolved work root and the journal leaf must be plain, never symlinked.
//!
//! Replay does not trust hook validation. It rechecks the schema/version,
//! producer/identity/state agreement, safe exact identifiers, current
//! stage-to-Loom-session ownership, exact four-part SubagentStart join, parent
//! and worker transcript layout, regular no-symlink paths, RFC3339 timestamps,
//! transcript byte length, final-record JSON and digest, and deterministic event
//! id. Transcript growth invalidates an earlier stop. Codex replay likewise
//! checks requested model/effort, invocation and job/thread/turn identifiers,
//! terminal timestamp, bounded outcome, and authorization-to-terminal identity.
//! Unknown versions, conflicts, malformed complete lines, stale records, and
//! missing evidence remain [`WorkerOutcome::Unknown`].
//!
//! Codex adapters compute their event id over the NUL-separated canonical
//! string beginning `loom.lifecycle.codex.v1`, followed by producer, stage,
//! Loom session, Claude parent UUID, forwarder, unit, invocation, canonical
//! workspace, `companion:<job>` or `direct:<thread>:<tool-use>`, state,
//! requested model, requested effort, evidence kind, turn id (or empty),
//! terminal RFC3339 (or empty), and bounded outcome. Use
//! `store::codex_event_id`; replay recomputes it.

pub mod claude;
mod claude_event;
mod claude_files;
mod codex_evidence;
mod lock;
pub mod model;
pub mod store;
mod validation;

pub use claude::{
    validate_subagent_stop, validate_teammate_idle, ActiveStageSession, ClaudeEnvironment,
    ClaudeEvidenceError, ClaudeStartEvidence, ClaudeStarts,
};
pub use model::{
    CodexEvidence, CodexEvidenceKind, CodexEvidenceOutcome, CodexExecution, LifecycleProducer,
    LifecycleRecord, LifecycleState, WorkerIdentity, WorkerOutcome, LIFECYCLE_VERSION,
};
pub use store::{replay, AppendOutcome, LifecycleIndex};
// `commands::hook::review_harvest` checks a transcript path the same way.
pub(crate) use claude_files::reject_symlink_components;

#[cfg(test)]
mod tests;
