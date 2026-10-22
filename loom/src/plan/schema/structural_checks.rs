//! Structural preflight checks split out of `validation.rs` to keep
//! `validate_structural_preflight` itself from growing past its recorded
//! size ceiling. The checks below feed into the same `Vec<String>`
//! `validate_structural_preflight` returns - advisory warnings, same
//! severity as its other checks, never hard errors.

use std::collections::{HashMap, HashSet, VecDeque};

use super::types::StageDefinition;

pub(super) mod declared_skills;
pub(super) mod worker_table;
#[cfg(test)]
mod worker_table_tests;

/// Warn once per stage without a `summary` (absent or blank).
///
/// The dashboard shows `summary` in place of the agent-facing `description`;
/// without it a stage is listed by name only. Applies to every stage type.
pub(super) fn check_missing_summaries(stages: &[StageDefinition]) -> Vec<String> {
    stages
        .iter()
        .filter(|stage| stage.summary.as_deref().is_none_or(|s| s.trim().is_empty()))
        .map(|stage| {
            format!(
                "Stage '{}': no summary; the dashboard shows only the stage name. \
                 Add summary: with 1-3 sentences saying what the stage delivers.",
                stage.id
            )
        })
        .collect()
}

/// Extract `doc/plans/briefs/...` path-like tokens from free-form text.
///
/// A plain substring/token scan, not a markdown-link parser: split on
/// whitespace and common delimiters, trim surrounding punctuation, and keep
/// tokens that contain the brief-directory prefix. Templates such as
/// `doc/plans/briefs/<plan>/<stage-id>/` name a layout, not a file, and are
/// skipped.
pub(crate) fn extract_brief_paths(text: &str) -> Vec<String> {
    const PREFIX: &str = "doc/plans/briefs/";
    let mut paths = Vec::new();

    for token in text.split(|c: char| {
        c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '"' | '\'' | ',' | ';' | '|')
    }) {
        let trimmed = token.trim_matches(|c: char| matches!(c, '.' | ',' | ':' | '`'));
        if let Some(idx) = trimmed.find(PREFIX) {
            // A backtick closes a code span, so markdown after it (`dir/`.** in
            // bold) is not part of the path and must not reach the glob expander.
            let path = trimmed[idx..].split(['#', '`']).next().unwrap_or_default();
            if path.len() > PREFIX.len() && !path.contains(['<', '>']) {
                paths.push(path.to_string());
            }
        }
    }

    paths
}

/// Warn when two stages with no dependency relationship (in either
/// direction, transitively) declare overlapping `files:` globs. Two stages
/// with no dependency path between them can execute concurrently, and
/// concurrent writes to the same files are a lost-work hazard.
///
/// Overlap is judged on the glob patterns themselves - a string comparison
/// on each pattern's literal (non-wildcard) prefix, never filesystem
/// expansion - so the check is deterministic and cheap.
pub(super) fn check_overlapping_files_without_dependency(
    stages: &[StageDefinition],
) -> Vec<String> {
    let mut warnings = Vec::new();
    if stages.len() < 2 {
        return warnings;
    }

    let index_by_id: HashMap<&str, usize> = stages
        .iter()
        .enumerate()
        .map(|(idx, stage)| (stage.id.as_str(), idx))
        .collect();

    let closures: Vec<HashSet<usize>> = (0..stages.len())
        .map(|idx| transitive_dependencies(idx, stages, &index_by_id))
        .collect();

    for i in 0..stages.len() {
        for j in (i + 1)..stages.len() {
            // A dependency path in either direction means the two stages
            // cannot run concurrently regardless of file overlap.
            if closures[i].contains(&j) || closures[j].contains(&i) {
                continue;
            }

            for p1 in &stages[i].files {
                for p2 in &stages[j].files {
                    if glob_prefixes_overlap(p1, p2) {
                        warnings.push(format!(
                            "Stage '{}': files pattern '{}' overlaps stage '{}' files pattern \
                             '{}' with no dependency relationship between them — two stages \
                             that can run concurrently must not both write the same files",
                            stages[i].id, p1, stages[j].id, p2
                        ));
                    }
                }
            }
        }
    }

    warnings
}

/// Collect file-ownership warnings that are safe to report before workers run.
///
/// Stage-level glob overlap remains the source of DAG concurrency warnings;
/// the optional description table adds only intra-stage write-owner advice.
pub(super) fn check_file_ownership(stages: &[StageDefinition]) -> Vec<String> {
    let mut warnings = check_overlapping_files_without_dependency(stages);
    warnings.extend(worker_table::check_worker_table_ownership(stages));
    warnings.extend(check_missing_summaries(stages));
    warnings
}

/// BFS over `dependencies` to compute the set of stage indices `start`
/// transitively depends on.
///
/// `pub(super)` so `validation::v2_lints::loom_subcommands` (a descendant of
/// `plan::schema`) can reuse the same BFS instead of writing a second one.
pub(super) fn transitive_dependencies(
    start: usize,
    stages: &[StageDefinition],
    index_by_id: &HashMap<&str, usize>,
) -> HashSet<usize> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(start);

    while let Some(current) = queue.pop_front() {
        for dep in &stages[current].dependencies {
            if let Some(&dep_idx) = index_by_id.get(dep.as_str()) {
                if visited.insert(dep_idx) {
                    queue.push_back(dep_idx);
                }
            }
        }
    }

    visited
}

