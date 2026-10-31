use super::*;
use crate::context::extract::dialect::DIALECTS;
use std::fs;
use tempfile::TempDir;

/// (variant, Display, skill_name) for every variant.
const ALL_NAMES: [(DetectedLanguage, &str, &str); 11] = [
    (DetectedLanguage::Rust, "Rust", "rust"),
    (DetectedLanguage::TypeScript, "TypeScript", "typescript"),
    (DetectedLanguage::JavaScript, "JavaScript", "javascript"),
    (DetectedLanguage::Python, "Python", "python"),
    (DetectedLanguage::Go, "Go", "golang"),
    (DetectedLanguage::Java, "Java", "java"),
    (DetectedLanguage::CSharp, "C#", "csharp"),
    (DetectedLanguage::Ruby, "Ruby", "ruby"),
    (DetectedLanguage::Php, "PHP", "php"),
    (DetectedLanguage::C, "C", "c"),
    (DetectedLanguage::Cpp, "C++", "cpp"),
];

#[test]
fn test_detect_rust() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

    let languages = detect_project_languages(temp.path());

    assert_eq!(languages.len(), 1);
    assert!(languages.contains(&DetectedLanguage::Rust));
}

#[test]
fn test_detect_typescript() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("tsconfig.json"), "{}").unwrap();

    let languages = detect_project_languages(temp.path());

    assert_eq!(languages.len(), 1);
    assert!(languages.contains(&DetectedLanguage::TypeScript));
}

#[test]
fn test_detect_javascript_via_package_json() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("package.json"), "{}").unwrap();

    assert_eq!(
        detect_project_languages(temp.path()),
        vec![DetectedLanguage::JavaScript]
    );
}

#[test]
fn test_package_json_with_tsconfig_is_only_typescript() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("package.json"), "{}").unwrap();
    fs::write(temp.path().join("tsconfig.json"), "{}").unwrap();

    assert_eq!(
        detect_project_languages(temp.path()),
        vec![DetectedLanguage::TypeScript]
    );
}

#[test]
fn test_detect_python_via_pyproject() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("pyproject.toml"), "[tool.poetry]").unwrap();

    let languages = detect_project_languages(temp.path());

    assert_eq!(languages.len(), 1);
    assert!(languages.contains(&DetectedLanguage::Python));
}

#[test]
fn test_detect_python_via_requirements() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("requirements.txt"), "requests==2.28.0").unwrap();

    let languages = detect_project_languages(temp.path());

    assert_eq!(languages.len(), 1);
    assert!(languages.contains(&DetectedLanguage::Python));
}

#[test]
fn test_detect_go() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("go.mod"), "module example.com/myapp").unwrap();

    let languages = detect_project_languages(temp.path());

    assert_eq!(languages.len(), 1);
    assert!(languages.contains(&DetectedLanguage::Go));
}

#[test]
fn test_detect_multiple() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
    fs::write(temp.path().join("package.json"), "{}").unwrap();

    let languages = detect_project_languages(temp.path());

    assert_eq!(languages.len(), 2);
    assert!(languages.contains(&DetectedLanguage::Rust));
    assert!(languages.contains(&DetectedLanguage::JavaScript));
}

#[test]
fn test_detect_none() {
    let temp = TempDir::new().unwrap();
    // Empty directory

    let languages = detect_project_languages(temp.path());

    assert!(languages.is_empty());
}

#[test]
fn test_display_trait() {
    for (lang, expected) in ALL_NAMES.iter().map(|r| (&r.0, r.1)) {
        assert_eq!(format!("{lang}"), expected);
    }
}

#[test]
fn test_skill_name() {
    for (lang, _, skill) in &ALL_NAMES {
        assert_eq!(lang.skill_name(), *skill);
    }
}

#[test]
fn test_registry_domains() {
    use DetectedLanguage::*;
    assert_eq!(Rust.registry_domains(), ["crates.io", "static.crates.io"]);
    assert_eq!(TypeScript.registry_domains(), ["registry.npmjs.org"]);
    assert_eq!(JavaScript.registry_domains(), ["registry.npmjs.org"]);
    assert_eq!(Python.registry_domains(), ["pypi.org"]);
    assert_eq!(Go.registry_domains(), ["proxy.golang.org"]);
    assert_eq!(
        Java.registry_domains(),
        [
            "repo.maven.apache.org",
            "plugins.gradle.org",
            "services.gradle.org"
        ]
    );
    assert_eq!(CSharp.registry_domains(), ["api.nuget.org"]);
    assert_eq!(Ruby.registry_domains(), ["rubygems.org"]);
    assert_eq!(Php.registry_domains(), ["repo.packagist.org"]);
    assert!(C.registry_domains().is_empty());
    assert!(Cpp.registry_domains().is_empty());
}

#[test]
fn test_every_variant_has_a_skill_directory() {
    for row in &ALL_NAMES {
        let skill = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../skills")
            .join(format!("loom-{}", row.0.skill_name()))
            .join("SKILL.md");
        assert!(skill.is_file(), "missing {}", skill.display());
    }
}

