//! What no probe fixture or note may keep, beyond the paths and ids `redact` maps: the sandbox
//! proxy credentials and sockets, the attribution text of Claude Code's prompts, the account's
//! plan, usage and reset times, and the records that list the operator's skills and agents.
//!
//! Every pattern works on the text as stored, JSON escapes included, and its replacement holds
//! no quote or backslash, so a scrubbed JSON line stays valid JSON.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

/// `(pattern, replacement)`, applied in order.
const RULES: [(&str, &str); 13] = [
    // The sandbox proxy login: `srt.<base64 user>[:<hex password>]`, and its password variable.
    (r"srt\.[A-Za-z0-9+/=]+(?::[0-9a-f]{32})?", "<PROXY-TOKEN>"),
    (
        r#"(CLOUDSDK_PROXY_PASSWORD[ =]'?)[^\s'"\\]+"#,
        "${1}<PROXY-TOKEN>",
    ),
    (r"claude-http-[0-9a-f]+\.sock", "claude-http-<SOCK>.sock"),
    (r"/run/user/\d+/cc-socks/\d+\.sock", "<PEER-SOCK>"),
    (r"/tmp/claude-\d+", "<CLAUDE-TMP>"),
    // The commit and pull request attribution the prompts quote.
    (
        r#"Co-Authored-By: Claude[^<"\\\n]*(?:<noreply@anthropic\.com>)?"#,
        "<ATTRIBUTION>",
    ),
    (r"<?noreply@anthropic\.com>?", "<ATTRIBUTION>"),
    (
        r#"(?:\x{1F916} )?Generated with \[Claude Code\](?:\([^)\s"\\]*\))?"#,
        "<ATTRIBUTION>",
    ),
    // The /usage panes: reset times, time zones and percentages used.
    (r#"Resets [^(\n"\\<]{0,40}\([^)\n"\\]*\)"#, "Resets <RESET>"),
    (
        r"\((?:Africa|America|Antarctica|Arctic|Asia|Atlantic|Australia|Europe|Indian|Pacific|Etc)/[A-Za-z0-9_+/-]+\)",
        "(<TZ>)",
    ),
    (r"\b\d{1,3}(?:\.\d+)?% (used|left|remaining)", "<PCT>% $1"),
    // The subscription tier, in a pane and in an `account` object.
    (r"Claude (?:Ultra|Max|Pro|Team|Enterprise|Free)\b", "<PLAN>"),
    (r#"("subscriptionType"\s*:\s*")[^"]*""#, "${1}<PLAN>\""),
];

static COMPILED: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    RULES
        .iter()
        .map(|(pattern, with)| (Regex::new(pattern).unwrap(), *with))
        .collect()
});

/// The replacement of a plain-text line that holds a sandbox launch command: the command is
/// the environment of the session, and none of it is kept.
const LAUNCH_OMITTED: &str = "(sandbox launch command omitted: it names the session's environment)";

/// `text` with every rule applied; a plain line (not a JSON record) holding `--setenv` is
/// replaced whole. A JSON record holding one is left to the caller's check: a probe's JSON
/// has none today, and the rescrub test fails when one appears.
pub fn text(text: &str) -> String {
    let mut out = text.to_owned();
    for (rule, with) in COMPILED.iter() {
        out = rule.replace_all(&out, *with).into_owned();
    }
    let launch =
        |line: &&str| !line.trim_start().starts_with(['{', '[']) && line.contains("--setenv");
    let lines = out
        .split('\n')
        .map(|line| if launch(&line) { LAUNCH_OMITTED } else { line });
    lines.collect::<Vec<_>>().join("\n")
}

/// The kind of `line` when it is a transcript record no fixture keeps: an `attachment` that
/// lists the skills, agents or tools of the operator's setup, and the raw API request records.
pub fn omitted(line: &str) -> Option<String> {
    if !line.trim_start().starts_with('{') {
        return None;
    }
    let record: Value = serde_json::from_str(line).ok()?;
    let kind = record["type"].as_str()?;
    if kind == "api-request-blob" || kind == "api-request-shape" {
        return Some(kind.to_owned());
    }
    let attachment = record.pointer("/attachment/type")?.as_str()?;
    attachment
        .contains("listing")
        .then(|| "listing attachment".to_owned())
}

