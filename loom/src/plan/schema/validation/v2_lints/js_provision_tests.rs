use std::path::Path;

use super::covered;

fn covers(package: &str, dirs: &[&str]) -> bool {
    covered(Path::new(package), dirs.iter().copied())
}

#[test]
fn root_spellings_cover_every_package() {
    assert!(covers("web", &["."]));
    assert!(covers("web", &[""]));
    assert!(covers("web/app", &["./web"]));
    assert!(!covers("api", &["web"]));
}

#[test]
fn parent_components_fold_before_the_comparison() {
    assert!(covers("api", &["web/../api"]));
    assert!(covers("api/../web", &["web"]));
}

#[test]
fn a_directory_climbing_out_of_the_repository_covers_nothing() {
    assert!(!covers("web", &["../web"]));
    assert!(!covers("../web", &["../web"]));
}
