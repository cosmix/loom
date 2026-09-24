//! Test-file patterns and line-level test syntax for supported languages.

use std::path::Path;
use std::sync::LazyLock;

use glob::{MatchOptions, Pattern};

/// A language's conventional test files, declarations, and assertions.
pub struct LanguageProfile {
    pub name: &'static str,
    pub test_file_globs: &'static [&'static str],
    pub test_declaration: &'static str,
    pub assertion: &'static str,
}

static PROFILES: [LanguageProfile; 14] = [
    LanguageProfile {
        name: "rust",
        test_file_globs: &[
            "**/tests/**/*.rs",
            "**/tests.rs",
            "**/*_tests.rs",
            "**/tests_*.rs",
            "**/*_test.rs",
        ],
        test_declaration: r"#\[(?:tokio::)?test\]",
        assertion: r"\bassert(?:_eq|_ne)?!\s*\(",
    },
    LanguageProfile {
        name: "go",
        test_file_globs: &["**/*_test.go"],
        test_declaration: r"^func Test\w*\s*\(",
        assertion: r"\bt\.(?:Error|Fatal|Fail)(?:f|Now)?\s*\(",
    },
    LanguageProfile {
        name: "python",
        test_file_globs: &["**/test_*.py", "**/*_test.py", "**/tests/**/*.py"],
        test_declaration: r"^\s*(?:async\s+)?def test_\w+\s*\(",
        assertion: r"\bassert\b|self\.assert\w*\s*\(",
    },
    LanguageProfile {
        name: "javascript",
        test_file_globs: &[
            "**/*.test.js",
            "**/*.test.jsx",
            "**/*.test.ts",
            "**/*.test.tsx",
            "**/*.test.mjs",
            "**/*.test.cjs",
            "**/*.spec.js",
            "**/*.spec.jsx",
            "**/*.spec.ts",
            "**/*.spec.tsx",
            "**/*.spec.mjs",
            "**/*.spec.cjs",
            "**/__tests__/**",
        ],
        test_declaration: r"\b(?:it|test)\s*\(",
        assertion: r"\bexpect\s*\(|\bassert\.",
    },
    LanguageProfile {
        name: "java",
        test_file_globs: &[
            "src/test/**/*Test.java",
            "**/src/test/**/*Test.java",
            "**/*Tests.java",
        ],
        test_declaration: r"@Test\b",
        assertion: r"\bassert[A-Z]\w*\s*\(",
    },
    LanguageProfile {
        name: "kotlin",
        test_file_globs: &["**/*Test.kt", "**/*Tests.kt", "**/*Spec.kt"],
        test_declaration: r"@Test\b",
        assertion: r"\bassert\w*\s*\(|\bshould\w+\b|\bexpect\s*\(",
    },
    LanguageProfile {
        name: "scala",
        test_file_globs: &["**/*Spec.scala", "**/*Test.scala", "**/*Suite.scala"],
        test_declaration: r"\b(?:test|it)\s*\(|\bshould\b",
        assertion: r"\b(?:assert|expect)\s*\(|\bshould(?:Be|Equal|EqualTo)\b",
    },
    LanguageProfile {
        name: "csharp",
        test_file_globs: &["**/*Tests.cs", "**/*Test.cs"],
        test_declaration: r"\[(?:Fact|Test|TestMethod|Theory)\]",
        assertion: r"\bAssert\.\w+\s*\(",
    },
    LanguageProfile {
        name: "ruby",
        test_file_globs: &["**/*_test.rb", "**/*_spec.rb", "**/test_*.rb"],
        test_declaration: r#"^\s*def test_\w+|^\s*(?:it|test)\s*(?:\(|['"])"#,
        assertion: r"\bassert(?:_\w+)?\b|\bexpect\s*\(",
    },
    LanguageProfile {
        name: "php",
        test_file_globs: &["**/*Test.php"],
        test_declaration: r"public\s+function\s+test\w*\s*\(|#\[\s*Test\s*\]|\b(?:test|it)\s*\(",
        assertion: r"\b(?:assert\w*|expect)\s*\(",
    },
    LanguageProfile {
        name: "swift",
        test_file_globs: &["**/Tests/**/*.swift", "**/*Tests.swift"],
        test_declaration: r"\bfunc\s+test\w*\s*\(",
        assertion: r"\bXCTAssert\w*\s*\(",
    },
    LanguageProfile {
        name: "elixir",
        test_file_globs: &["**/*_test.exs"],
        test_declaration: r#"^\s*test\s+"[^"]+"(?:\s*,.*?)?\s+do\b"#,
        assertion: r"\b(?:assert|refute)\b",
    },
    LanguageProfile {
        name: "cpp",
        test_file_globs: &[
            "**/*_test.cpp",
            "**/*_test.cc",
            "**/test_*.cpp",
            "**/tests/**/*.cpp",
        ],
        test_declaration: r"\b(?:TEST|TEST_F|TEST_P|TEST_CASE)\s*\(",
        assertion: r"\b(?:EXPECT_\w+|ASSERT_\w+|REQUIRE|CHECK|assert)\s*\(",
    },
    LanguageProfile {
        name: "dart",
        test_file_globs: &["**/*_test.dart"],
        test_declaration: r"\b(?:test|testWidgets)\s*\(",
        assertion: r"\b(?:expect|assert)\s*\(",
    },
];

static COMPILED_GLOBS: LazyLock<Vec<Vec<Pattern>>> = LazyLock::new(|| {
    all()
        .iter()
        .map(|profile| {
            profile
                .test_file_globs
                .iter()
                .map(|glob| Pattern::new(glob).expect("language profile glob must compile"))
                .collect()
        })
        .collect()
});

