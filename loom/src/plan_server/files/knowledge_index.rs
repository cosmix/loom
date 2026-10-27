//! Parse the tier-2 topic tables in the knowledge index.

/// A topic and its link relative to the knowledge directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexTopic {
    pub category: String,
    pub slug: String,
    pub path: String,
    pub blurb: String,
}

/// Return topics in index order, ignoring summaries and malformed rows.
pub fn parse_index(text: &str) -> Vec<IndexTopic> {
    let mut topics = Vec::new();
    let mut in_topics = false;
    let mut category = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with("## ") {
            in_topics = line.starts_with("## Tier 2 ");
            category = None;
        } else if in_topics && line.starts_with("### ") {
            category = Some(line[4..].trim());
        } else if in_topics {
            if let Some(topic) = category.and_then(|name| parse_row(line, name)) {
                topics.push(topic);
            }
        }
    }
    topics
}

fn parse_row(line: &str, category: &str) -> Option<IndexTopic> {
    let cells = table_cells(line)?;
    if cells.len() != 3 || cells[2].trim().parse::<usize>().is_err() {
        return None;
    }
    let link = cells[0].trim().strip_prefix('[')?;
    let (slug, path) = link.split_once("](")?;
    let path = path.strip_suffix(')')?;
    if slug.is_empty() || category.is_empty() || path != format!("{category}/{slug}.md") {
        return None;
    }
    Some(IndexTopic {
        category: category.to_owned(),
        slug: slug.to_owned(),
        path: path.to_owned(),
        blurb: cells[1].trim().to_owned(),
    })
}

fn table_cells(line: &str) -> Option<Vec<String>> {
    let body = line.strip_prefix('|')?.strip_suffix('|')?;
    let mut cells = vec![String::new()];
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'|') {
            chars.next();
            cells.last_mut()?.push('|');
        } else if ch == '|' {
            cells.push(String::new());
        } else {
            cells.last_mut()?.push(ch);
        }
    }
    Some(cells)
}
