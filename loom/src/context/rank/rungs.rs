//! The exact-match rung accumulator both rankers award through.
//!
//! A rung is a named, additive boost plus the [`SelectionReason`] that explains
//! it. Collecting them behind one type is what lets the knowledge ladder
//! (`rank.rs`) and the source ladder (`rank_source.rs`) share the ONE piece of
//! policy that cannot be expressed in the reason list: how strong the evidence
//! behind a high-tier rung was.
//!
//! [`crate::context::schema::Confidence::from_reasons`] classifies `High` from
//! the mere presence of `ExplicitId`, `ExactPath` or `ExactSymbol`. That is the
//! right rule for a backticked or identifier-shaped match, and the wrong rule
//! for one admitted purely because the name happens to be uncommon in this
//! corpus (`lexical::TermEvidence::rare`) — corpus rarity is real evidence, but
//! it is *weaker* evidence, and labelling it `high` is how a coincidence
//! becomes a claim. Since the reason set has no way to say "exact, but only
//! just", the strength rides alongside as a confidence CEILING.

use crate::context::lexical::TermEvidence;
use crate::context::schema::{Confidence, SelectionReason};

/// How strong the evidence behind a high-tier rung was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RungStrength {
    /// An explicit id, a full path, or an identifier-shaped/backticked match.
    Full,
    /// Admitted by corpus rarity alone.
    RareOnly,
}

/// The exact-match rungs one candidate earned.
#[derive(Debug, Default)]
pub(crate) struct RungScore {
    /// Summed boost across every rung that fired.
    pub(crate) score: f32,
    /// Reasons in ladder order, ready to hand to a `RankedCandidate`.
    pub(crate) reasons: Vec<SelectionReason>,
    /// High-tier rungs awarded so far.
    high_rungs: usize,
    /// High-tier rungs awarded so far that rested on corpus rarity alone.
    rare_only_rungs: usize,
}

impl RungScore {
    /// Award a rung whose evidence is not an identifier match: an explicit id,
    /// a full-path hit, a link neighbour, a stage dependency.
    pub(crate) fn award(&mut self, boost: f32, reason: SelectionReason) {
        self.award_with(boost, reason, RungStrength::Full);
    }

    /// Award a rung earned by matching a name in the query text, carrying the
    /// evidence that admitted it.
    pub(crate) fn award_matched(
        &mut self,
        boost: f32,
        reason: SelectionReason,
        evidence: &TermEvidence,
    ) {
        let strength = if evidence.backticked || evidence.shaped {
            RungStrength::Full
        } else {
            RungStrength::RareOnly
        };
        self.award_with(boost, reason, strength);
    }

    fn award_with(&mut self, boost: f32, reason: SelectionReason, strength: RungStrength) {
        self.score += boost;
        // Asking `Confidence` which reasons are high-tier, rather than listing
        // them here, keeps this in step with `schema.rs` for free: a reason
        // promoted or demoted there changes this accumulator with it.
        if Confidence::from_reasons(std::slice::from_ref(&reason)) == Confidence::High {
            self.high_rungs += 1;
            if strength == RungStrength::RareOnly {
                self.rare_only_rungs += 1;
            }
        }
        self.reasons.push(reason);
    }

    /// `Some(Confidence::Medium)` when EVERY high-tier rung this candidate
    /// earned rested on corpus rarity alone; `None` when it earned none, or
    /// earned at least one on stronger evidence.
    ///
    /// One full-strength rung is enough to restore `High`: a caller who names a
    /// chunk by id, or writes a whole path, has said what they meant, and a
    /// weaker second rung alongside it takes nothing away.
    pub(crate) fn confidence_ceiling(&self) -> Option<Confidence> {
        (self.high_rungs > 0 && self.high_rungs == self.rare_only_rungs)
            .then_some(Confidence::Medium)
    }

    /// True when no rung fired at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.reasons.is_empty()
    }
}
