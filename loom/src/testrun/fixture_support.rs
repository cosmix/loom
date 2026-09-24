//! Test-time access to the recorded runner output under `testrun/fixtures/`.
//! Each scenario is `<scenario>.stdout`, `<scenario>.stderr` and
//! `<scenario>.meta`, whose `exit:` line holds the exit code.

use std::fs;
use std::path::PathBuf;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/testrun/fixtures/");

fn adapter_dir(adapter: &str) -> PathBuf {
    PathBuf::from(FIXTURES).join(adapter)
}

/// `(stdout, stderr, exit code)` of `scenario` (`one-pass`, `one-pass.nocolor`)
/// recorded for `adapter`. Panics naming the file when one is unreadable.
pub(crate) fn load(adapter: &str, scenario: &str) -> (String, String, Option<i32>) {
    let read = |extension: &str| {
        let file = format!("{scenario}.{extension}");
        let path = adapter_dir(adapter).join(file);
        match fs::read(&path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) => panic!("reading fixture {}: {error}", path.display()),
        }
    };
    let exit_code = read("meta")
        .lines()
        .find_map(|line| line.strip_prefix("exit:"))
        .and_then(|code| code.trim().parse().ok());
    (read("stdout"), read("stderr"), exit_code)
}

/// The scenario names recorded for `adapter`, sorted: the stem of every
/// `.meta` file, variants included (`one-pass.nocolor`). Empty when the
/// adapter has no fixture directory.
pub(crate) fn scenarios(adapter: &str) -> Vec<String> {
    let Ok(entries) = fs::read_dir(adapter_dir(adapter)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let file_name = entry.file_name();
            file_name
                .to_str()?
                .strip_suffix(".meta")
                .map(str::to_string)
        })
        .collect();
    names.sort();
    names
}