/// `text` (a note) with each line that is an omitted record replaced by a one-line mark.
pub fn without_records(text: &str) -> String {
    let marked = text.split('\n').map(|line| match omitted(line) {
        Some(kind) => format!("(record omitted: {kind})"),
        None => line.to_owned(),
    });
    marked.collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_sockets_and_paths_are_scrubbed() {
        let line = "--setenv HTTP_PROXY http://srt.dG9vbHVf+/=:0123456789abcdef0123456789abcdef@127.0.0.1:3128 \
                    --setenv CLOUDSDK_PROXY_PASSWORD 0123456789abcdef0123456789abcdef --bind /tmp/claude-http-360aaaf69fd4ef8b.sock /x \
                    uds:/run/user/1000/cc-socks/1355052.sock /tmp/claude-1000/cwd-1";
        let json = format!(r#"{{"cmd":"{line}"}}"#);
        let got = text(&json);
        assert!(serde_json::from_str::<Value>(&got).is_ok(), "{got}");
        for gone in [
            "srt.",
            "01234567",
            "360aaaf6",
            "cc-socks",
            "claude-1000",
            "dG9v",
        ] {
            assert!(!got.contains(gone), "{gone} in {got}");
        }
        assert!(got.contains("<PROXY-TOKEN>@127.0.0.1:3128"));
        assert!(got.contains("claude-http-<SOCK>.sock"));
        assert!(got.contains("uds:<PEER-SOCK>"));
        assert!(got.contains("<CLAUDE-TMP>/cwd-1"));
    }

    #[test]
    fn a_plain_launch_command_line_is_replaced_whole() {
        let note = "before\nbwrap --new-session --setenv A b --setenv C 'd e' -- /bin/zsh\nafter";
        assert_eq!(text(note), format!("before\n{LAUNCH_OMITTED}\nafter"));
    }

    #[test]
    fn attribution_inside_json_strings_is_scrubbed_and_the_line_still_parses() {
        let record = r#"{"p":"- End commits with:\nCo-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>\n- PRs: x `Co-Authored-By: Claude <noreply@anthropic.com>`. 🤖 Generated with [Claude Code](https://claude.com/claude-code)\nmail noreply@anthropic.com","q":"Generated with [Claude Code]"}"#;
        let got = text(record);
        let parsed: Value = serde_json::from_str(&got).unwrap();
        let want = "- End commits with:\n<ATTRIBUTION>\n- PRs: x `<ATTRIBUTION>`. <ATTRIBUTION>\nmail <ATTRIBUTION>";
        assert_eq!(parsed["p"].as_str().unwrap(), want);
        assert_eq!(parsed["q"], "<ATTRIBUTION>");
    }

    #[test]
    fn usage_plan_and_zone_are_scrubbed() {
        let pane =
            "Claude Ultra account\n  Resets Nov 15, 9pm (Europe/Athens)\n███ 8% used\n0.5% left";
        let want = "<PLAN> account\n  Resets <RESET>\n███ <PCT>% used\n<PCT>% left";
        assert_eq!(text(pane), want);
        let account =
            r#"{"account":{"subscriptionType":"Claude Ultra","tz":"(Asia/Tokyo)"},"x":"4% of 1m"}"#;
        let got = text(account);
        assert_eq!(
            got,
            r#"{"account":{"subscriptionType":"<PLAN>","tz":"(<TZ>)"},"x":"4% of 1m"}"#
        );
        assert_eq!(text(&got), got, "scrubbing is idempotent");
    }

    #[test]
    fn listing_and_request_records_are_omitted() {
        let listing =
            r#"{"type":"attachment","attachment":{"type":"skill_listing","content":"- a: b"}}"#;
        let delta = r#"{"type":"attachment","attachment":{"type":"agent_listing_delta"}}"#;
        let kept = r#"{"type":"attachment","attachment":{"type":"date"}}"#;
        assert_eq!(omitted(listing).as_deref(), Some("listing attachment"));
        assert!(omitted(delta).is_some());
        assert_eq!(
            omitted(r#"{"type":"api-request-blob","x":1}"#).as_deref(),
            Some("api-request-blob")
        );
        assert!(omitted(r#"{"type":"api-request-shape"}"#).is_some());
        assert!(omitted(kept).is_none() && omitted("not json").is_none());
        let note = format!("a\n{listing}\nb\n{kept}");
        assert_eq!(
            without_records(&note),
            format!("a\n(record omitted: listing attachment)\nb\n{kept}")
        );
    }
}
