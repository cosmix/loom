//! The thirteen request kinds a CLI invocation can relay to the daemon.

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
    FreezeContracts,
    FileDispute,
    Commit,
    Check,
    Complete,
    #[serde(rename = "subagents:control")]
    WorkerControl,
}

impl RequestKind {
    /// All thirteen kinds, in the per-writer matrix's order
    /// (`doc/plans/PLAN-loom-state-confinement.md` section 5, then
    /// `freeze-contracts` from verification v2's contract phase,
    /// `file-dispute` from its dispute kinds, `commit`, the staged index the
    /// daemon commits for a session, `check`, a host verification job, and
    /// `complete`, the host job whose passing run lets the daemon complete
    /// the stage), and `subagents:control`, the lead's start, message and
    /// cancel of a native Codex worker.
    pub fn all() -> [RequestKind; 13] {
        [
            RequestKind::Memory,
            RequestKind::Block,
            RequestKind::Dispute,
            RequestKind::Handoff,
            RequestKind::MergeResolved,
            RequestKind::Verdict,
            RequestKind::Telemetry,
            RequestKind::FreezeContracts,
            RequestKind::FileDispute,
            RequestKind::Commit,
            RequestKind::Check,
            RequestKind::Complete,
            RequestKind::WorkerControl,
        ]
    }

    /// The ten kinds only a session's lead process may relay; the relay
    /// hook drops these from a teammate before an inbox entry is ever
    /// written, and the daemon's drain refuses one that arrives anyway.
    pub fn is_control(self) -> bool {
        matches!(
            self,
            RequestKind::Block
                | RequestKind::Dispute
                | RequestKind::Handoff
                | RequestKind::MergeResolved
                | RequestKind::Verdict
                | RequestKind::FreezeContracts
                | RequestKind::FileDispute
                | RequestKind::Commit
                | RequestKind::Complete
                | RequestKind::WorkerControl
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
            RequestKind::FreezeContracts => "freeze-contracts",
            RequestKind::FileDispute => "file-dispute",
            RequestKind::Commit => "commit",
            RequestKind::Check => "check",
            RequestKind::Complete => "complete",
            RequestKind::WorkerControl => "subagents:control",
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
    fn is_control_matches_the_ten_control_kinds() {
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
                RequestKind::FreezeContracts,
                RequestKind::FileDispute,
                RequestKind::Commit,
                RequestKind::Complete,
                RequestKind::WorkerControl,
            ]
        );
    }

    #[test]
    fn freeze_contracts_is_kebab_case_on_the_wire() {
        assert_eq!(RequestKind::FreezeContracts.to_string(), "freeze-contracts");
    }

    #[test]
    fn file_dispute_is_kebab_case_on_the_wire() {
        assert_eq!(RequestKind::FileDispute.to_string(), "file-dispute");
    }

    #[test]
    fn commit_is_kebab_case_on_the_wire() {
        assert_eq!(RequestKind::Commit.to_string(), "commit");
        assert_eq!(
            "commit".parse::<RequestKind>().unwrap(),
            RequestKind::Commit
        );
    }

    #[test]
    fn worker_control_is_subagents_control_on_the_wire() {
        assert_eq!(RequestKind::WorkerControl.to_string(), "subagents:control");
        assert_eq!(
            "subagents:control".parse::<RequestKind>().unwrap(),
            RequestKind::WorkerControl
        );
        assert_eq!(
            serde_json::to_value(RequestKind::WorkerControl).unwrap(),
            serde_json::json!("subagents:control")
        );
    }

    #[test]
    fn from_str_rejects_an_unknown_kind() {
        assert!("bogus".parse::<RequestKind>().is_err());
    }
}
