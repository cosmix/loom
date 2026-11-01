//! Flattening of the untrusted free-form strings a status payload carries.
//!
//! A [`StageSummary`] is broadcast to every `loom status` subscriber once a
//! second, rendered into terminal cells by the ledger TUI and written to stdout
//! by the static renderer. Several of its strings were never written by loom:
//! `model` and `execution_models` come from the `model` field of a spawn ledger
//! row, which the spawn hook records verbatim from the caller's tool input;
//! `last_tool` and `last_activity` come from heartbeat JSON; `review_reason`
//! carries adjudication and human-review text; `cleanup_warning` carries git's
//! own error text; and `failure_info.evidence` is a crashed session's stderr.
//!
//! Emitted raw, an ESC in any of them is an ANSI sequence the operator's
//! terminal obeys, and a bidi override reverses everything a reader sees after
//! it. The renderers cannot be where that is stopped: they budget by DISPLAY
//! WIDTH, which is zero for precisely those characters, so a width-bounded
//! column bounds nothing. Flattening here, at the collector boundary, covers
//! the ledger TUI, the static renderer and the wire in one place.

use super::{BlockDetail, StageSummary};
use crate::context::untrusted::{inline_safe, multiline_safe, whole_safe};

#[path = "sanitize_token_flow.rs"]
mod token_flow;
pub(super) use token_flow::{flow_is_fresh, sanitize_token_flow};

/// Longest close reason a status payload carries.
///
/// A reason is the whole message a stage agent leaves with `loom stage block`,
/// and the operator acts on it, so it is not cut to the inline limit. Four
/// thousand characters holds several paragraphs and still bounds what one
/// stage contributes to the shared payload.
const MAX_CLOSE_REASON_CHARS: usize = 4000;

/// Last line of a close reason a cap cut short, naming where the rest is.
const CLOSE_REASON_TRUNCATED_MARKER: &str =
    "... reason truncated; see the stage file for the full text";

/// Sanitize display fields on a stage summary, in place. When adding a display
/// field, choose its whole, multiline, or bounded presentation here. Identity
/// fields such as IDs, dependencies and branch names must remain unchanged.
///
/// Structured, enumerated and numeric fields are left alone: they cannot carry
/// a control character in the first place. `incoherence` is included because it
/// is not the fixed string it looks like — `orchestrator::coherence` wraps
/// fixed prose around the stage's `session` pointer and a session record's
/// `stage_id`, both read from unvalidated frontmatter. `merge_resolver_session`
/// is likewise a session record's `id`, and the attention note prints it.
/// `close_reason` is free text a stage agent writes with `loom stage block`; it
/// keeps its line breaks and is bounded by [`bound_close_reason`]. Acceptance
/// criteria, failure evidence and the block detail are read in full on the
/// dashboard, so they go through [`neutralize`], which cuts nothing.
pub(super) fn sanitize_stage_summary(summary: &mut StageSummary) {
    neutralize(&mut summary.name);
    if let Some(description) = summary.description.as_mut() {
        *description = description
            .split('\n')
            .map(whole_safe)
            .collect::<Vec<_>>()
            .join("\n");
    }
    flatten(&mut summary.model);
    summary.execution_models.iter_mut().for_each(flatten);
    summary.last_tool.iter_mut().for_each(flatten);
    summary.last_activity.iter_mut().for_each(flatten);
    summary.review_reason.iter_mut().for_each(flatten);
    flatten_multiline(&mut summary.summary);
    flatten_multiline(&mut summary.review_notes);
    summary.cleanup_warning.iter_mut().for_each(flatten);
    summary.merge_block.iter_mut().for_each(flatten);
    summary.stash_warning.iter_mut().for_each(flatten);
    summary.incoherence.iter_mut().for_each(flatten);
    summary.merge_resolver_session.iter_mut().for_each(flatten);
    bound_close_reason(&mut summary.close_reason);
    summary.acceptance.iter_mut().for_each(neutralize);
    summary
        .terminal_history_session
        .iter_mut()
        .for_each(flatten);
    sanitize_block_detail(&mut summary.block_detail);
    if let Some(blocker) = summary.completion_blocker.as_mut() {
        flatten(&mut blocker.failure_code);
        blocker.summary.iter_mut().for_each(flatten);
        flatten(&mut blocker.next_action);
    }
    if let Some(failure) = summary.failure_info.as_mut() {
        failure.evidence.iter_mut().for_each(neutralize);
    }
}

