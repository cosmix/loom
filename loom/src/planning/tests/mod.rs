use std::fs;

use tempfile::TempDir;

mod briefs_tests;
mod discover_tests;
mod implements_tests;
mod kinds_tests;
mod metaplan_tests;
mod pressure_tests;
mod problems_tests;
mod project_tests;
mod runtime_tests;
mod slug_tests;
mod vision_tests;
mod xref_tests;

/// A temp repository holding an empty `doc/` and the given files, parents created.
fn tree<B: AsRef<[u8]>>(files: &[(&str, B)]) -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("doc")).unwrap();
    for (rel, bytes) in files {
        let path = temp.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    temp
}

/// The vision's text with a specification table listing `numbers`.
fn vision(numbers: &[u32]) -> String {
    let rows: String = numbers
        .iter()
        .map(|n| format!("| {n:02} | S{n} |\n"))
        .collect();
    format!("# Vision\n\n## Specification sections\n\n| # | Name |\n| --- | --- |\n{rows}")
}
