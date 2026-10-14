# C2: Codex rollout line parser (codex unit)

Read doc/plans/briefs/token-flow/common.md section 5 (Codex rollout) and the Format drift row of section 7 first. Do not run git. Do not touch any path under .loom.

## Files

- Write: loom/src/tokens/codex_line.rs and loom/src/tokens/codex_line_tests.rs.
- Read only: loom/src/tokens/model.rs (TokenUsage; written in parallel by unit C3 from common.md section 3, so rely on the listing there), loom/src/commands/usage/codex_tokens.rs (existing subset rules, for reference only; do not import it).

## Surface (pinned)

    use crate::tokens::model::TokenUsage;

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct CodexTotals { pub input: u64, pub cached_input: u64, pub cache_write: u64, pub output: u64 }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum CodexLine {
        TokenCount { timestamp: i64, total: CodexTotals },
        Model(String),
        SessionMeta { thread_id: String },
        Other,
        Unreadable,
    }

    pub fn parse_line(line: &str) -> CodexLine

    impl CodexTotals {
        /// Field-wise difference; None when any field decreased.
        pub fn delta_since(&self, previous: &CodexTotals) -> Option<CodexTotals>
        /// input = input - cached_input - cache_write (saturating), cache_write, cache_read = cached_input, output.
        pub fn to_usage(&self) -> TokenUsage
    }

## Steps

1. Implement parse_line with serde_json.
   - type event_msg with payload.type token_count and an info object holding numeric total_token_usage.input_tokens and output_tokens gives TokenCount. cached_input_tokens and cache_write_input_tokens default to 0. The timestamp is the top-level RFC 3339 timestamp converted to unix seconds with chrono; a missing or bad timestamp makes the line Unreadable.
   - payload.info null gives Other.
   - type turn_context with a string payload.model gives Model.
   - type session_meta with a string payload.id gives SessionMeta.
   - A line that fails to parse as JSON and contains the text token_count gives Unreadable. A token_count event whose info is an object without a numeric total_token_usage gives Unreadable.
   - Anything else gives Other.
2. Implement delta_since and to_usage.
3. Write unit tests from the exact sample lines in common.md section 5:
   - both token_count lines;
   - delta between them is input 22019, cached 21120, output 96;
   - to_usage of the first gives input 9095, cache_read 12160, output 110;
   - info null;
   - turn_context;
   - session_meta;
   - a torn JSON line containing token_count is Unreadable;
   - a valid user message whose text mentions token_count is Other;
   - a decrease makes delta_since return None.

Declare tests with `#[cfg(test)] #[path = "codex_line_tests.rs"] mod tests;`. Done when your files compile and your tests are written (C3's model.rs exists before you start). Your only check is `cargo test --lib --no-run` from loom/: TMPDIR is read-only inside the codex sandbox, so tests that create temp files cannot run there, and the orchestrator runs `cargo test --lib tokens::` after your wave. A compile error in a file you do not own is not yours: report it and stop; never edit that file and never wait for it. Keep both files under 250 lines, functions under 50 lines, rustfmt clean.
