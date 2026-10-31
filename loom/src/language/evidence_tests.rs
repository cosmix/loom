use super::{detect_project_languages, DetectedLanguage};
use crate::skills::project::ProjectProfile;
use std::fs;
use tempfile::TempDir;

fn assert_evidence(name: &str, contents: &str, expected: Option<DetectedLanguage>, kind: &str) {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join(name), contents).unwrap();
    let languages = detect_project_languages(temp.path());
    assert_eq!(
        languages,
        expected.into_iter().collect::<Vec<_>>(),
        "{name}"
    );
    let profile = ProjectProfile::discover(temp.path());
    assert!(
        profile
            .types
            .iter()
            .any(|entry| entry.kind == kind && entry.path.as_os_str().is_empty()),
        "{name}: {:?}",
        profile.types
    );
}

#[test]
fn root_and_project_share_manifest_evidence() {
    use DetectedLanguage::*;
    for (name, content, expected, kind) in [
        ("Cargo.toml", "", Some(Rust), "rust"),
        ("go.mod", "", Some(Go), "golang"),
        ("pom.xml", "", Some(Java), "java"),
        ("Gemfile", "", Some(Ruby), "ruby"),
        ("composer.json", "", Some(Php), "php"),
        ("CMakeLists.txt", "", Some(Cpp), "cpp"),
        (
            "package.json",
            r#"{"devDependencies":{"typescript":"*"}}"#,
            Some(TypeScript),
            "typescript",
        ),
        ("package.json", "{invalid", Some(JavaScript), "javascript"),
        ("App.slnx", "", Some(CSharp), "csharp"),
        ("thing.gemspec", "", Some(Ruby), "ruby"),
        ("meson.build", "", Some(Cpp), "cpp"),
        ("setup.py", "", Some(Python), "python"),
        ("build.gradle.kts", "", None, "kotlin"),
        ("build.sbt", "", None, "scala"),
    ] {
        assert_evidence(name, content, expected, kind);
    }
}

#[test]
fn directory_and_symlink_manifest_entries_are_not_evidence() {
    let temp = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("Cargo.toml")).unwrap();
    fs::create_dir(temp.path().join("Directory.slnx")).unwrap();
    fs::write(outside.path().join("package.json"), "{}").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("package.json"),
        temp.path().join("package.json"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("package.json"),
        temp.path().join("fake.gemspec"),
    )
    .unwrap();
    assert!(detect_project_languages(temp.path()).is_empty());
    assert!(ProjectProfile::discover(temp.path()).types.is_empty());
}

#[test]
fn root_detection_stays_local_while_project_scans_children() {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("nested")).unwrap();
    fs::write(temp.path().join("nested/go.mod"), "module nested").unwrap();
    assert!(detect_project_languages(temp.path()).is_empty());
    assert!(ProjectProfile::discover(temp.path())
        .types
        .iter()
        .any(|entry| entry.kind == "golang" && entry.path == std::path::Path::new("nested")));
}

#[test]
fn nested_cmake_does_not_hide_independent_meson_evidence() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("CMakeLists.txt"), "").unwrap();
    fs::create_dir(temp.path().join("nested")).unwrap();
    fs::write(temp.path().join("nested/CMakeLists.txt"), "").unwrap();
    fs::write(temp.path().join("nested/meson.build"), "").unwrap();
    let profile = ProjectProfile::discover(temp.path());
    assert!(profile
        .types
        .iter()
        .any(|entry| entry.kind == "cpp" && entry.path == std::path::Path::new("nested")));
    assert_eq!(
        profile
            .package_details()
            .iter()
            .find(|entry| entry.path == std::path::Path::new("nested"))
            .unwrap()
            .runner,
        None
    );
}

#[test]
fn newly_recognized_manifests_do_not_invent_test_runners() {
    for name in [
        "App.slnx",
        "thing.gemspec",
        "meson.build",
        "conanfile.txt",
        "vcpkg.json",
    ] {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(name), "").unwrap();
        let details = ProjectProfile::discover(temp.path()).package_details();
        assert_eq!(details.len(), 1, "{name}");
        assert_eq!(details[0].runner, None, "{name}");
    }
}
