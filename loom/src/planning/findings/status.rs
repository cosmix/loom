//! A finding's `Status:` and `Severity:` values.

use super::text::{one_line, reason};

/// Where a finding stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingStatus {
    /// Not yet addressed.
    Open,
    /// Addressed by an edit; the detail names what changed, as comma-separated `About:` tokens
    /// (`section_04#rebuild-budget, PLAN-x.md`).
    Addressed(String),
    /// An address session tried and could not address it, with why.
    Unaddressed(String),
    /// Dismissed by a reader or a review, with the reason.
    Dismissed(String),
}

impl FindingStatus {
    /// The text after `Status:`. Details pass the same sanitizers a write applies, so a status
    /// without a usable detail renders bare and reads back as a reported bad status.
    pub fn render(&self) -> String {
        match self {
            Self::Open => "open".to_owned(),
            Self::Addressed(changed) => with_detail("addressed", " ", changed),
            Self::Unaddressed(why) => {
                with_detail("unaddressed", ": ", &reason(why).unwrap_or_default())
            }
            Self::Dismissed(why) => {
                with_detail("dismissed", ": ", &reason(why).unwrap_or_default())
            }
        }
    }

    /// Whether the status carries a usable detail (`Open` has none to carry).
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Open => true,
            Self::Addressed(changed) => !one_line(changed).is_empty(),
            Self::Unaddressed(why) | Self::Dismissed(why) => reason(why).is_some(),
        }
    }

    /// Whether the finding still waits for someone to act on it.
    pub fn is_open(&self) -> bool {
        *self == Self::Open
    }

    pub(super) fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value == "open" {
            return Some(Self::Open);
        }
        if let Some(why) = value.strip_prefix("dismissed:") {
            return reason(why).map(Self::Dismissed);
        }
        if let Some(why) = value.strip_prefix("unaddressed:") {
            return reason(why).map(Self::Unaddressed);
        }
        // `addressed <tokens>`, also read with the colon the other two statuses take.
        let rest = value.strip_prefix("addressed")?;
        let changed = match rest.strip_prefix(':') {
            Some(after) => after,
            None if rest.starts_with(char::is_whitespace) => rest,
            None => return None,
        };
        let changed = changed.trim();
        (!changed.is_empty()).then(|| Self::Addressed(changed.to_owned()))
    }
}

fn with_detail(word: &str, separator: &str, detail: &str) -> String {
    let detail = one_line(detail);
    if detail.is_empty() {
        word.to_owned()
    } else {
        format!("{word}{separator}{detail}")
    }
}

/// How much a finding matters, as its `Severity:` line names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The feature would ship broken or contradict its spec.
    High,
    /// Something is missing or under-specified that a reader or builder would trip on.
    Medium,
    /// Wording, consistency or polish.
    Low,
}

impl Severity {
    /// The word a `Severity:` line holds.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }

    pub(super) fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "high" => Some(Self::High),
            "medium" => Some(Self::Medium),
            "low" => Some(Self::Low),
            _ => None,
        }
    }
}