fn detect_in(entries: &[&str]) -> Vec<DetectedLanguage> {
    let temp = TempDir::new().unwrap();
    for entry in entries {
        fs::write(temp.path().join(entry), "").unwrap();
    }
    detect_project_languages(temp.path())
}

#[test]
fn test_detect_manifest_rules() {
    use DetectedLanguage::*;
    let cases: [(&[&str], DetectedLanguage); 16] = [
        (&["pom.xml"], Java),
        (&["build.gradle"], Java),
        (&["App.csproj"], CSharp),
        (&["App.sln"], CSharp),
        (&["App.slnx"], CSharp),
        (&["Gemfile"], Ruby),
        (&["thing.gemspec"], Ruby),
        (&["composer.json"], Php),
        (&["CMakeLists.txt"], Cpp),
        (&["meson.build"], Cpp),
        (&["conanfile.txt"], Cpp),
        (&["conanfile.py"], Cpp),
        (&["vcpkg.json"], Cpp),
        (&["tsconfig.json"], TypeScript),
        (&["package.json"], JavaScript),
        (&["go.mod"], Go),
    ];
    for (entries, expected) in cases {
        assert_eq!(detect_in(entries), vec![expected], "entries {entries:?}");
    }
    // Kotlin has no DetectedLanguage variant; project detection reports it separately.
    assert!(detect_in(&["build.gradle.kts"]).is_empty());
}

#[test]
fn test_detect_c_has_no_manifest() {
    assert!(!detect_in(&["Makefile", "main.c"]).contains(&DetectedLanguage::C));
}

#[test]
fn test_detect_unreadable_root_is_empty() {
    assert!(detect_project_languages(Path::new("/nonexistent/loom/root")).is_empty());
}

#[test]
fn test_extension_mapping_new_languages() {
    use DetectedLanguage::*;
    let cases = [
        ("js", JavaScript),
        ("jsx", JavaScript),
        ("mjs", JavaScript),
        ("cjs", JavaScript),
        ("java", Java),
        ("cs", CSharp),
        ("rb", Ruby),
        ("rake", Ruby),
        ("gemspec", Ruby),
        ("php", Php),
        ("c", C),
        ("h", Cpp),
        ("cc", Cpp),
        ("cpp", Cpp),
        ("cxx", Cpp),
        ("hh", Cpp),
        ("hpp", Cpp),
        ("hxx", Cpp),
        ("tsx", TypeScript),
    ];
    for (ext, expected) in cases {
        assert_eq!(
            detect_languages_from_files(&[format!("src/a.{ext}")]),
            vec![expected],
            "extension .{ext}"
        );
    }
}

#[test]
fn test_every_dialect_extension_maps_to_a_language() {
    for dialect in DIALECTS {
        for ext in dialect.extensions {
            assert!(
                language_for_path(&format!("a.{ext}")).is_some(),
                "dialect {} extension .{ext} has no DetectedLanguage",
                dialect.id
            );
        }
    }
}

#[test]
fn test_detect_from_files_globs() {
    let files = vec![
        "loom/src/**/*.rs".to_string(),
        "frontend/**/*.tsx".to_string(),
    ];
    let langs = detect_languages_from_files(&files);
    assert_eq!(
        langs,
        vec![DetectedLanguage::Rust, DetectedLanguage::TypeScript]
    );
}

#[test]
fn test_detect_from_files_extensions() {
    assert_eq!(
        detect_languages_from_files(&["a.rs".to_string()]),
        vec![DetectedLanguage::Rust]
    );
    // All TypeScript extension variants resolve.
    for ext in ["ts", "tsx", "mts", "cts"] {
        assert_eq!(
            detect_languages_from_files(&[format!("a.{ext}")]),
            vec![DetectedLanguage::TypeScript],
            "extension .{ext} should map to TypeScript"
        );
    }
    assert_eq!(
        detect_languages_from_files(&["pkg/main.go".to_string()]),
        vec![DetectedLanguage::Go]
    );
    assert_eq!(
        detect_languages_from_files(&["app/models.py".to_string()]),
        vec![DetectedLanguage::Python]
    );
}

#[test]
fn test_detect_from_files_dedup_preserves_order() {
    let files = vec![
        "src/a.rs".to_string(),
        "src/b.rs".to_string(),
        "scripts/x.py".to_string(),
    ];
    let langs = detect_languages_from_files(&files);
    assert_eq!(
        langs,
        vec![DetectedLanguage::Rust, DetectedLanguage::Python]
    );
}

#[test]
fn test_detect_from_files_ignores_unknown_and_extensionless() {
    let files = vec![
        "Makefile".to_string(),
        ".gitignore".to_string(),
        "docs/readme.md".to_string(),
        "my.dir/Makefile".to_string(),
        "src/".to_string(),
    ];
    assert!(detect_languages_from_files(&files).is_empty());
}
