//! What a probe writes, always redacted: its notes file, rewritten after every entry so a
//! failed probe keeps what it saw, and the fixture files its records become. The records that
//! list the operator's setup are left out of both.

use std::fs;
use std::path::{Path, PathBuf};

use super::redact::Redactor;
use super::scrub;

/// `tests/fixtures/plan_session` of this crate.
pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plan_session")
}

pub struct Output {
    name: &'static str,
    redactor: Redactor,
    notes: String,
}

/// `body` in a text fence that no line of it can close.
fn fenced(body: &str) -> String {
    let longest = body
        .lines()
        .map(|line| line.chars().take_while(|c| *c == '`').count())
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(3) + 1);
    format!("{fence}text\n{}\n{fence}\n", body.trim_end())
}

impl Output {
    pub fn new(name: &'static str, redactor: Redactor, version: &str) -> Output {
        let notes = format!("# Probe {name}\n\nClaude Code: {version}\n");
        let mut output = Output {
            name,
            redactor,
            notes,
        };
        output.flush();
        output
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Registers a session id with the redaction.
    pub fn session(&mut self, id: &str) {
        self.redactor.session(id);
    }

    /// Registers an agent id; returns its token number.
    pub fn agent(&mut self, id: &str) -> usize {
        self.redactor.agent(id)
    }

    /// Appends a section: `heading`, then `body` verbatim in a text fence.
    pub fn note(&mut self, heading: &str, body: &str) {
        let body = if body.trim().is_empty() {
            "(none)"
        } else {
            body
        };
        self.notes
            .push_str(&format!("\n## {heading}\n\n{}", fenced(body)));
        self.flush();
    }

    /// Writes the notes to `probe-notes/<name>.md`.
    fn flush(&mut self) {
        let text = note_text(&mut self.redactor, &self.notes);
        let path = fixtures().join(format!("probe-notes/{}.md", self.name));
        if let Err(error) = write(&path, &text) {
            eprintln!("cannot write {}: {error}", path.display());
        }
    }

    /// Writes `lines`, redacted and without the omitted records, to `<fixtures>/<relative>`.
    pub fn fixture(&mut self, relative: &str, lines: &[String]) {
        let text = records(&mut self.redactor, lines);
        let path = fixtures().join(relative);
        if let Err(error) = write(&path, &text) {
            panic!("cannot write the fixture {}: {error}", path.display());
        }
    }
}

/// The redacted `lines`, one per line, without the records `scrub::omitted` names.
pub fn records(redactor: &mut Redactor, lines: &[String]) -> String {
    let mut text = String::new();
    for line in lines.iter().filter(|line| scrub::omitted(line).is_none()) {
        text.push_str(&redactor.redact(line));
        text.push('\n');
    }
    text
}

/// The redacted notes `text`, an omitted record replaced by a one-line mark.
pub fn note_text(redactor: &mut Redactor, text: &str) -> String {
    redactor.redact(&scrub::without_records(text))
}

fn write(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)
}
