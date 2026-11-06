//! Metaplan `Covers:` lines: malformed tokens and anchors that name nothing.

use std::path::Path;

use super::plans::unresolved_anchor;
use super::{problem, unlocked, Problem};
use crate::planning::metaplan::MetaplanEntry;
use crate::planning::project::Project;

pub(super) fn collect(project: &Project, found: &mut Vec<Problem>) {
    for feature in &project.features {
        for (path, entries) in &feature.metaplans {
            if !unlocked(project, path) {
                continue;
            }
            for entry in entries {
                entry_problems(project, (path, &feature.name), entry, found);
            }
        }
    }
}

fn entry_problems(
    project: &Project,
    (path, feature): (&Path, &str),
    entry: &MetaplanEntry,
    found: &mut Vec<Problem>,
) {
    let at = Some(entry.line + 1);
    let heading = &entry.heading;
    for token in &entry.bad_covers {
        let message = format!(
            "metaplan entry \"{heading}\": Covers: token `{token}` does not name a product section"
        );
        found.push(problem("covers-bad-token", path, at, message));
    }
    for section in &entry.covers {
        let Some(anchor) = &section.anchor else {
            continue;
        };
        if let Some(reason) = unresolved_anchor(project, feature, section.number, anchor) {
            let message = format!("metaplan entry \"{heading}\": Covers: {reason}");
            found.push(problem("covers-anchor-missing", path, at, message));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::planning::project::Project;

    fn load(metaplan: &str) -> Project {
        let temp = tempfile::tempdir().unwrap();
        let files = [
            (
                "doc/product/a/index.md",
                "# V\n\n## Specification sections\n\n| # | Name |\n| --- | --- |\n| 01 | S1 |\n",
            ),
            ("doc/product/a/section_01.md", "# S1\n\n## One\n\nText.\n"),
            ("doc/plans/a/index.md", metaplan),
        ];
        for (rel, text) in files {
            let path = temp.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Project::load(temp.path()).unwrap()
    }

    fn messages(project: &Project, code: &str) -> Vec<(Option<usize>, String)> {
        let wanted = project.problems.iter().filter(|p| p.code == code);
        wanted.map(|p| (p.line, p.message.clone())).collect()
    }

    #[test]
    fn resolving_covers_report_nothing() {
        let project = load("# M\n\n## 1. A (`PLAN-a.md`)\n\nCovers: section_01#one, section_01\n");
        assert!(messages(&project, "covers-bad-token").is_empty());
        assert!(messages(&project, "covers-anchor-missing").is_empty());
    }

    #[test]
    fn bad_tokens_and_missing_anchors_are_reported_on_the_entry() {
        let project = load(
            "# M\n\n## 1. A (`PLAN-a.md`)\n\nCovers: section_01#gone, section_04#x, vision, Up\n",
        );
        let bad = messages(&project, "covers-bad-token");
        assert_eq!(bad.len(), 2, "{bad:?}");
        assert!(bad[0].1.contains("`vision`") && bad[1].1.contains("`Up`"));
        assert_eq!(bad[0].0, Some(3));
        let missing = messages(&project, "covers-anchor-missing");
        assert_eq!(missing.len(), 2, "{missing:?}");
        assert!(missing[0].1.contains("no heading `gone`"), "{missing:?}");
        assert!(
            missing[1].1.contains("section_04 does not exist"),
            "{missing:?}"
        );
    }
}
