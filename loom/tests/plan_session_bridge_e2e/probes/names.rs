//! The operator's personal skill, command, agent and plugin names, replaced in probe output by
//! numbered tokens. The set is read from the real `~/.claude` when a probe starts and numbered
//! in sorted order, so one operator's runs map every name to the same token.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use regex::Regex;
use serde_json::Value;

/// The claude.ai account skills' namespace (`anthropic-skills:*` rows).
const ACCOUNT_NAMESPACE: &str = "anthropic-skills";

/// Where a name counts as a match. A short plain word (`address`, `ai`) is also an ordinary word
/// of prose, so it is matched only where a name is written as one.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    /// Any whole token.
    Anywhere,
    /// `/name`, or `"name"` (also as `\"name\"` inside a JSON string).
    Marked,
    /// `name:`, the namespace of a plugin's rows.
    Namespace,
}

struct Name {
    token: String,
    mode: Mode,
}

#[derive(Default)]
pub struct Names {
    finder: Option<Regex>,
    known: HashMap<String, Name>,
}

fn identifier(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The sorted, de-duplicated `names` without a `probe-` prefix (the probes' own files).
fn sorted(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut names: Vec<String> = names
        .into_iter()
        .filter(|name| !name.starts_with("probe-") && !name.starts_with('.'))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The file names in `dir`, with `suffix` removed; entries without it are skipped.
fn entries(dir: &Path, suffix: &str) -> Vec<String> {
    let found = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok);
    let names = found.map(|entry| entry.file_name().to_string_lossy().into_owned());
    let kept = names.filter_map(|name| Some(name.strip_suffix(suffix)?.to_owned()));
    sorted(kept)
}

/// The plugin names `~/.claude/plugins/installed_plugins.json` registers: each key before its
/// `@marketplace`, and the `name` of its manifest when that differs (`pydantic-ai` is `ai`).
fn plugins(claude: &Path) -> Vec<String> {
    let text = fs::read_to_string(claude.join("plugins/installed_plugins.json"));
    let registry: Value = serde_json::from_str(&text.unwrap_or_default()).unwrap_or_default();
    let mut names = vec![ACCOUNT_NAMESPACE.to_owned()];
    for (key, installs) in registry["plugins"].as_object().into_iter().flatten() {
        names.push(key.split('@').next().unwrap_or(key).to_owned());
        let path = installs[0]["installPath"].as_str().unwrap_or_default();
        let manifest = fs::read_to_string(Path::new(path).join(".claude-plugin/plugin.json"));
        let manifest: Value =
            serde_json::from_str(&manifest.unwrap_or_default()).unwrap_or_default();
        names.extend(manifest["name"].as_str().map(str::to_owned));
    }
    sorted(names)
}

impl Names {
    /// The names under `home`'s `.claude`; none when it does not exist.
    pub fn load(home: &Path) -> Names {
        let claude = home.join(".claude");
        let mut skills = entries(&claude.join("skills"), "");
        skills.extend(entries(&claude.join("commands"), ".md"));
        let agents = entries(&claude.join("agents"), ".md");
        Names::from_lists(&skills, &agents, &plugins(&claude))
    }

    /// Skills and commands become `<SKILL-n>`, agents `<AGENT-DEF-n>` (`<AGENT-n>` is a
    /// subagent id), plugins `<PLUGIN-n>`; `n` is the 1-based position in each sorted list.
    pub fn from_lists(skills: &[String], agents: &[String], plugins: &[String]) -> Names {
        let mut known = HashMap::new();
        let kinds = [
            ("SKILL", skills, false),
            ("AGENT-DEF", agents, false),
            ("PLUGIN", plugins, true),
        ];
        for (kind, names, plugin) in kinds {
            for (n, name) in sorted(names.iter().cloned()).into_iter().enumerate() {
                let token = format!("<{kind}-{}>", n + 1);
                let mode = mode_of(&name, plugin);
                known.entry(name).or_insert(Name { token, mode });
            }
        }
        let mut texts: Vec<&String> = known.keys().collect();
        texts.sort_by_key(|text| std::cmp::Reverse(text.len()));
        let alternatives: Vec<String> = texts.iter().map(|t| regex::escape(t)).collect();
        let finder =
            (!alternatives.is_empty()).then(|| Regex::new(&alternatives.join("|")).unwrap());
        Names { finder, known }
    }

    /// `text` with every name written as a whole token replaced by its token.
    pub fn scrub(&self, text: &str) -> String {
        let Some(finder) = &self.finder else {
            return text.to_owned();
        };
        let mut out = String::with_capacity(text.len());
        let (mut copied, mut at) = (0, 0);
        while let Some(found) = finder.find_at(text, at) {
            let before = text[..found.start()].chars().next_back();
            let after = text[found.end()..].chars().next();
            let name = &self.known[found.as_str()];
            if accepts(name.mode, before, after) {
                out.push_str(&text[copied..found.start()]);
                out.push_str(&name.token);
                (copied, at) = (found.end(), found.end());
            } else {
                // Retry one character on, so a name inside the rejected match is still found.
                let first = found.as_str().chars().next().map_or(1, char::len_utf8);
                at = found.start() + first;
            }
        }
        out.push_str(&text[copied..]);
        out
    }
}

fn mode_of(name: &str, plugin: bool) -> Mode {
    if plugin {
        return if name.len() >= 4 {
            Mode::Anywhere
        } else {
            Mode::Namespace
        };
    }
    let plain = name.chars().all(|c| c.is_ascii_lowercase());
    if plain && name.len() < 8 {
        Mode::Marked
    } else {
        Mode::Anywhere
    }
}

fn accepts(mode: Mode, before: Option<char>, after: Option<char>) -> bool {
    if before.is_some_and(identifier) || after.is_some_and(identifier) {
        return false;
    }
    match mode {
        Mode::Anywhere => true,
        Mode::Marked => {
            let quoted = before == Some('"') && matches!(after, Some('"' | '\\'));
            before == Some('/') || quoted
        }
        Mode::Namespace => after == Some(':'),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Names {
        let words = |list: &[&str]| list.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        let skills = words(&[
            "wrangler",
            "cloudflare",
            "cloudflare-one",
            "address",
            "loom-skills",
        ]);
        Names::from_lists(
            &skills,
            &words(&["loom-advisor"]),
            &words(&["vnkt", "ai", "codex"]),
        )
    }

    #[test]
    fn names_are_replaced_as_whole_tokens_with_a_stable_numbering() {
        let n = names();
        let text =
            r#"/loom-skills "wrangler" wrangler.jsonc cloudflare-one cloudflare vnkt:vnkt-triage"#;
        let want = "/<SKILL-4> \"<SKILL-5>\" <SKILL-5>.jsonc <SKILL-3> <SKILL-2> \
                    <PLUGIN-3>:<PLUGIN-3>-triage";
        assert_eq!(n.scrub(text), want);
        let text = "ai:building codex:run <AGENT-DEF-1> loom-advisor";
        let want = "<PLUGIN-1>:building <PLUGIN-2>:run <AGENT-DEF-1> <AGENT-DEF-1>";
        assert_eq!(n.scrub(text), want);
    }

    #[test]
    fn plain_words_and_longer_words_are_left_alone() {
        let n = names();
        for text in [
            "the address of a cloudflared tool",
            "Peer address: uds:/x and an email address",
            "said hello, ai is a word; maintain",
            "loom-skills-extra",
            "xwrangler wranglers",
        ] {
            let want = if text == "loom-skills-extra" {
                "<SKILL-4>-extra"
            } else {
                text
            };
            assert_eq!(n.scrub(text), want);
        }
        let marked = r#"{\"address\":1} /address.md"#;
        assert_eq!(n.scrub(marked), r#"{\"<SKILL-1>\":1} /<SKILL-1>.md"#);
        assert_eq!(Names::default().scrub("wrangler"), "wrangler");
    }
}
