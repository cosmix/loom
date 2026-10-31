//! Keeping a briefing inside [`MAX_PROMPT_BYTES`](super::MAX_PROMPT_BYTES).
//!
//! The evidence sections are trimmed in order of expendability — diff first,
//! then a contract's quoted files, the failure output, the listing, then the
//! plan excerpt — and the instructions are never touched: a session that
//! cannot see the whole diff can still go and read the tree, but a session
//! that has lost half its protocol has no way back.

use super::contract::{CURRENT_CONTENT_HEADING, DIFF_HEADING, FROZEN_CONTENT_HEADING};
use super::{Prompt, MAX_PROMPT_BYTES};

pub(super) const TRUNCATION_MARKER: &str = "\n... [truncated] ...\n";

/// Enforce [`MAX_PROMPT_BYTES`] by truncating the diff / contract-content /
/// failure-output sections of the evidence first, then the listing, then the
/// plan excerpt.
pub(super) fn truncate_to_budget(prompt: &mut Prompt) {
    if prompt.total_len() <= MAX_PROMPT_BYTES {
        return;
    }
    // Strategy: repeatedly halve the diff fences first, then the contract
    // content fences, the failure output fence, the listing fence, and the
    // plan excerpt.
    let candidates = [
        super::impact::STAGE_DIFF_HEADING,
        "## Evidence commit diff (git show)",
        // A contract briefing quotes the contract file twice and diffs it once;
        // these go before the frozen-set section at its tail is cut.
        DIFF_HEADING,
        FROZEN_CONTENT_HEADING,
        CURRENT_CONTENT_HEADING,
        "## Failure output (what the criterion produced)",
        "## Worktree top-level files (3-deep listing)",
        "## Plan acceptance criteria source (from plan file)",
    ];
    for header in candidates {
        while prompt.total_len() > MAX_PROMPT_BYTES {
            if !halve_section(&mut prompt.evidence, header) {
                break;
            }
        }
        if prompt.total_len() <= MAX_PROMPT_BYTES {
            return;
        }
    }
    // Hard cap: if every section was already trimmed and we still
    // exceed budget, truncate the evidence tail.
    if prompt.total_len() > MAX_PROMPT_BYTES {
        let allowed = MAX_PROMPT_BYTES.saturating_sub(prompt.instructions.len());
        truncate_string(&mut prompt.evidence, allowed);
    }
}

/// Find a section outside quoted fences and halve its first fenced body.
/// Returns `false` if no further
/// trimming is possible (no header, no fence, or fence already empty).
fn halve_section(evidence: &mut String, header: &str) -> bool {
    let Some((body_start, body_end)) = section_body(evidence, header) else {
        return false;
    };
    let body_len = body_end - body_start;
    // Keep the first half; replace the rest with the marker.
    let keep = body_len / 2;
    let mut new_body = String::with_capacity(keep + TRUNCATION_MARKER.len());
    let kept_slice = utf8_safe_prefix(&evidence[body_start..body_end], keep);
    new_body.push_str(kept_slice);
    if !new_body.ends_with('\n') {
        new_body.push('\n');
    }
    new_body.push_str(TRUNCATION_MARKER);
    // Refuse to "halve" when the marker would make the new body the same
    // size as (or larger than) the original — otherwise the outer trim
    // loop spins forever once the body shrinks near `TRUNCATION_MARKER.len()`.
    if new_body.len() >= body_len {
        return false;
    }
    evidence.replace_range(body_start..body_end, &new_body);
    true
}

/// Recognize complete delimiters on their own lines and ignore headings inside them.
fn section_body(evidence: &str, header: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    let mut fence = 0;
    let mut selected = false;
    let mut body_start = None;
    for line in evidence.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        let ticks = content.bytes().take_while(|b| *b == b'`').count();
        if fence > 0 {
            if ticks >= fence && content[ticks..].trim().is_empty() {
                if let Some(start) = body_start {
                    return Some((start, offset));
                }
                fence = 0;
            }
        } else if ticks >= 3 {
            fence = ticks;
            if selected {
                body_start = Some(offset + line.len());
            }
        } else if content.starts_with("## ") {
            if selected {
                return None;
            }
            selected = content == header;
        }
        offset += line.len();
    }
    None
}

