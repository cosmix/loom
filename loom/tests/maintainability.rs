//! Regression gate for source-file and function size limits.

#[path = "maintainability/baseline.rs"]
mod baseline;
#[path = "maintainability/lexer.rs"]
mod lexer;
#[path = "maintainability/scanner.rs"]
mod scanner;

use std::path::PathBuf;

#[test]
fn repository_maintainability_debt_does_not_grow_or_go_stale() {
    let crate_root = crate_root();
    let measurements = scanner::scan_repository(&crate_root)
        .unwrap_or_else(|error| panic!("maintainability scan failed: {error}"));
    let baseline_path = crate_root.join("maintainability-baseline.txt");
    let baseline_source = std::fs::read_to_string(&baseline_path).unwrap_or_else(|error| {
        panic!(
            "failed to read maintainability baseline {}: {error}",
            baseline_path.display()
        )
    });
    let entries = baseline::parse(&baseline_source)
        .unwrap_or_else(|errors| panic_with_errors("invalid maintainability baseline", errors));
    let violations = baseline::current_violations(&measurements);

    if let Err(errors) = baseline::validate(&entries, &violations) {
        panic_with_errors("maintainability baseline mismatch", errors);
    }
}

fn crate_root() -> PathBuf {
    option_env!("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("current directory should be available"))
}

fn panic_with_errors(context: &str, errors: Vec<String>) -> ! {
    panic!("{context}:\n- {}", errors.join("\n- "))
}
