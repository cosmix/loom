//! The semantic prefixes a note's text may open with.
//!
//! Prefix conventions let validation enforce note shapes and let pending
//! reports group notes by the same classification.

use anyhow::{bail, Result};

/// Which documented prefix convention a note's text opens with, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NotePrefix {
    /// `mistake: tried X because ... Failed because Y. Prevention: ... Fix: Z`
    Mistake,
    /// `stale-knowledge: <file>#<heading> claims X; ... Correction: ...`
    StaleKnowledge { file: String, heading: String },
    /// `found: ...`, `gotcha: ...` or `found/gotcha: ... in file:line`
    Found,
    /// No recognized prefix; the note is free-form.
    None,
}

impl NotePrefix {
    /// Parse the prefix convention a note's text opens with.
    ///
    /// A `stale-knowledge:` note splits on the FIRST `#` to separate the
    /// file from the heading, since the heading itself may contain `#` or
    /// `:`; the heading then runs up to the first ` claims ` or `;`,
    /// whichever comes first. A `stale-knowledge:` note that never reaches
    /// a `#` fails to parse and reports as [`NotePrefix::None`] - the
    /// shape check in `record_kind` treats that the same as a missing
    /// `Correction:`.
    pub(super) fn parse(text: &str) -> NotePrefix {
        if text.starts_with("mistake:") {
            return NotePrefix::Mistake;
        }
        if let Some(rest) = text.strip_prefix("stale-knowledge:") {
            return Self::parse_stale_knowledge(rest);
        }
        if ["found:", "gotcha:", "found/gotcha:"]
            .iter()
            .any(|prefix| text.starts_with(prefix))
        {
            return NotePrefix::Found;
        }
        NotePrefix::None
    }

    fn parse_stale_knowledge(rest: &str) -> NotePrefix {
        let Some((file, after_hash)) = rest.trim_start().split_once('#') else {
            return NotePrefix::None;
        };
        let claims_at = after_hash.find(" claims ");
        let semicolon_at = after_hash.find(';');
        let end = match (claims_at, semicolon_at) {
            (Some(claims), Some(semicolon)) => claims.min(semicolon),
            (Some(claims), None) => claims,
            (None, Some(semicolon)) => semicolon,
            (None, None) => after_hash.len(),
        };
        let file = file.trim();
        let heading = after_hash[..end].trim();
        if file.is_empty() || heading.is_empty() {
            return NotePrefix::None;
        }
        NotePrefix::StaleKnowledge {
            file: file.to_string(),
            heading: heading.to_string(),
        }
    }
}

/// Validate the shape promised by the parsed prefix before recording a note.
pub(super) fn validate_note_shape(prefix: &NotePrefix, text: &str) -> Result<()> {
    if matches!(prefix, NotePrefix::Mistake) && !text.contains("Prevention:") {
        bail!(
            "a `mistake:` note must contain `Prevention:`. Expected shape:\n\
             mistake: tried X because [misleading signal]. Failed because Y.\n\
             Prevention: [detection rule]. Fix: Z"
        );
    }
    if text.starts_with("stale-knowledge:") {
        let parsed_ok = matches!(prefix, NotePrefix::StaleKnowledge { .. });
        if !parsed_ok || !text.contains("Correction:") {
            bail!(
                "a `stale-knowledge:` note must parse to `<file>#<heading>` and contain `Correction:`. Expected shape:\n\
                 stale-knowledge: <file>#<heading> claims X; the tree does Y (file:line).\n\
                 Correction: <replacement text>"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_note_shape, NotePrefix};

    #[test]
    fn mistake_prefix_is_recognized() {
        let text = "mistake: tried X because Y looked right. Failed because Z. Prevention: check W first. Fix: did V";
        assert_eq!(NotePrefix::parse(text), NotePrefix::Mistake);
    }

    #[test]
    fn stale_knowledge_splits_on_first_hash_only() {
        let text = "stale-knowledge: doc/loom/knowledge/patterns/foo.md#Section With # And : Marks claims X; the tree does Y (file.rs:12). Correction: use Y instead";
        assert_eq!(
            NotePrefix::parse(text),
            NotePrefix::StaleKnowledge {
                file: "doc/loom/knowledge/patterns/foo.md".to_string(),
                heading: "Section With # And : Marks".to_string(),
            }
        );
    }

    #[test]
    fn stale_knowledge_stops_at_semicolon_when_there_is_no_claims_word() {
        let text = "stale-knowledge: file.md#Heading; the tree does Y. Correction: fix it";
        assert_eq!(
            NotePrefix::parse(text),
            NotePrefix::StaleKnowledge {
                file: "file.md".to_string(),
                heading: "Heading".to_string(),
            }
        );
    }

    #[test]
    fn stale_knowledge_without_a_hash_fails_to_parse() {
        let text = "stale-knowledge: file.md claims X; Correction: fix it";
        assert_eq!(NotePrefix::parse(text), NotePrefix::None);
    }

    #[test]
    fn found_and_gotcha_prefixes_are_one_class() {
        for text in [
            "found: surprising thing in src/foo.rs:10",
            "gotcha: surprising thing in src/foo.rs:10",
            "found/gotcha: surprising thing in src/foo.rs:10",
        ] {
            assert_eq!(NotePrefix::parse(text), NotePrefix::Found, "{text}");
        }
        assert_eq!(NotePrefix::parse("foundation: x"), NotePrefix::None);
    }

    #[test]
    fn free_form_text_has_no_prefix() {
        assert_eq!(NotePrefix::parse("just a plain note"), NotePrefix::None);
    }

    /// Parse `text` and validate it in one step, matching how `record_kind`
    /// drives [`NotePrefix::parse`] and [`validate_note_shape`] together.
    fn parse_and_validate(text: &str) -> Result<(), anyhow::Error> {
        validate_note_shape(&NotePrefix::parse(text), text)
    }

    #[test]
    fn mistake_without_prevention_is_refused() {
        let error =
            parse_and_validate("mistake: tried X because it looked right. Failed because Y.")
                .unwrap_err();
        assert!(error.to_string().contains("Prevention:"));
    }

    #[test]
    fn mistake_with_prevention_passes() {
        parse_and_validate(
            "mistake: tried X because Y. Failed because Z. Prevention: check W. Fix: did V",
        )
        .unwrap();
    }

    #[test]
    fn stale_knowledge_without_correction_is_refused() {
        let error = parse_and_validate(
            "stale-knowledge: foo.md#Heading claims X; the tree does Y (foo.rs:1).",
        )
        .unwrap_err();
        assert!(error.to_string().contains("Correction:"));
    }

    #[test]
    fn stale_knowledge_without_a_hash_is_refused() {
        let error =
            parse_and_validate("stale-knowledge: foo.md claims X; Correction: fix it").unwrap_err();
        assert!(error.to_string().contains("<file>#<heading>"));
    }

    #[test]
    fn stale_knowledge_with_hash_and_correction_passes() {
        parse_and_validate(
            "stale-knowledge: foo.md#Heading claims X; the tree does Y (foo.rs:1). Correction: use Y",
        )
        .unwrap();
    }

    #[test]
    fn found_gotcha_has_no_shape_requirement() {
        parse_and_validate("found/gotcha: surprising thing in src/foo.rs:10").unwrap();
    }

    #[test]
    fn free_form_note_has_no_shape_requirement() {
        parse_and_validate("just a plain note").unwrap();
    }
}