/// Hard-truncate `s` to at most `max_bytes` bytes, respecting UTF-8
/// boundaries and leaving a trailing truncation marker so a downstream
/// reader sees the cut.
fn truncate_string(s: &mut String, max_bytes: usize) {
    if s.len() <= max_bytes {
        return;
    }
    let allowance = max_bytes.saturating_sub(TRUNCATION_MARKER.len());
    let prefix = utf8_safe_prefix(s, allowance).to_string();
    *s = prefix;
    s.push_str(TRUNCATION_MARKER);
}

fn utf8_safe_prefix(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    let mut idx = max_bytes;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    &s[..idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_safe_prefix_does_not_split_multibyte() {
        let s = "héllo wörld";
        // 1 byte past the start of 'é' (UTF-8 2-byte char) — must back up.
        let prefix = utf8_safe_prefix(s, 2);
        assert!(s.starts_with(prefix));
        assert_eq!(prefix.len(), 1, "got: {prefix:?}");
    }

    #[test]
    fn halve_section_shrinks_diff_fence() {
        let mut u = String::from("## Evidence commit diff (git show)\n\n```diff\n");
        for _ in 0..1000 {
            u.push_str("- old\n+ new\n");
        }
        u.push_str("```\n");
        let before = u.len();
        assert!(halve_section(&mut u, "## Evidence commit diff (git show)"));
        assert!(u.len() < before);
        assert!(u.contains(TRUNCATION_MARKER));
    }

    #[test]
    fn quoted_headings_and_shorter_fences_do_not_redirect_truncation() {
        let header = "## Diff (frozen vs current)";
        let quoted = format!("{header}\n```\n{}", "quoted\n".repeat(100));
        let target = format!(
            "{}\n```\n{}",
            "target\n".repeat(5000),
            "tail\n".repeat(5000)
        );
        let mut prompt = Prompt {
            instructions: "instructions".to_string(),
            evidence: format!(
                "## Earlier\n\n{}{header}\n\n{}## Remaining\n\npreserved\n",
                super::super::fenced("text", &quoted),
                super::super::fenced("diff", &target)
            ),
        };
        // Force the priority pass without hard-cutting the tail.
        prompt
            .instructions
            .push_str(&"i".repeat(MAX_PROMPT_BYTES - prompt.total_len() + 100));
        let instructions = prompt.instructions.clone();
        truncate_to_budget(&mut prompt);
        assert!(prompt.evidence.contains(&quoted));
        assert!(prompt.evidence.contains("## Remaining\n\npreserved"));
        assert!(prompt.evidence.contains(TRUNCATION_MARKER));
        assert_eq!(prompt.instructions, instructions);
        assert!(prompt.total_len() <= MAX_PROMPT_BYTES);
    }

    #[test]
    fn missing_section_body_does_not_trim_the_next_section() {
        let mut evidence = format!(
            "## Diff (frozen vs current)\n\n(unavailable)\n\n## Other\n\n```\n{}\n```\n",
            "data".repeat(100)
        );
        let original = evidence.clone();
        assert!(!halve_section(&mut evidence, "## Diff (frozen vs current)"));
        assert_eq!(evidence, original);
    }

    /// The protocol half must survive any amount of evidence: it is the only
    /// thing telling the session how to hand its verdict back.
    #[test]
    fn instructions_are_never_truncated() {
        let instructions = "loom stage adjudicate --stage s1 --dispute 1".repeat(10);
        let mut prompt = Prompt {
            instructions: instructions.clone(),
            evidence: "x".repeat(MAX_PROMPT_BYTES * 2),
        };
        truncate_to_budget(&mut prompt);
        assert_eq!(prompt.instructions, instructions);
        assert!(prompt.total_len() <= MAX_PROMPT_BYTES);
    }
}
