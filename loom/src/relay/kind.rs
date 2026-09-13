//! The seven request kinds a CLI invocation can relay to the daemon.

use anyhow::bail;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// One relayed request's shape. Wire names match the CLI action a kind
/// corresponds to (`loom stage merge --resolved` writes `merge-resolved`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RequestKind {
    Memory,
    Block,
    Dispute,
    Handoff,
    MergeResolved,
    Verdict,
    Telemetry,
}

impl RequestKind {
    /// All seven kinds, in the per-writer matrix's order
    /// (`doc/plans/PLAN-loom-state-confinement.md` section 5).
    pub fn all() -> [RequestKind; 7] {
        [
            RequestKind::Memory,
            RequestKind::Block,
            RequestKind::Dispute,
            RequestKind::Handoff,
            RequestKind::MergeResolved,
            RequestKind::Verdict,
            RequestKind::Telemetry,
        ]
    }

    /// The five kinds only a session's lead process may relay; the relay
    /// hook drops these from a teammate before an inbox entry is ever
    /// written.
    pub fn is_control(self) -> bool {
        matches!(
            self,
            RequestKind::Block
                | RequestKind::Dispute
                | RequestKind::Handoff
                | RequestKind::MergeResolved
                | RequestKind::Verdict
        )
    }

    fn wire_name(self) -> &'static str {
        match self {
            RequestKind::Memory => "memory",
            RequestKind::Block => "block",
            RequestKind::Dispute => "dispute",
            RequestKind::Handoff => "handoff",
            RequestKind::MergeResolved => "merge-resolved",
            RequestKind::Verdict => "verdict",
            RequestKind::Telemetry => "telemetry",
        }
    }
}

impl fmt::Display for RequestKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire_name())
    }
}

impl FromStr for RequestKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for kind in RequestKind::all() {
            if kind.wire_name() == s {
                return Ok(kind);
            }
        }
        bail!("unknown relay request kind: {s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_from_str_and_serde_agree_on_every_wire_name() {
        for kind in RequestKind::all() {
            let name = kind.to_string();
            assert_eq!(name.parse::<RequestKind>().unwrap(), kind);
            assert_eq!(serde_json::to_value(kind).unwrap(), serde_json::json!(name));
            assert_eq!(
                serde_json::from_value::<RequestKind>(serde_json::json!(name)).unwrap(),
                kind
            );
        }
    }

    #[test]
    fn merge_resolved_is_kebab_case_on_the_wire() {
        assert_eq!(RequestKind::MergeResolved.to_string(), "merge-resolved");
    }

    #[test]
    fn is_control_matches_the_five_control_kinds() {
        let control: Vec<RequestKind> = RequestKind::all()
            .into_iter()
            .filter(|kind| kind.is_control())
            .collect();
        assert_eq!(
            control,
            vec![
                RequestKind::Block,
                RequestKind::Dispute,
                RequestKind::Handoff,
                RequestKind::MergeResolved,
                RequestKind::Verdict,
            ]
        );
    }

    #[test]
    fn from_str_rejects_an_unknown_kind() {
        assert!("bogus".parse::<RequestKind>().is_err());
    }
}
