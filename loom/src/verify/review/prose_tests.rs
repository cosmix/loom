use super::*;

fn digest(path: &str, content: &str) -> Option<String> {
    behaviour_digest(path, Some(content.as_bytes()))
}

#[test]
fn only_unprotected_markdown_is_prose() {
    assert_eq!(digest("doc/x.md", "text"), None);
    assert_eq!(behaviour_digest("doc/x.md", None), None);
    for path in [
        "skills/y/SKILL.md",
        "agents/z.md",
        "CLAUDE.md.template",
        "AGENTS.md",
        "loom/tests/fixtures/a.md",
        "commands/a.md",
        "loom-hooks/a.md",
        ".claude/a.md",
        ".codex/a.md",
        "loom/src/planning/guidance/text/summary_codex.md",
    ] {
        assert_ne!(digest(path, "old"), digest(path, "new"), "{path}");
    }
}

#[test]
fn comments_and_blank_lines_are_hashed() {
    for path in [
        "a.rs", "a.ts", "a.tsx", "a.js", "a.jsx", "a.mjs", "a.cjs", "a.go",
    ] {
        assert_ne!(
            digest(path, "  run();\n"),
            digest(path, "// note\n\n  run();\n"),
            "{path}"
        );
    }
    assert_ne!(
        digest("a.py", "  run()\n"),
        digest("a.py", " # note\n\n  run()\n")
    );
    assert_ne!(digest("a.rs", "run();\n"), digest("a.rs", "  run();\n"));
    assert_ne!(digest("a.rs", "run();\n"), digest("a.rs", "run();"));
    assert_ne!(
        digest("a.rs", "run();\n"),
        digest("a.rs", "run(); // note\n")
    );
    assert_ne!(
        digest("a.rs", "run();\n"),
        digest("a.rs", "/* note */\nrun();\n")
    );
}

#[test]
fn pragmas_and_attributes_are_behaviour() {
    for pragma in [
        "@ts-ignore",
        "eslint-disable",
        "oxlint",
        "biome-ignore",
        "prettier-ignore",
        "noqa",
        "type: ignore",
        "pylint",
        "mypy",
        "rustfmt",
        "clippy",
        "go:generate",
        "nolint",
        "istanbul",
        "c8 ignore",
        "v8 ignore",
        "@vitest",
        "@jest",
        "@generated",
        "<reference",
        "+build",
        "coding",
        "#!",
        "@jsx",
    ] {
        for (path, prefix) in [("a.ts", "//"), ("a.py", "#")] {
            let content = format!("{prefix} {}\nrun();\n", pragma.to_uppercase());
            assert_ne!(digest(path, &content), digest(path, "run();\n"), "{pragma}");
        }
    }
    assert_ne!(
        digest("a.rs", "struct A;\n"),
        digest("a.rs", "#[derive(Clone)]\nstruct A;\n")
    );
}

#[test]
fn other_formats_and_invalid_utf8_keep_the_plain_hash() {
    for path in [
        "a.sh",
        "a.yaml",
        "a.toml",
        "a.json",
        "a.css",
        "a.template",
        "a.txt",
    ] {
        assert_ne!(
            digest(path, "run\n"),
            digest(path, "# comment\nrun\n"),
            "{path}"
        );
        assert_eq!(
            digest(path, "run\n"),
            Some(format!("v2:{}", hash(b"run\n")))
        );
    }
    assert_eq!(
        behaviour_digest("a.rs", Some(b"//\xff\n")),
        Some(format!("v2:{}", hash(b"//\xff\n")))
    );
    assert_eq!(
        behaviour_digest("a.rs", None).as_deref(),
        Some("v2:deleted")
    );
}
