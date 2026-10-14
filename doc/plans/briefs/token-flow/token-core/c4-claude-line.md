# C4: Claude transcript line parser (codex unit)

Read doc/plans/briefs/token-flow/common.md section 5 (Claude transcript line) and the Format drift row of section 7 first. Do not run git. Do not touch any path under .loom.

## Files

- Write: loom/src/tokens/claude_line.rs and loom/src/tokens/claude_line_tests.rs.
- Read only: loom/src/tokens/model.rs (TokenUsage; written in parallel by unit C3, so rely on the listing in common.md section 3), loom/src/commands/usage/claude_usage.rs (existing field mapping, reference only; do not import it).

## Surface (pinned)

    use crate::tokens::model::TokenUsage;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ClaudeLine {
        Usage { message_id: Option<String>, timestamp: i64, model: Option<String>, usage: TokenUsage },
        Other,
        Unreadable,
    }

    pub fn parse_line(line: &str) -> ClaudeLine

## Steps

1. Implement parse_line with serde_json.
   - Only a line whose top-level type is assistant and whose message.usage is an object can be Usage.
   - input_tokens and output_tokens must be non-negative integers, otherwise the line is Unreadable. cache_creation_input_tokens and cache_read_input_tokens default to 0 when absent.
   - message_id comes from message.id and model from message.model.
   - timestamp is the top-level RFC 3339 timestamp converted to unix seconds with chrono. A missing or bad timestamp on an assistant usage line makes it Unreadable.
   - A line that fails to parse as JSON and contains the text "usage" (with the quotes) is Unreadable.
   - Every other line is Other: user lines, tool results, summaries, an assistant line without usage, and valid JSON that mentions usage in text.
2. Declare the tests with `#[cfg(test)] #[path = "claude_line_tests.rs"] mod tests;`.
3. Write unit tests:
   - the exact sample line of common.md section 5 parses to input 2, cache_write 14477, cache_read 4542, output 20, model claude-sonnet-5-5, message id msg_011CfaJtKtxUSKsEhtf2ECes, timestamp 1790802575;
   - missing cache fields give 0;
   - a string output_tokens is Unreadable;
   - torn JSON containing "usage" is Unreadable;
   - a user line whose text contains "usage" is Other;
   - an assistant line without usage is Other.

Done when your files compile and your tests are written (C3's model.rs exists before you start). Your only check is `cargo test --lib --no-run` from loom/: TMPDIR is read-only inside the codex sandbox, so tests that create temp files cannot run there, and the orchestrator runs `cargo test --lib tokens::` after your wave. A compile error in a file you do not own is not yours: report it and stop; never edit that file and never wait for it. Keep both files under 250 lines, functions under 50 lines, rustfmt clean.
