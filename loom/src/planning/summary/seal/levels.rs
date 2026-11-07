//! The two levels of a plan summary draft. The opening level comes first: headings and
//! paragraphs whose sentences each cite whole paragraphs of this summary's own detail level, in
//! order. The detail level follows: paragraphs whose sentences cite the plan.
//!
//! Sentence ids are hashes an agent cannot compute, so every refusal that needs them lists the
//! draft's detail sentences by id with their first words; the agent writes the detail level,
//! seals, and copies the ids from the refusal into the opening level.

use std::collections::BTreeSet;

use crate::context::untrusted::terminal_safe;
use crate::planning::summary::format::{
    cites_own, Block, FormatError, Link, LinkTarget, PlanCaps, Sentence, SummaryBody,
    MAX_OPENING_PARAGRAPH,
};
use crate::planning::summary::store;

use super::restate;

/// Words of a detail sentence a refusal quotes beside its id.
const QUOTED_WORDS: usize = 6;

const ORDER: &str = "the opening level comes first: headings and opening paragraphs end where the \
                     detail level begins";

/// A plan draft's sentences sorted into its levels.
struct Draft<'a> {
    id: &'a str,
    /// Every sentence citing this summary, in order: the opening level.
    opening: Vec<&'a Sentence>,
    /// The detail sentences of each paragraph holding any, in order: the detail level.
    paragraphs: Vec<Vec<&'a Sentence>>,
}

impl<'a> Draft<'a> {
    fn new(id: &'a str, body: &'a SummaryBody) -> Self {
        let mut draft = Draft {
            id,
            opening: Vec::new(),
            paragraphs: Vec::new(),
        };
        for block in &body.blocks {
            let Block::Paragraph(sentences) = block else {
                continue;
            };
            let (opening, detail): (Vec<&Sentence>, Vec<&Sentence>) =
                sentences.iter().partition(|s| cites_own(id, s));
            draft.opening.extend(opening);
            if !detail.is_empty() {
                draft.paragraphs.push(detail);
            }
        }
        draft
    }

    fn detail(&self) -> impl Iterator<Item = &'a Sentence> + '_ {
        self.paragraphs.iter().flatten().copied()
    }

    fn is_detail(&self, id: &str) -> bool {
        self.detail().any(|s| s.id == id)
    }

    /// `detail sentences: s-… "First six words of it", …`: the ids an opening sentence cites.
    fn detail_list(&self) -> String {
        let items: Vec<String> = self
            .detail()
            .map(|s| format!("{} \"{}\"", s.id, first_words(&s.text)))
            .collect();
        if items.is_empty() {
            "detail sentences: none".to_string()
        } else {
            format!("detail sentences: {}", items.join(", "))
        }
    }
}

fn first_words(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().take(QUOTED_WORDS).collect();
    terminal_safe(&words.join(" "))
}

/// The ids of this summary's sentences that `sentence` cites, in link order.
fn own_targets<'s>(summary_id: &str, sentence: &'s Sentence) -> Vec<&'s str> {
    let targets = sentence
        .links
        .iter()
        .filter_map(|link| match &link.resolved {
            LinkTarget::Sentence { summary, id } if summary == summary_id => Some(id.as_str()),
            _ => None,
        });
    targets.collect()
}

fn at(sentence: &Sentence, message: String) -> FormatError {
    FormatError {
        line: sentence.line + 1,
        message,
    }
}

/// The level problems of the draft of summary `id`, its levels held to `caps`; none unless `id`
/// is a plan summary.
pub(super) fn check(id: &str, body: &SummaryBody, caps: PlanCaps) -> Vec<FormatError> {
    if !store::is_plan(id) {
        return Vec::new();
    }
    let draft = Draft::new(id, body);
    let mut errors = limits(&draft, caps);
    if draft.opening.is_empty() {
        errors.push(FormatError {
            line: 1,
            message: format!(
                "a plan summary opens with sentences that cite its detail sentences as \
                 [d](#s-…); {}",
                draft.detail_list()
            ),
        });
        return errors;
    }
    errors.extend(order(id, body));
    errors.extend(crowded(id, body));
    for sentence in &draft.opening {
        errors.extend(opening_problems(&draft, sentence));
    }
    errors.extend(unopened(&draft));
    errors
}

/// More opening or detail sentences than their caps, reported at the first sentence over.
fn limits(draft: &Draft<'_>, caps: PlanCaps) -> Vec<FormatError> {
    let detail: Vec<&Sentence> = draft.detail().collect();
    let over = |sentences: &[&Sentence], cap: usize, level: &str| -> Option<FormatError> {
        let first = sentences.get(cap)?;
        Some(at(first, format!("more than {cap} {level} sentences")))
    };
    let opening = over(&draft.opening, caps.opening, "opening");
    let detail = over(&detail, caps.detail, "detail");
    opening.into_iter().chain(detail).collect()
}

/// Opening paragraphs of more than [`MAX_OPENING_PARAGRAPH`] sentences, each reported at its first
/// sentence over: the Summary page draws a paragraph as a block, so a long one is a wall of text.
fn crowded(id: &str, body: &SummaryBody) -> Vec<FormatError> {
    let message = format!(
        "more than {MAX_OPENING_PARAGRAPH} sentences in one opening paragraph; end the \
         paragraph with a blank line and group the rest by topic"
    );
    let mut errors = Vec::new();
    for block in &body.blocks {
        let Block::Paragraph(sentences) = block else {
            continue;
        };
        let mut opening = sentences.iter().filter(|s| cites_own(id, s));
        if let Some(over) = opening.nth(MAX_OPENING_PARAGRAPH) {
            errors.push(at(over, message.clone()));
        }
    }
    errors
}

