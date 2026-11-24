//! Regression gate for source-file and function size limits.

#[path = "maintainability/baseline.rs"]
mod baseline;
#[path = "maintainability/lexer.rs"]
mod lexer;
#[path = "maintainability/scanner.rs"]
mod scanner;
#[path = "maintainability/tighten.rs"]
mod tighten;

use std::path::{Path, PathBuf};

use loom::fs::safe_replace::replace_no_follow;

const BASELINE_FILE: &str = "maintainability-baseline.txt";

#[test]
fn repository_maintainability_debt_does_not_grow_or_go_stale() {
    let crate_root = crate_root();
    let measurements = scanner::scan_repository(&crate_root)
        .unwrap_or_else(|error| panic!("maintainability scan failed: {error}"));
    let baseline_path = crate_root.join(BASELINE_FILE);
    let baseline_source = std::fs::read_to_string(&baseline_path).unwrap_or_else(|error| {
        panic!(
            "failed to read maintainability baseline {}: {error}",
            baseline_path.display()
        )
    });
    let violations = baseline::current_violations(&measurements);
    let baseline_source = tighten_baseline(&crate_root, baseline_source, &violations);
    let entries = baseline::parse(&baseline_source)
        .unwrap_or_else(|errors| panic_with_errors("invalid maintainability baseline", errors));

    if let Err(errors) = baseline::validate(&entries, &violations) {
        panic_with_errors("maintainability baseline mismatch", errors);
    }
}

fn tighten_baseline(
    crate_root: &Path,
    source: String,
    violations: &[baseline::Violation],
) -> String {
    if std::env::var("LOOM_LEDGER_TIGHTEN").as_deref() != Ok("1") {
        return source;
    }

    let (tightened, changed) = tighten::tighten(&source, violations);
    if changed > 0 {
        let relative = Path::new(BASELINE_FILE);
        let path = crate_root.join(relative);
        replace_no_follow(crate_root, relative, tightened.as_bytes()).unwrap_or_else(|error| {
            panic!(
                "failed to write maintainability baseline {}: {error}",
                path.display()
            )
        });
        eprintln!(
            "maintainability: lowered or removed {changed} ledger row(s) in {}",
            path.display()
        );
    }
    tightened
}

fn crate_root() -> PathBuf {
    option_env!("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("current directory should be available"))
}

fn panic_with_errors(context: &str, errors: Vec<String>) -> ! {
    panic!("{context}:\n- {}", errors.join("\n- "))
}