/// The criteria commands come from files a stage agent can
/// influence, and the card prints them.
fn sanitize_block_detail(detail: &mut BlockDetail) {
    for criterion in &mut detail.failing_criteria {
        neutralize(&mut criterion.command);
        criterion.diagnostic.iter_mut().for_each(neutralize);
    }
}

/// Flatten an identifier-like value to one line of at most `MAX_INLINE_CHARS`.
fn flatten(value: &mut String) {
    *value = inline_safe(value);
}

/// Neutralize a value the dashboard shows whole ([`whole_safe`]): each
/// control, line-shaped, bidi and zero-width format character becomes a
/// space, a run of combining marks keeps its first
/// [`MAX_STACKED_MARKS`](crate::context::untrusted::MAX_STACKED_MARKS), and
/// nothing else is cut, so a 400-character evidence line or criterion arrives
/// as it was written. Combining marks are zero-width too: the run bound is
/// what keeps one base character from carrying a flood of them into a
/// width-budgeted terminal column and every frame of the payload. Marks
/// outside the ranges `whole_safe` knows pass unbounded.
///
/// Evidence lists are shipped whole as well. Revisit with a cap that leaves a
/// visible marker if one stage's evidence makes the status frame too large for
/// the dashboard's refresh.
pub(super) fn neutralize(value: &mut String) {
    *value = whole_safe(value);
}

/// Neutralize a value the dashboard renders as a block, keeping its lines.
/// A value that is blank afterwards becomes `None` so the web never gets `""`.
fn flatten_multiline(value: &mut Option<String>) {
    *value = value
        .as_deref()
        .map(multiline_safe)
        .filter(|text| !text.trim().is_empty());
}

/// Neutralize a close reason as [`flatten_multiline`] does, then cut it to
/// [`MAX_CLOSE_REASON_CHARS`], ending a cut reason with
/// [`CLOSE_REASON_TRUNCATED_MARKER`] on a line of its own.
fn bound_close_reason(value: &mut Option<String>) {
    flatten_multiline(value);
    let Some(reason) = value.as_mut() else {
        return;
    };
    if let Some((cut, _)) = reason.char_indices().nth(MAX_CLOSE_REASON_CHARS) {
        reason.truncate(cut);
        reason.truncate(reason.trim_end().len());
        reason.push('\n');
        reason.push_str(CLOSE_REASON_TRUNCATED_MARKER);
    }
}

