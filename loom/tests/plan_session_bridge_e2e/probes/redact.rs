//! Redaction of probe output: the home and temp paths, the user's identity, the personal skill
//! and plugin names, and session and agent ids become fixed tokens, mapped consistently within
//! one probe. [`scrub`] then removes credentials, attribution text and account details.

use std::path::Path;

use regex::Regex;

use super::names::Names;
use super::scrub;

/// Replaces literals and learned ids with tokens; ids are numbered in the order first seen.
pub struct Redactor {
    literals: Vec<(String, String)>,
    user: Option<(Regex, String)>,
    sessions: Vec<String>,
    agents: Vec<String>,
    ids: Regex,
    names: Names,
}

/// Claude Code's project directory name for `path`: every non-alphanumeric char becomes `-`.
fn dashed(path: &str) -> String {
    let dash = |c: char| if c.is_ascii_alphanumeric() { c } else { '-' };
    path.chars().map(dash).collect()
}

impl Redactor {
    /// `home` becomes `<HOME>`, `base` `<BASE>` (both also in their dashed project-directory
    /// form), `user` `<USER>` as a whole word, each `(text, token)` of `identity` its token, and
    /// the skill, command, agent and plugin names under `home`'s `.claude` their numbered tokens.
    pub fn new(home: &Path, base: &Path, user: &str, identity: &[(String, &str)]) -> Redactor {
        let mut literals = Vec::new();
        for (path, token) in [(home, "<HOME>"), (base, "<BASE>")] {
            let text = path.to_string_lossy().into_owned();
            literals.push((dashed(&text), token.to_owned()));
            literals.push((text, token.to_owned()));
        }
        let identity = identity.iter().map(|(t, k)| (t.clone(), (*k).to_owned()));
        literals.extend(identity);
        literals.retain(|(text, _)| text.len() > 1);
        // The longest first: a path under another is never cut by the shorter one.
        literals.sort_by_key(|(text, _)| std::cmp::Reverse(text.len()));
        literals.dedup_by(|a, b| a.0 == b.0);
        let word = format!(r"\b{}\b", regex::escape(user));
        let user = (!user.is_empty()).then(|| (Regex::new(&word).unwrap(), "<USER>".to_owned()));
        let ids = r#""(session_id|sessionId|agent_id|agentId)"\s*:\s*"([^"<]{8,})""#;
        Redactor {
            literals,
            user,
            sessions: Vec::new(),
            agents: Vec::new(),
            ids: Regex::new(ids).unwrap(),
            names: Names::load(home),
        }
    }

    /// Registers a session id; returns its token number.
    pub fn session(&mut self, id: &str) -> usize {
        number(&mut self.sessions, id)
    }

    /// Registers an agent id; returns its token number.
    pub fn agent(&mut self, id: &str) -> usize {
        number(&mut self.agents, id)
    }

    /// Registers every id `text` names in a `session_id`, `sessionId`, `agent_id` or `agentId`
    /// field.
    fn learn(&mut self, text: &str) {
        let found: Vec<(String, String)> = self
            .ids
            .captures_iter(text)
            .map(|c| (c[1].to_owned(), c[2].to_owned()))
            .collect();
        for (field, id) in found {
            if field.starts_with("session") {
                self.session(&id);
            } else {
                self.agent(&id);
            }
        }
    }

    /// `text` with every literal, the user name, every known or newly named id and every
    /// personal name replaced, then scrubbed.
    pub fn redact(&mut self, text: &str) -> String {
        self.learn(text);
        let mut out = text.to_owned();
        for (literal, token) in &self.literals {
            out = out.replace(literal.as_str(), token);
        }
        let sessions = self
            .sessions
            .iter()
            .enumerate()
            .map(|(n, id)| (id, "SESSION", n));
        let agents = self
            .agents
            .iter()
            .enumerate()
            .map(|(n, id)| (id, "AGENT", n));
        let mut ids: Vec<_> = sessions.chain(agents).collect();
        ids.sort_by_key(|(id, ..)| std::cmp::Reverse(id.len()));
        for (id, kind, n) in ids {
            out = out.replace(id.as_str(), &format!("<{kind}-{}>", n + 1));
        }
        if let Some((word, token)) = &self.user {
            out = word.replace_all(&out, token.as_str()).into_owned();
        }
        scrub::text(&self.names.scrub(&out))
    }
}

/// The 1-based position of `id` in `known`, appended when new.
fn number(known: &mut Vec<String>, id: &str) -> usize {
    match known.iter().position(|seen| seen == id) {
        Some(at) => at + 1,
        None => {
            known.push(id.to_owned());
            known.len()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redactor() -> Redactor {
        let identity = [("ann@example.com".to_owned(), "<EMAIL>")];
        let base = Path::new("/home/ann/tmp/.tmpX1");
        Redactor::new(Path::new("/home/ann"), base, "ann", &identity)
    }

    #[test]
    fn redaction_maps_paths_users_and_ids_consistently() {
        let mut r = redactor();
        let first = concat!(
            r#"{"sessionId":"11111111-aaaa-2222","cwd":"/home/ann/tmp/.tmpX1/project","#,
            r#""agentId":"abcdef0123456789","#,
            r#""path":"/home/ann/.claude/projects/-home-ann-tmp--tmpX1-project/x.jsonl","#,
            r#""by":"ann","mail":"ann@example.com","note":"annotation"}"#
        );
        let want = concat!(
            r#"{"sessionId":"<SESSION-1>","cwd":"<BASE>/project","agentId":"<AGENT-1>","#,
            r#""path":"<HOME>/.claude/projects/<BASE>-project/x.jsonl","#,
            r#""by":"<USER>","mail":"<EMAIL>","note":"annotation"}"#
        );
        assert_eq!(r.redact(first), want);
        let second = concat!(
            r#"{"session_id":"99999999-bbbb-0000","agent_id":"abcdef0123456789","#,
            r#""text":"resumed from 11111111-aaaa-2222"}"#
        );
        let want = concat!(
            r#"{"session_id":"<SESSION-2>","agent_id":"<AGENT-1>","#,
            r#""text":"resumed from <SESSION-1>"}"#
        );
        assert_eq!(r.redact(second), want);
    }

    #[test]
    fn redaction_numbers_registered_ids_and_skips_short_ones() {
        let mut r = redactor();
        assert_eq!(r.agent("feedfacecafe0001"), 1);
        assert_eq!(r.agent("feedfacecafe0002"), 2);
        assert_eq!(r.agent("feedfacecafe0001"), 1);
        let text = r#"agent-feedfacecafe0002.jsonl {"agentId":"a7f3"}"#;
        assert_eq!(
            r.redact(text),
            r#"agent-<AGENT-2>.jsonl {"agentId":"a7f3"}"#
        );
    }
}
