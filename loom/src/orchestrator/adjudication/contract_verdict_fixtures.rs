//! Contract disputes and their `contract-rulings` verdicts, as the apply
//! tests write them.

use crate::models::dispute::{
    Citation, ContractRuling, ContractRulingKind, DisputeKind, DisputeVerdict, FileReplacement,
    PlanPatch,
};

/// A contract dispute over `ids` carrying `replacements`.
pub(super) fn contract_kind(ids: &[&str], replacements: Vec<FileReplacement>) -> DisputeKind {
    DisputeKind::Contract {
        contract_ids: ids.iter().map(|id| (*id).to_string()).collect(),
        replacements,
    }
}

/// A grounded ruling of `kind` on `contract`.
pub(super) fn ruling(contract: &str, kind: ContractRulingKind) -> ContractRuling {
    ContractRuling {
        contract: contract.to_string(),
        ruling: kind,
        reasoning: format!("judged {contract} on the cited lines"),
        citations: vec![Citation {
            file: "tests/x_contract.rs".to_string(),
            line: Some(1),
            excerpt: "fn rejects_x()".to_string(),
            claim: "the contract's spec decides it".to_string(),
        }],
    }
}

/// A `contract-rulings` verdict of `rulings` carrying `plan_patch` (`{}` for
/// none).
pub(super) fn rulings_verdict(
    rulings: &[(&str, ContractRulingKind)],
    plan_patch: serde_json::Value,
) -> DisputeVerdict {
    DisputeVerdict::ContractRulings {
        rulings: rulings
            .iter()
            .map(|(contract, kind)| ruling(contract, *kind))
            .collect(),
        plan_patch: PlanPatch { inner: plan_patch },
    }
}