/// The literal, non-wildcard prefix of a glob pattern: the substring up to
/// (not including) the first wildcard character, with a trailing `/`
/// trimmed. A pattern with no wildcard yields the whole literal path.
fn glob_prefix(pattern: &str) -> &str {
    let end = pattern.find(['*', '?', '[']).unwrap_or(pattern.len());
    pattern[..end].trim_end_matches('/')
}

/// Two glob patterns overlap when one's prefix is a path-component-wise
/// prefix of the other: equal, or one equals the other plus `/` plus more.
fn glob_prefixes_overlap(p1: &str, p2: &str) -> bool {
    let a = glob_prefix(p1);
    let b = glob_prefix(p2);
    // An empty literal prefix (e.g. "**/*.rs", where the wildcard starts
    // the pattern) means the match set is not bounded by any directory, so
    // it is treated as overlapping everything - "" is not a "/"-joined
    // prefix of a non-empty path, so the component-wise comparison below
    // would otherwise call it disjoint. This is deliberately conservative:
    // the heuristic compares directory prefixes only, never extensions, so
    // "**/*.rs" and "**/*.md" still warn even though they match no files in
    // common. A missed overlap costs a stage its work in a merge conflict;
    // a spurious warning costs a plan author one glance - do not "fix" this.
    if a.is_empty() || b.is_empty() {
        return true;
    }
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(
        id: &str,
        deps: &[&str],
        files: &[&str],
        description: Option<&str>,
    ) -> StageDefinition {
        let mut s = crate::plan::schema::tests::make_stage(id, id);
        s.dependencies = deps.iter().map(|d| d.to_string()).collect();
        s.files = files.iter().map(|f| f.to_string()).collect();
        s.description = description.map(|d| d.to_string());
        s
    }

    #[test]
    fn overlapping_globs_without_dependency_warn() {
        let stages = vec![
            stage("a", &[], &["src/a/**"], None),
            stage("b", &[], &["src/a/config.rs"], None),
        ];
        let warnings = check_overlapping_files_without_dependency(&stages);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("src/a/**"));
        assert!(warnings[0].contains("src/a/config.rs"));
    }

    #[test]
    fn empty_prefix_glob_overlaps_rooted_pattern() {
        let stages = vec![
            stage("a", &[], &["**/*.rs"], None),
            stage("b", &[], &["loom/src/plan/**"], None),
        ];
        let warnings = check_overlapping_files_without_dependency(&stages);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("**/*.rs"));
        assert!(warnings[0].contains("loom/src/plan/**"));
    }

    #[test]
    fn disjoint_globs_do_not_warn() {
        let stages = vec![
            stage("a", &[], &["src/a/**"], None),
            stage("b", &[], &["src/b/**"], None),
        ];
        assert!(check_overlapping_files_without_dependency(&stages).is_empty());
    }

    #[test]
    fn overlapping_globs_with_dependency_do_not_warn() {
        let stages = vec![
            stage("a", &[], &["src/a/**"], None),
            stage("b", &["a"], &["src/a/config.rs"], None),
        ];
        assert!(check_overlapping_files_without_dependency(&stages).is_empty());
    }

    #[test]
    fn transitive_dependency_suppresses_warning() {
        let stages = vec![
            stage("a", &[], &["src/a/**"], None),
            stage("b", &["a"], &[], None),
            stage("c", &["b"], &["src/a/config.rs"], None),
        ];
        assert!(check_overlapping_files_without_dependency(&stages).is_empty());
    }

    #[test]
    fn placeholder_brief_paths_are_not_references() {
        let text = "under `doc/plans/briefs/v2/<stage-id>/` and \
                    (`doc/plans/briefs/v2/fixtures/<adapter>/`, see doc/plans/briefs/v2/NOTES.md)";
        assert_eq!(
            extract_brief_paths(text),
            vec!["doc/plans/briefs/v2/NOTES.md"]
        );
    }

    #[test]
    fn markdown_after_a_code_span_is_not_part_of_the_path() {
        let text = "**Briefs live outside `doc/plans/briefs/`.** See `doc/plans/briefs/v2/a.md`.**";
        assert_eq!(extract_brief_paths(text), vec!["doc/plans/briefs/v2/a.md"]);
    }

    #[test]
    fn missing_and_blank_summaries_each_warn() {
        let mut absent = crate::plan::schema::tests::make_stage("a", "A");
        absent.summary = None;
        let mut blank = crate::plan::schema::tests::make_stage("b", "B");
        blank.summary = Some("  \n ".to_string());

        let warnings = check_missing_summaries(&[absent, blank]);

        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].starts_with("Stage 'a': no summary;"));
        assert!(warnings[1].starts_with("Stage 'b': no summary;"));
    }

    #[test]
    fn present_summary_is_silent() {
        let mut s = crate::plan::schema::tests::make_stage("a", "A");
        s.summary = Some("Ships the thing.".to_string());
        assert!(check_missing_summaries(&[s]).is_empty());
    }
}
