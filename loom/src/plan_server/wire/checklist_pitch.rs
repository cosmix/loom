//! Extract the home-card pitch from a vision or metaplan's first prose paragraph.

use std::path::Path;

use crate::fs::knowledge::splice::atx_heading;
use crate::planning::kinds::vision_path;
use crate::planning::project::Project;

pub(super) fn feature_pitch(project: &Project, feature: &str) -> Option<String> {
    let path = vision_path(feature);
    if project.docs.contains_key(&path) {
        return document_pitch(project, &path);
    }
    let path = &project.feature(feature)?.metaplans.first()?.0;
    document_pitch(project, path)
}

fn document_pitch(project: &Project, path: &Path) -> Option<String> {
    let doc = project.docs.get(path)?;
    let heading = doc
        .sections
        .headings
        .iter()
        .find(|heading| heading.level == 1)?;
    // Keep the pitch consistent with this snapshot until the watcher refreshes it.
    let body = project
        .text(path)?
        .lines()
        .skip(heading.span.start_line)
        .take(heading.span.end_line - heading.span.start_line)
        .collect::<Vec<_>>()
        .join("\n");
    first_paragraph(&body)
}

fn first_paragraph(body: &str) -> Option<String> {
    let lines: Vec<_> = body.lines().collect();
    let mut paragraph = Vec::new();
    let mut fence = None;
    let mut skipping = false;
    for (index, source) in lines.iter().enumerate().skip(1) {
        let line = source.trim();
        if in_fence(line, &mut fence) {
            if !paragraph.is_empty() {
                break;
            }
            skipping = false;
            continue;
        }
        let heading = atx_heading(line).is_some() || underline(line);
        let next = lines.get(index + 1).copied().unwrap_or_default();
        if line.is_empty()
            || heading
            || non_prose(source, next)
            || (paragraph.is_empty() && line.starts_with("Status:"))
        {
            if !paragraph.is_empty() {
                break;
            }
            skipping = !line.is_empty() && !heading;
            continue;
        }
        if !skipping {
            paragraph.push(line);
        }
    }
    if paragraph.is_empty() {
        return None;
    }
    let mut pitch = paragraph.join(" ");
    if let Some((end, _)) = pitch.char_indices().nth(280) {
        // Reserve one character for the ellipsis within the card's 280-character limit.
        let end = pitch[..end].char_indices().next_back()?.0;
        pitch.truncate(end);
        pitch.push('…');
    }
    Some(pitch)
}

fn in_fence(line: &str, fence: &mut Option<(char, usize)>) -> bool {
    let delimiter = line
        .chars()
        .next()
        .filter(|ch| matches!(ch, '`' | '~'))
        .map(|ch| (ch, line.chars().take_while(|next| *next == ch).count()))
        .filter(|(_, count)| *count >= 3);
    match (*fence, delimiter) {
        (None, Some(delimiter)) => *fence = Some(delimiter),
        (Some((ch, count)), Some((close, length)))
            if ch == close && length >= count && line[length..].trim().is_empty() =>
        {
            *fence = None;
        }
        (None, None) => return false,
        _ => {}
    }
    true
}

fn underline(line: &str) -> bool {
    let line = line.trim();
    !line.is_empty() && (line.chars().all(|ch| ch == '=') || line.chars().all(|ch| ch == '-'))
}

fn non_prose(source: &str, next: &str) -> bool {
    let line = source.trim();
    let unordered = line
        .strip_prefix(['-', '*', '+'])
        .is_some_and(|rest| rest.starts_with([' ', '\t']));
    let rest = line.trim_start_matches(|ch: char| ch.is_ascii_digit());
    let ordered = rest.len() < line.len()
        && rest
            .strip_prefix(['.', ')'])
            .is_some_and(|rest| rest.starts_with([' ', '\t']));
    let table = line.starts_with('|')
        || (line.contains('|')
            && next.contains('|')
            && next.contains("---")
            && next
                .chars()
                .all(|ch| matches!(ch, '-' | ':' | '|' | ' ' | '\t')));
    source.starts_with("    ")
        || source.starts_with('\t')
        || line.starts_with('>')
        || unordered
        || ordered
        || table
        || underline(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitch_skips_the_whole_status_paragraph() {
        let body = concat!(
            "# Demo\n\nStatus: agreed vision (2026-10-26).\n",
            "This continuation is also status metadata.\n\nA browser app for plans.\n",
        );

        assert_eq!(
            first_paragraph(body).as_deref(),
            Some("A browser app for plans.")
        );
    }

    #[test]
    fn pitch_uses_the_first_metaplan_without_a_vision() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("doc/plans/tokens/index.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            "# Token Optimization Metaplan\n\nThis metaplan orders six plans.\n",
        )
        .unwrap();
        let project = Project::load(temp.path()).unwrap();

        assert_eq!(
            feature_pitch(&project, "tokens").as_deref(),
            Some("This metaplan orders six plans.")
        );
    }

    #[test]
    fn pitch_prefers_the_vision_over_a_metaplan() {
        let temp = tempfile::tempdir().unwrap();
        for (relative, text) in [
            (
                "doc/product/demo/index.md",
                "# Demo\n\nStatus: agreed vision (2026-10-26).\n\nA browser app for plans.\n",
            ),
            (
                "doc/plans/demo/index.md",
                "# Demo Metaplan\n\nThis metaplan orders six plans.\n",
            ),
        ] {
            let path = temp.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let project = Project::load(temp.path()).unwrap();

        assert_eq!(
            feature_pitch(&project, "demo").as_deref(),
            Some("A browser app for plans.")
        );
    }

    #[test]
    fn pitch_uses_cached_vision_until_refresh() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(vision_path("demo"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# Vision\n\nCached pitch.\n").unwrap();
        let project = Project::load(temp.path()).unwrap();

        std::fs::write(&path, "# Vision\n\nEdited pitch.\n").unwrap();
        assert_eq!(
            feature_pitch(&project, "demo").as_deref(),
            Some("Cached pitch.")
        );

        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            feature_pitch(&project, "demo").as_deref(),
            Some("Cached pitch.")
        );
    }
}
