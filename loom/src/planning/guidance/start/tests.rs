use super::*;
use std::fs;
use tempfile::TempDir;

fn fixture() -> (TempDir, Project) {
    let temp = tempfile::tempdir().unwrap();
    for (path, text) in [
        (
            "product/f/index.md",
            "# Vision\n\n## Specification sections\n\n\
            | # | Section |\n| --- | --- |\n| 05 | Concept mode |\n",
        ),
        ("product/f/section_05.md", "# Concept mode\n"),
        ("plans/f/index.md", "# Metaplan\n"),
        ("loom/knowledge/index.md", "# Knowledge\n"),
    ] {
        let path = temp.path().join("doc").join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let project = Project::load(temp.path()).unwrap();
    (temp, project)
}

mod classification;
mod labels;
mod lookup;
