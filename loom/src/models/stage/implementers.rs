//! The agent lanes a stage may delegate implementation work to.

use serde::{Deserialize, Serialize};

/// A single agent lane a stage may delegate implementation work to.
///
/// Serialized as kebab-case in YAML (`claude`, `codex`). A closed enum rather
/// than a validated string: serde rejects unknown variants on its own, and the
/// closed set gives compile-time exhaustiveness at every match site.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Implementer {
    /// The Claude subagent lane (haiku for extremely simple or highly detailed pieces,
    /// sonnet for pieces that need exploration or test iteration, opus for architecture and
    /// algorithm implementation (xhigh effort for the hardest), fable only for visual/UI
    /// design or very hard algorithmic design).
    #[default]
    Claude,
    /// The codex implementation lane: spawned as `loom-codex-forwarder`, a
    /// loom-owned shim over the codex plugin's companion runtime (never the
    /// plugin's `codex:codex-rescue` directly).
    Codex,
}

impl std::fmt::Display for Implementer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Implementer::Claude => "claude",
            Implementer::Codex => "codex",
        };
        write!(f, "{s}")
    }
}

/// The set of agent lanes a stage is licensed to spawn subagents from.
///
/// A stage mixes lanes freely: routine implementation may go to codex while
/// tests go to sonnet and an architectural call stays with opus. Order is
/// meaningful — the FIRST lane is the one to reach for on routine
/// implementation ([`Implementers::preferred`]) — but every listed lane is
/// available for the parts of the stage that call for it.
///
/// Serialized transparently as a YAML sequence (`["codex", "claude"]`), so a
/// stage declares lanes the same way it declares any other list. An omitted
/// key defaults to `["claude"]`: the Claude lane is the harness the session
/// already runs in, so it needs no opt-in. Codex does — it needs a plugin, and
/// it needs the safety doctrine that [`Implementers::includes_codex`] gates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Implementers(Vec<Implementer>);

impl Default for Implementers {
    fn default() -> Self {
        Self(vec![Implementer::Claude])
    }
}

impl Implementers {
    /// Build a lane set from an ordered list.
    pub fn new(lanes: Vec<Implementer>) -> Self {
        Self(lanes)
    }

    /// The lane to reach for on ROUTINE implementation work — the first listed.
    ///
    /// Falls back to [`Implementer::Claude`] for an empty set. Validation
    /// rejects an explicitly empty list, so the fallback only covers a value
    /// constructed in code rather than parsed from a plan.
    pub fn preferred(&self) -> Implementer {
        self.0.first().copied().unwrap_or_default()
    }

    /// Whether the codex lane is licensed for this stage.
    ///
    /// This is the gate for the codex safety doctrine: any stage that may
    /// spawn even ONE codex subagent has to carry the blast-radius rules,
    /// whether or not codex is its preferred lane.
    pub fn includes_codex(&self) -> bool {
        self.0.contains(&Implementer::Codex)
    }

    /// Whether the Claude subagent lane is licensed for this stage.
    pub fn includes_claude(&self) -> bool {
        self.0.contains(&Implementer::Claude)
    }

    /// Whether more than one lane is licensed.
    pub fn is_mixed(&self) -> bool {
        self.0.len() > 1
    }

    /// True when no lane is listed at all — an invalid state that validation rejects.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'a> IntoIterator for &'a Implementers {
    type Item = &'a Implementer;
    type IntoIter = std::slice::Iter<'a, Implementer>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl std::fmt::Display for Implementers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rendered: Vec<String> = self.0.iter().map(|l| l.to_string()).collect();
        write!(f, "{}", rendered.join(", "))
    }
}
