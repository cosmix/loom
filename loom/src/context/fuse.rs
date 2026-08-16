//! Reciprocal-rank fusion for independently ranked channel lists.

use crate::context::rank::RankedCandidate;
use crate::context::schema::ChunkId;
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// Rank offset used by reciprocal-rank fusion.
pub const RRF_K: f32 = 60.0;

#[derive(Debug, Clone)]
struct Accumulator {
    candidate: RankedCandidate,
    best_rank: usize,
    score: f32,
}

/// Reciprocal rank fusion over per-channel ranked lists.
///
/// Each list contributes `1 / (RRF_K + rank)` for its 1-based positions.
pub fn fuse(lists: &[Vec<RankedCandidate>]) -> Vec<RankedCandidate> {
    let mut fused: BTreeMap<ChunkId, Accumulator> = BTreeMap::new();
    for list in lists {
        for (position, candidate) in list.iter().enumerate() {
            let rank = position + 1;
            let contribution = 1.0 / (RRF_K + rank as f32);
            if let Some(entry) = fused.get_mut(&candidate.id) {
                entry.score += contribution;
                for reason in &candidate.reasons {
                    if !entry.candidate.reasons.contains(reason) {
                        entry.candidate.reasons.push(*reason);
                    }
                }
                if rank < entry.best_rank {
                    entry.best_rank = rank;
                    entry.candidate.channel = candidate.channel;
                }
            } else {
                fused.insert(
                    candidate.id.clone(),
                    Accumulator {
                        candidate: candidate.clone(),
                        best_rank: rank,
                        score: contribution,
                    },
                );
            }
        }
    }
    let mut output: Vec<RankedCandidate> = fused
        .into_values()
        .map(|entry| RankedCandidate {
            score: entry.score,
            ..entry.candidate
        })
        .collect();
    output.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    output
}
