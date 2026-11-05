//! The two levels of a summary body. Opening sentences cite sentences of their own summary
//! (written `[d](#s-xxxxxxxx)`); the detail sentences they open cite the source. Only a plan
//! summary is sealed with two levels, but any body splits, so a hand edit cannot break a reader.

use super::{Block, LinkTarget, Sentence};

/// Whether `sentence` cites a sentence of its own summary `summary_id`.
pub fn cites_own(summary_id: &str, sentence: &Sentence) -> bool {
    sentence.links.iter().any(|link| {
        matches!(&link.resolved, LinkTarget::Sentence { summary, .. } if summary == summary_id)
    })
}

/// A body split into what a reader sees first and the detail its opening sentences open to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Levels<'a> {
    /// Every heading and every paragraph holding an opening sentence, in order; every block of a
    /// one-level body.
    pub shown: Vec<&'a Block>,
    /// The sentences of every other paragraph, in order; empty for a one-level body.
    pub detail: Vec<&'a Sentence>,
}

/// Split the `blocks` of summary `summary_id` into its two levels. A body in which no sentence
/// cites its own summary has one level: everything is shown and nothing is detail.
pub fn split_levels<'a>(summary_id: &str, blocks: &'a [Block]) -> Levels<'a> {
    let opens = |sentences: &[Sentence]| sentences.iter().any(|s| cites_own(summary_id, s));
    let two_levels = blocks
        .iter()
        .any(|block| matches!(block, Block::Paragraph(sentences) if opens(sentences)));
    let mut levels = Levels {
        shown: Vec::new(),
        detail: Vec::new(),
    };
    for block in blocks {
        match block {
            Block::Paragraph(sentences) if two_levels && !opens(sentences) => {
                levels.detail.extend(sentences);
            }
            _ => levels.shown.push(block),
        }
    }
    levels
}
