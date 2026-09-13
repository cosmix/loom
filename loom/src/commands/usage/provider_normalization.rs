use std::collections::BTreeMap;
use std::path::Path;

use super::provider_types::{NormalizedEvent, ProvenanceStatus};

pub(crate) const UNKNOWN_PROJECT: &str = "unknown";

pub(crate) fn project_basename(path: Option<&Path>) -> String {
    path.and_then(Path::to_str)
        .and_then(path_basename)
        .unwrap_or(UNKNOWN_PROJECT)
        .to_owned()
}

fn path_basename(path: &str) -> Option<&str> {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let basename = trimmed.rsplit(['/', '\\']).next()?;
    (!basename.is_empty() && basename != "." && basename != "..").then_some(basename)
}

pub(crate) fn classify_by_id<K>(
    rows: &mut [NormalizedEvent],
    key: impl Fn(&NormalizedEvent) -> Option<K>,
) where
    K: Ord,
{
    let mut groups: BTreeMap<K, Vec<usize>> = BTreeMap::new();
    for (index, row) in rows.iter().enumerate() {
        if let Some(key) = key(row) {
            groups.entry(key).or_default().push(index);
        }
    }
    for indices in groups.into_values().filter(|indices| indices.len() > 1) {
        classify_group(rows, &indices);
    }
}

fn classify_group(rows: &mut [NormalizedEvent], indices: &[usize]) {
    let first = &rows[indices[0]];
    let exact = indices.iter().skip(1).all(|index| {
        rows[*index].event_timestamp == first.event_timestamp && rows[*index].tokens == first.tokens
    });
    for index in indices.iter().copied().skip(usize::from(exact)) {
        rows[index].provenance = if exact {
            ProvenanceStatus::DuplicateExact
        } else {
            ProvenanceStatus::AmbiguousConflict
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_name_is_basename_only_for_native_and_foreign_separators() {
        assert_eq!(
            project_basename(Some(Path::new("/private/nested/project"))),
            "project"
        );
        assert_eq!(
            project_basename(Some(Path::new(r"C:\private\nested\project"))),
            "project"
        );
    }

    #[test]
    fn missing_or_root_project_path_is_unknown() {
        assert_eq!(project_basename(None), UNKNOWN_PROJECT);
        assert_eq!(project_basename(Some(Path::new("/"))), UNKNOWN_PROJECT);
    }
}