/// Whether a stage id may be joined into a path.
///
/// Stage ids reach the filesystem in several collectors — a heartbeat file, a
/// subagent ledger — and every one of them reads an id straight out of a stage
/// file's frontmatter. An id carrying `../` would make the daemon read an
/// arbitrary file and surface its strings in the payload.
///
/// `.` is rejected alongside `..`: joined onto the subagents directory it names
/// that directory itself, which is not a stage, and both shell spawn guards
/// (`loom-hooks/spawn-guard.sh`, `loom-hooks/codex-forward.sh`) reject it explicitly.
pub(super) fn valid_stage_id(stage_id: &str) -> bool {
    !stage_id.is_empty()
        && stage_id != "."
        && !stage_id.contains("..")
        && stage_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::status::data::{ActivityStatus, StageStatus};
    use crate::context::untrusted::MAX_INLINE_CHARS;
    use crate::models::failure::{FailureInfo, FailureType};
    use chrono::Utc;

    pub(super) fn summary() -> StageSummary {
        StageSummary {
            id: "stage-1".to_string(),
            name: "Stage One".to_string(),
            status: StageStatus::Executing,
            activity_status: ActivityStatus::Working,
            model: "sonnet".to_string(),
            ..Default::default()
        }
    }

    pub(super) fn failure_with(evidence: Vec<String>) -> FailureInfo {
        FailureInfo {
            failure_type: FailureType::SessionCrash,
            detected_at: Utc::now(),
            evidence,
        }
    }

    #[test]
    fn blank_summary_and_review_notes_become_none() {
        let mut stage = summary();
        stage.summary = Some("  \n ".to_string());
        stage.review_notes = Some("  \n ".to_string());

        sanitize_stage_summary(&mut stage);

        assert_eq!(stage.summary, None);
        assert_eq!(stage.review_notes, None);
    }

    #[test]
    fn an_ansi_sequence_in_the_model_name_is_flattened() {
        let mut stage = summary();
        stage.model = "\u{1b}[2J\u{1b}[31msonnet".to_string();

        sanitize_stage_summary(&mut stage);

        assert!(!stage.model.contains('\u{1b}'));
        assert_eq!(stage.model, "[2J [31msonnet");
    }

    #[test]
    fn a_zero_width_flood_is_bounded_to_the_inline_limit() {
        let mut stage = summary();
        // Zero-width characters cost the width-based column budget nothing, so
        // 10,000 of them fit a 16-cell column intact without this.
        stage.last_activity = Some("a\u{200B}".repeat(10_000));

        sanitize_stage_summary(&mut stage);

        let activity = stage.last_activity.unwrap();
        assert!(!activity.contains('\u{200B}'));
        assert_eq!(activity.chars().count(), MAX_INLINE_CHARS);
    }

    #[test]
    fn a_bidi_override_in_evidence_is_flattened() {
        let mut stage = summary();
        stage.failure_info = Some(failure_with(vec![
            "build failed\u{202E}dessap stset lla".to_string()
        ]));

        sanitize_stage_summary(&mut stage);

        let evidence = &stage.failure_info.unwrap().evidence;
        assert_eq!(evidence, &["build failed dessap stset lla"]);
    }

    #[test]
    fn a_control_sequence_in_the_merge_resolver_session_is_flattened() {
        let mut stage = summary();
        stage.merge_resolver_session = Some("resolver\u{1b}[2J\u{202E}1".to_string());

        sanitize_stage_summary(&mut stage);

        let session = stage.merge_resolver_session.unwrap();
        assert!(!session.contains('\u{1b}'));
        assert!(!session.contains('\u{202E}'));
    }

    #[test]
    fn a_control_sequence_in_the_close_reason_is_flattened() {
        let mut stage = summary();
        stage.close_reason = Some("blocked\u{1b}[2J\u{202E}now".to_string());

        sanitize_stage_summary(&mut stage);

        let reason = stage.close_reason.unwrap();
        assert!(!reason.contains('\u{1b}'));
        assert!(!reason.contains('\u{202E}'));
    }

    #[test]
    fn review_notes_keep_their_lines_while_review_reason_stays_bounded() {
        let mut stage = summary();
        let reason = format!("first line\nsecond line\n{}", "x".repeat(300));
        stage.review_reason = Some(reason.clone());
        stage.review_notes = Some(reason.clone());
        stage.summary = Some("Builds the\nthing".to_string());

        sanitize_stage_summary(&mut stage);

        assert_eq!(stage.review_notes.as_deref(), Some(reason.as_str()));
        assert_eq!(stage.summary.as_deref(), Some("Builds the\nthing"));
        let flat = stage.review_reason.unwrap();
        assert!(!flat.contains('\n'));
        assert_eq!(flat.chars().count(), MAX_INLINE_CHARS);
    }

    #[test]
    fn ordinary_values_pass_through_unchanged() {
        let mut stage = summary();
        stage.execution_models = vec!["opus".to_string(), "terra".to_string()];
        stage.last_tool = Some("Bash".to_string());
        stage.cleanup_warning = Some("worktree still on disk".to_string());

        sanitize_stage_summary(&mut stage);

        assert_eq!(stage.model, "sonnet");
        assert_eq!(stage.execution_models, ["opus", "terra"]);
        assert_eq!(stage.last_tool.as_deref(), Some("Bash"));
        assert_eq!(
            stage.cleanup_warning.as_deref(),
            Some("worktree still on disk")
        );
    }

    #[test]
    fn stage_name_is_safe_and_keeps_identity_and_unicode() {
        let mut stage = summary();
        stage.id = "stage-1".into();
        stage.name = "Δοκιμή \u{1b}]52;c;payload\u{7} \u{1b}[31m\u{202e}終".into();
        stage.description = Some("Description \u{1b}[31m".into());
        sanitize_stage_summary(&mut stage);
        assert_eq!(stage.id, "stage-1");
        assert!(stage.name.contains("Δοκιμή"));
        assert!(stage.name.contains('終'));
        assert!(!stage.name.contains(['\u{1b}', '\u{7}', '\u{202e}']));
        assert!(!stage.description.unwrap().contains('\u{1b}'));
    }

    #[test]
    fn stage_ids_that_could_escape_a_directory_are_rejected() {
        assert!(valid_stage_id("stage-1.a_b"));
        assert!(!valid_stage_id(""));
        assert!(!valid_stage_id("."));
        assert!(!valid_stage_id(".."));
        assert!(!valid_stage_id("a/../b"));
        assert!(!valid_stage_id("a..b"));
        assert!(!valid_stage_id("bad/stage"));
    }
}

#[cfg(test)]
#[path = "sanitize_close_reason_tests.rs"]
mod close_reason_tests;

#[cfg(test)]
#[path = "sanitize_whole_text_tests.rs"]
mod whole_text_tests;
