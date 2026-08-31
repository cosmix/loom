//! Keeping a briefing inside [`MAX_PROMPT_BYTES`](super::MAX_PROMPT_BYTES).
//!
//! The evidence sections are trimmed in order of expendability — diff first,
//! then the failure output, then the listing, then the plan excerpt — and the
//! instructions are never touched: a session that cannot see the whole diff
//! can still go and read the tree, but a session that has lost half its
//! protocol has no way back.

use super::{Prompt, MAX_PROMPT_BYTES};

pub(super) const TRUNCATION_MARKER: &str = "\n... [truncated] ...\n";

/// Enforce [`MAX_PROMPT_BYTES`] by truncating the diff / failure-output
/// sections of the evidence first, then the listing, then the plan excerpt.
pub(super) fn truncate_to_budget(prompt: &mut Prompt) {
    if prompt.total_len() <= MAX_PROMPT_BYTES {
        return;
    }
    // Strategy: repeatedly halve the diff fence first, then the failure
    // output fence, then the listing fence, then the plan excerpt.
    let candidates = [
        "## Evidence commit diff (git show)",
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

/// Find a "## Header" section in `evidence` and halve the contents of the
/// first triple-backtick fence inside it. Returns `false` if no further
/// trimming is possible (no header, no fence, or fence already empty).
fn halve_section(evidence: &mut String, header: &str) -> bool {
    let Some(header_pos) = evidence.find(header) else {
        return false;
    };
    let after_header = header_pos + header.len();
    let fence_open_rel = match evidence[after_header..].find("```") {
        Some(p) => p,
        None => return false,
    };
    let fence_open = after_header + fence_open_rel;
    // Skip the rest of the fence-open line.
    let body_start = match evidence[fence_open..].find('\n') {
        Some(p) => fence_open + p + 1,
        None => return false,
    };
    let fence_close_rel = match evidence[body_start..].find("```") {
        Some(p) => p,
        None => return false,
    };
    let body_end = body_start + fence_close_rel;
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
