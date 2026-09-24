//! Build the briefing an adjudication session is given for one dispute.
//!
//! It has two parts:
//! - `instructions`: what the session judges, and how it records the verdict.
//! - `evidence`: what it judges the dispute against.
//!
//! Each kind of dispute has its own builder: `criterion` (a disputed
//! acceptance criterion), `findings` (review findings), `contract` (a frozen
//! contract) and `integrity` (test-integrity events). [`build`] routes by the
//! request's kind.
//!
//! The whole thing is hard-capped to roughly 100 KiB (see `truncate`), with
//! the instructions never trimmed. `signals/adjudication.rs` wraps the result
//! in a signal file; nothing here writes anything.

mod contract;
mod criterion;
mod diff;
mod execution_site;
mod findings;
mod integrity;
mod sources;
mod truncate;

use std::path::Path;

use crate::models::dispute::{DisputeKind, DisputeRequest};
use crate::models::stage::Stage;

pub use execution_site::ExecutionSite;

/// Total briefing byte budget. The session's context window is far larger;
/// this cap exists so we never accidentally ship hundreds of KB of diff into
/// a signal file.
pub const MAX_PROMPT_BYTES: usize = 100_000;

/// The two halves of an assembled briefing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// The job and the verdict protocol. Never truncated.
    pub instructions: String,
    /// Everything the session judges the dispute against.
    pub evidence: String,
}

impl Prompt {
    /// Total byte length of the assembled briefing. Used by tests + the
    /// truncation pass to enforce [`MAX_PROMPT_BYTES`].
    pub fn total_len(&self) -> usize {
        self.instructions.len() + self.evidence.len()
    }

    /// The briefing as it appears in the signal file.
    pub fn render(&self) -> String {
        format!("{}\n{}", self.instructions, self.evidence)
    }
}

/// What every per-kind builder reads. The kind itself, with the disputed ids
/// and (for findings and integrity events) their snapshots, is
/// `request.kind`.
pub(crate) struct KindPromptInput<'a> {
    /// The disputed stage.
    pub stage: &'a Stage,
    /// The dispute's number (`request.id`).
    pub dispute_id: u32,
    /// The dispute as filed.
    pub request: &'a DisputeRequest,
    /// Where the stage runs its criteria and contracts: the worktree root
    /// joined with `working_dir`, or the repository root once the worktree
    /// is gone.
    pub site: &'a ExecutionSite,
    /// The stage's worktree root; `None` once it is gone from disk.
    pub worktree: Option<&'a Path>,
    /// The `.loom/work/` root.
    pub work_dir: &'a Path,
}

impl KindPromptInput<'_> {
    /// `## Recording your verdict` for this dispute: where the draft goes,
    /// `schema` as step 1's body, and the `loom stage adjudicate` step.
    /// `schema` spells the JSON out in full — a fenced literal example with
    /// every field and its allowed values, never a Rust type name — and ends
    /// with a blank line.
    fn verdict_protocol(&self, schema: &str) -> String {
        let (stage_id, dispute_id) = (&self.stage.id, self.dispute_id);
        let draft = super::verdict_draft_file(self.work_dir, stage_id, dispute_id);
        let mut s = String::new();
        s.push_str("## Recording your verdict\n\n");
        s.push_str(&draft_location(dispute_id, &draft));
        s.push_str(
            "1. Write a SINGLE JSON object — no prose, no markdown fences, no comments —\n   to the draft file. Schema:\n\n",
        );
        s.push_str(schema);
        s.push_str("2. Run:\n\n");
        s.push_str("```bash\n");
        s.push_str(&format!(
            "loom stage adjudicate --stage {stage_id} --dispute {dispute_id} --verdict-file <draft file>\n"
        ));
        s.push_str("```\n\n");
        s.push_str("The command validates the JSON and hands the verdict to the orchestrator,\n");
        s.push_str(
            "which applies it on its next poll. If it prints a PENDING RELAY notice, keep\n",
        );
        s.push_str("its output unfiltered and in the foreground and follow that notice. If it\n");
        s.push_str("reports an error, correct the JSON and run it again. Once it succeeds, your\n");
        s.push_str("work is done — end your turn. The daemon closes this session once the\n");
        s.push_str("verdict is applied.\n\n");
        s
    }
}

/// Build the briefing for the supplied dispute, routed by its kind.
///
/// `plan_path` is the live plan markdown (a criterion briefing quotes the
/// stage's acceptance criteria as the plan states them). `work_dir` is the
/// `.loom/work/` root, used to resolve the repository, the stage's worktree
/// and the draft file the session writes its JSON verdict to before handing
/// it to `loom stage adjudicate`.
///
/// Total: every source a builder reads degrades to a message in place rather
/// than failing, because a briefing missing one section is still usable and a
/// dispute with no briefing at all is not.
pub fn build(plan_path: &Path, stage: &Stage, dispute: &DisputeRequest, work_dir: &Path) -> Prompt {
    let site = ExecutionSite::resolve(work_dir, stage);
    let input = KindPromptInput {
        stage,
        dispute_id: dispute.id,
        request: dispute,
        site: &site,
        worktree: site.worktree(),
        work_dir,
    };
    let mut prompt = match &dispute.kind {
        DisputeKind::Criterion { criterion_index } => {
            criterion::build(&input, plan_path, *criterion_index)
        }
        DisputeKind::Findings { .. } => findings::build(&input),
        DisputeKind::Contract { .. } => contract::build(&input),
        DisputeKind::Integrity { .. } => integrity::build(&input),
    };
    truncate::truncate_to_budget(&mut prompt);
    prompt
}

/// Where the draft goes. A session started with a scratch directory may write
/// it nowhere else, and one started without has only the legacy path, so the
/// briefing names both and the session's own environment picks.
fn draft_location(dispute_id: u32, verdict_draft: &Path) -> String {
    let scratch = format!(
        "$LOOM_SCRATCH_DIR/{}",
        super::scratch_verdict_file_name(dispute_id)
    );
    format!(
        "First find your draft file. Run `printf '%s\\n' \"${{LOOM_SCRATCH_DIR:-}}\"`:\n\n\
         - it prints a directory: the draft file is `{scratch}` (that directory\n  \
         joined with the file name), the only place this session may write it;\n\
         - it prints nothing: the draft file is `{}`.\n\n",
        verdict_draft.display()
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_golden;

#[cfg(test)]
mod tests_kinds;