/// Headings and opening paragraphs that follow the first detail paragraph, headings between the
/// last opening paragraph and the first detail paragraph, and paragraphs that mix the two levels.
fn order(id: &str, body: &SummaryBody) -> Vec<FormatError> {
    let mut lines = Vec::new();
    let mut detail_begun = false;
    // Headings since the last opening paragraph: refused once the detail level begins after them.
    let mut after_opening = Vec::new();
    for block in &body.blocks {
        match block {
            Block::Heading { line, .. } if detail_begun => lines.push(line + 1),
            Block::Heading { line, .. } => after_opening.push(line + 1),
            Block::Paragraph(sentences) => {
                lines.extend(misplaced(id, sentences, detail_begun));
                let begins_detail = sentences.iter().any(|s| !cites_own(id, s));
                if begins_detail && !detail_begun {
                    lines.append(&mut after_opening);
                }
                after_opening.clear();
                detail_begun |= begins_detail;
            }
        }
    }
    let error = |line| FormatError {
        line,
        message: ORDER.to_string(),
    };
    lines.into_iter().map(error).collect()
}

/// The line of the first sentence of a paragraph out of its level's place: an opening sentence
/// once the detail level has begun, else the first sentence whose level differs from the
/// paragraph's first.
fn misplaced(id: &str, sentences: &[Sentence], detail_begun: bool) -> Option<usize> {
    let first = cites_own(id, sentences.first()?);
    let out = if detail_begun {
        sentences.iter().find(|s| cites_own(id, s))
    } else {
        sentences.iter().find(|s| cites_own(id, s) != first)
    };
    out.map(|s| s.line + 1)
}

/// Problems of one opening sentence: a source link beside its self-links, a self-link to no
/// detail sentence, a detail sentence cited twice, part of a detail paragraph, or a restatement
/// of a sentence it opens.
fn opening_problems(draft: &Draft<'_>, sentence: &Sentence) -> Vec<FormatError> {
    let mut errors = Vec::new();
    let source = |link: &Link| matches!(link.resolved, LinkTarget::Source { .. });
    if sentence.links.iter().any(source) {
        let message = format!(
            "sentence {} cites both this summary and the plan; an opening sentence cites only \
             detail sentences",
            sentence.id
        );
        errors.push(at(sentence, message));
    }
    let targets = own_targets(draft.id, sentence);
    let (cited, unknown): (Vec<&str>, Vec<&str>) =
        targets.into_iter().partition(|id| draft.is_detail(id));
    let unknown: BTreeSet<&str> = unknown.into_iter().collect();
    for target in unknown {
        // The target is draft text (the format only checks its `s-` prefix), so it is made
        // terminal-safe here for every consumer of the refusal.
        let message = format!(
            "sentence {} cites {}, which is not a detail sentence of this summary; {}",
            sentence.id,
            terminal_safe(target),
            draft.detail_list()
        );
        errors.push(at(sentence, message));
    }
    let twice = cited
        .iter()
        .enumerate()
        .find(|(index, id)| cited[..*index].contains(id));
    if let Some((_, id)) = twice {
        let message = format!("sentence {} cites detail sentence {id} twice", sentence.id);
        errors.push(at(sentence, message));
    } else if !cited.is_empty() && !opens_whole_paragraphs(&cited, &draft.paragraphs) {
        let message = format!(
            "sentence {} opens part of a detail paragraph; cite every sentence of each paragraph \
             it opens, in order",
            sentence.id
        );
        errors.push(at(sentence, message));
    }
    errors.extend(restatements(draft, sentence, &cited));
    errors
}

/// Whether `cited` is the ids of one or more consecutive detail paragraphs, each whole and in
/// order.
fn opens_whole_paragraphs(cited: &[&str], paragraphs: &[Vec<&Sentence>]) -> bool {
    let Some(&first) = cited.first() else {
        return false;
    };
    let start = paragraphs
        .iter()
        .position(|paragraph| paragraph.first().is_some_and(|s| s.id == first));
    let Some(start) = start else {
        return false;
    };
    let mut rest = cited;
    for paragraph in &paragraphs[start..] {
        if rest.is_empty() {
            break;
        }
        let ids: Vec<&str> = paragraph.iter().map(|s| s.id.as_str()).collect();
        match rest.strip_prefix(ids.as_slice()) {
            Some(tail) => rest = tail,
            None => return false,
        }
    }
    rest.is_empty()
}

/// An opening sentence that repeats a detail sentence it opens shows the reader the same text
/// twice when it opens.
fn restatements(draft: &Draft<'_>, sentence: &Sentence, cited: &[&str]) -> Vec<FormatError> {
    let cited: BTreeSet<&str> = cited.iter().copied().collect();
    let restated = draft
        .detail()
        .filter(|detail| cited.contains(detail.id.as_str()))
        .filter(|detail| restate::restates(&sentence.text, &detail.text));
    let error = |detail: &Sentence| {
        let message = format!(
            "sentence {} restates detail sentence {}; say what the paragraph adds up to",
            sentence.id, detail.id
        );
        at(sentence, message)
    };
    restated.map(error).collect()
}

/// Detail sentences no opening sentence cites.
fn unopened(draft: &Draft<'_>) -> Vec<FormatError> {
    let opened: BTreeSet<&str> = draft
        .opening
        .iter()
        .flat_map(|s| own_targets(draft.id, s))
        .collect();
    let error = |detail: &Sentence| {
        let message = format!(
            "detail sentence {} is opened by no opening sentence",
            detail.id
        );
        at(detail, message)
    };
    draft
        .detail()
        .filter(|detail| !opened.contains(detail.id.as_str()))
        .map(error)
        .collect()
}