/// Every supported language profile, in stable lookup order.
pub fn all() -> &'static [LanguageProfile] {
    &PROFILES
}

/// Find a profile by its exact name.
pub fn by_name(name: &str) -> Option<&'static LanguageProfile> {
    all().iter().find(|profile| profile.name == name)
}

/// Find the profile of a relative test-file path.
pub fn for_path(path: &str) -> Option<&'static LanguageProfile> {
    let javascript_extension = matches!(
        Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs")
    );
    all()
        .iter()
        .zip(COMPILED_GLOBS.iter())
        .find(|(profile, patterns)| {
            (profile.name != "javascript" || javascript_extension)
                && patterns.iter().any(|pattern| {
                    pattern.matches_with(
                        path,
                        MatchOptions {
                            case_sensitive: true,
                            require_literal_separator: true,
                            require_literal_leading_dot: false,
                        },
                    )
                })
        })
        .map(|(profile, _)| profile)
}

/// Return the skill name associated with a language profile.
pub fn skill_for(profile: &str) -> String {
    let name = match profile {
        "go" => "golang",
        "javascript" => "typescript",
        name => name,
    };
    format!("loom-{name}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    const SAMPLES: &[(&str, &str, &str, &str)] = &[
        (
            "rust",
            "#[tokio::test]",
            "assert_eq!(actual, 2);",
            "let actual = 2;",
        ),
        (
            "go",
            "func TestSum(t *testing.T) {",
            "t.Errorf(\"bad sum\")",
            "sum := 2",
        ),
        (
            "python",
            "def test_sum():",
            "self.assertEqual(actual, 2)",
            "actual = 2",
        ),
        (
            "javascript",
            "test('sum', () => {",
            "expect(actual).toBe(2)",
            "const actual = 2",
        ),
        (
            "java",
            "@Test",
            "assertEquals(2, actual);",
            "int actual = 2;",
        ),
        (
            "kotlin",
            "@Test",
            "assertEquals(2, actual)",
            "val actual = 2",
        ),
        (
            "scala",
            "test(\"sum\") {",
            "assert(actual == 2)",
            "val actual = 2",
        ),
        (
            "csharp",
            "[Fact]",
            "Assert.Equal(2, actual);",
            "var actual = 2;",
        ),
        (
            "ruby",
            "def test_sum",
            "assert_equal 2, actual",
            "actual = 2",
        ),
        (
            "php",
            "public function testSum(): void {",
            "$this->assertSame(2, $actual);",
            "$actual = 2;",
        ),
        (
            "swift",
            "func testSum() {",
            "XCTAssertEqual(actual, 2)",
            "let actual = 2",
        ),
        (
            "elixir",
            "test \"sum\" do",
            "assert actual == 2",
            "actual = 2",
        ),
        (
            "cpp",
            "TEST(Sum, Works) {",
            "EXPECT_EQ(actual, 2);",
            "int actual = 2;",
        ),
        (
            "dart",
            "testWidgets('sum', (tester) async {",
            "expect(actual, 2);",
            "final actual = 2;",
        ),
    ];

    #[test]
    fn every_language_profile_counts_samples() {
        assert_eq!(SAMPLES.len(), all().len());
        for (profile, &(name, declaration_line, assertion_line, negative_line)) in
            all().iter().zip(SAMPLES)
        {
            assert_eq!(profile.name, name);
            let declaration = Regex::new(profile.test_declaration).unwrap();
            let assertion = Regex::new(profile.assertion).unwrap();
            assert!(declaration.is_match(declaration_line), "{name} declaration");
            assert!(assertion.is_match(assertion_line), "{name} assertion");
            assert!(
                !declaration.is_match(negative_line),
                "{name} declaration negative"
            );
            assert!(
                !assertion.is_match(negative_line),
                "{name} assertion negative"
            );
        }
    }

    #[test]
    fn paths_map_to_profiles() {
        let samples = [
            ("loom/src/math_test.rs", Some("rust")),
            ("src/foo_test.go", Some("go")),
            ("tests/test_math.py", Some("python")),
            ("web/src/app.test.tsx", Some("javascript")),
            ("src/test/java/MathTest.java", Some("java")),
            ("src/test/kotlin/MathTest.kt", Some("kotlin")),
            ("src/test/scala/MathSpec.scala", Some("scala")),
            ("tests/MathTests.cs", Some("csharp")),
            ("spec/math_spec.rb", Some("ruby")),
            ("tests/MathTest.php", Some("php")),
            ("Tests/MathTests.swift", Some("swift")),
            ("test/math_test.exs", Some("elixir")),
            ("tests/math_test.cpp", Some("cpp")),
            ("test/math_test.dart", Some("dart")),
            ("web/__tests__/math.ts", Some("javascript")),
            ("web/__tests__/data.json", None),
            ("loom/src/lib.rs", None),
        ];
        for (path, expected) in samples {
            assert_eq!(
                for_path(path).map(|profile| profile.name),
                expected,
                "{path}"
            );
        }
    }

    #[test]
    fn names_find_exact_profiles() {
        assert_eq!(by_name("go").map(|profile| profile.name), Some("go"));
        assert!(by_name("Go").is_none());
        assert!(by_name("unknown").is_none());
    }

    #[test]
    fn skill_names_follow_profile_mapping() {
        assert_eq!(skill_for("go"), "loom-golang");
        assert_eq!(skill_for("javascript"), "loom-typescript");
        assert_eq!(skill_for("rust"), "loom-rust");
    }
}
