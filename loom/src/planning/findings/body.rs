//! The lines of one ledger entry under its heading.

/// The keys a metadata line starts with.
pub(super) const META_KEYS: [&str; 3] = ["About:", "Source:", "Status:"];

/// An entry body split into its metadata lines and its text.
///
/// The metadata is the run of `About:`, `Source:` and `Status:` lines (blank lines allowed
/// between them) that opens the body; the text starts at the first other line. A text line that
/// looks like metadata is therefore text once that first line has begun it.
pub(super) struct Body<'a> {
    /// The first `About:` value.
    pub about: Option<&'a str>,
    /// The first `Source:` value.
    pub source: Option<&'a str>,
    /// The index and value of the first `Status:` line.
    pub status: Option<(usize, &'a str)>,
    /// The index of the last metadata line.
    pub last_meta: Option<usize>,
    /// The index of the first text line; the slice's length when there is no text.
    pub text_start: usize,
}

/// Split the lines under an entry heading (line endings already removed).
pub(super) fn scan<'a>(lines: &[&'a str]) -> Body<'a> {
    let mut body = Body {
        about: None,
        source: None,
        status: None,
        last_meta: None,
        text_start: lines.len(),
    };
    for (index, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let Some(key) = META_KEYS.into_iter().find(|key| line.starts_with(key)) else {
            body.text_start = index;
            break;
        };
        let value = line[key.len()..].trim();
        match key {
            "About:" => body.about = body.about.or(Some(value)),
            "Source:" => body.source = body.source.or(Some(value)),
            _ => body.status = body.status.or(Some((index, value))),
        }
        body.last_meta = Some(index);
    }
    body
}
