use super::{assertion_statements, Statement};
use regex::Regex;

fn statements(content: &str, language: &str) -> Vec<Statement> {
    let profile = crate::testrun::languages::all()
        .iter()
        .find(|p| p.name == language)
        .unwrap();
    assertion_statements(content, &Regex::new(profile.assertion).unwrap(), language)
}

#[test]
fn rust_single_line_and_byte_offsets() {
    let input = "é    assert_eq!(a, b);";
    let result = statements(input, "rust");
    assert_eq!(result[0].text, "assert_eq!(a, b);");
    assert_eq!(&input[result[0].start..result[0].end], "assert_eq!(a, b);");
    assert_eq!(result[0].start, "é    ".len());
}

#[test]
fn rust_reflow_and_trailing_comma_share_key() {
    let single = statements("assert_eq!(a, b);", "rust");
    let multi = statements("assert_eq!(\n a,\n b,\n);", "rust");
    assert_eq!(single[0].key, multi[0].key);
    assert_ne!(single[0].text, multi[0].text);
}

#[test]
fn semicolon_in_string_and_two_assertions() {
    let result = statements("assert_eq!(a, \";\"); assert!(b);", "rust");
    assert_eq!(
        result.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
        ["assert_eq!(a, \";\");", "assert!(b);"]
    );
}

#[test]
fn closing_bracket_below_zero_ends_statement() {
    let input = "let f = || assert!(x) }";
    let result = statements(input, "rust");
    assert_eq!(result[0].text, "assert!(x)");
    assert_eq!(&input[result[0].end..], "}");
}

#[test]
fn rust_lifetimes_do_not_open_strings() {
    let result = statements("fn f<'a>(x: &'a str) { assert!(x.is_empty()); }", "rust");
    assert_eq!(result[0].text, "assert!(x.is_empty());");
}

#[test]
fn comments_are_removed_from_code_and_key() {
    let first = statements("assert!(a); // note's", "rust");
    assert_eq!(first[0].text, "assert!(a);");
    let a = statements("assert!(a, // why\n b);", "rust");
    let b = statements("assert!(a, // another\n b);", "rust");
    assert_eq!(a[0].text, "assert!(a, b);");
    assert_eq!(a[0].key, b[0].key);
}

#[test]
fn javascript_newline_ends_at_zero_depth() {
    let source = "expect(a)\n  .toBe(1);";
    let result = statements(source, "javascript");
    assert_eq!(result[0].text, "expect(a)");
    assert_eq!(&source[result[0].end..], "\n  .toBe(1);");
    let nested = statements("expect(\n a,\n).toBe(1);", "javascript");
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].text, "expect( a, ).toBe(1);");
}

#[test]
fn comment_apostrophe_does_not_swallow_next_line() {
    let result = statements("expect(a); // don't\nexpect(b);", "javascript");
    assert_eq!(result.len(), 2);
    assert_eq!(result[1].text, "expect(b);");
}

#[test]
fn python_brackets_and_separate_statements() {
    let result = statements("assert x == (1,\n 2)\nassert y == 2", "python");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].text, "assert x == (1, 2)");
    assert_eq!(result[1].text, "assert y == 2");
}

#[test]
fn python_backslash_continuation_and_tuple_comma() {
    let result = statements("assert x == \\\n (1,)", "python");
    assert_eq!(result[0].text, "assert x == (1,)");
    assert_eq!(result[0].key, "assertx==(1,)");
    assert_ne!(
        result[0].key,
        statements("assert x == (1)", "python")[0].key
    );
}

#[test]
fn python_triple_quoted_strings_preserve_newlines() {
    for quote in ["\"\"\"", "'''"] {
        let input = format!("assert x == {quote}a\nb; # c{quote}\nassert y");
        let result = statements(&input, "python");
        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0].text,
            format!("assert x == {quote}a\nb; # c{quote}")
        );
    }
}

#[test]
fn rust_string_preserves_newline() {
    let result = statements("assert_eq!(s, \"a\nb\");", "rust");
    assert_eq!(result[0].text, "assert_eq!(s, \"a\nb\");");
}

#[test]
fn single_quote_ends_at_line_break() {
    let result = statements("assert 'unfinished\nassert y", "python");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].text, "assert 'unfinished");
}

#[test]
fn infix_statements_include_the_subject() {
    let x = statements("    x shouldBe 1", "kotlin");
    let y = statements("    y shouldBe 1", "kotlin");
    assert_eq!(x[0].text, "x shouldBe 1");
    assert_ne!(x[0].key, y[0].key);
    assert_eq!(
        statements("val r = f(); r shouldBe 1", "kotlin")[0].text,
        "r shouldBe 1"
    );
    assert_eq!(
        statements("x shouldEqual 2", "scala")[0].text,
        "x shouldEqual 2"
    );
}

#[test]
fn infix_starts_after_previous_statement_and_ignores_quoted_semicolons() {
    let result = statements("x shouldBe 1; y shouldBe \"a;b\"", "kotlin");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].text, "x shouldBe 1;");
    assert_eq!(result[1].text, "y shouldBe \"a;b\"");
    assert_eq!(
        statements("val x = \";\"; y shouldBe 1", "kotlin")[0].text,
        "y shouldBe 1"
    );
}

#[test]
fn unterminated_statement_ends_at_end_of_input() {
    let input = "assert!(hello";
    let result = statements(input, "rust");
    assert_eq!(result[0].end, input.len());
    assert_eq!(result[0].text, input);
}

#[test]
fn empty_and_nonmatching_content() {
    assert!(statements("", "rust").is_empty());
    assert!(statements("let x = 1;", "rust").is_empty());
}

#[test]
fn go_starts_at_match_not_line_start() {
    let result = statements("if bad { t.Errorf(\"no\") }", "go");
    assert_eq!(result[0].text, "t.Errorf(\"no\")");
}

#[test]
fn lexical_matches_inside_comments_and_strings_are_included() {
    assert_eq!(statements("// assert!(x);", "rust")[0].text, "assert!(x);");
    assert_eq!(statements("\"assert!(x);\"", "rust")[0].text, "assert!(x);");
}

#[test]
fn infix_after_a_closer_of_an_earlier_line_starts_past_the_closer() {
    let source = "val r = listOf(1, 2).map {\n    it * 2\n} shouldBe listOf(2, 4)\n";
    let result = statements(source, "kotlin");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text, "shouldBe listOf(2, 4)");
    let changed = statements(&source.replace("(2, 4)", "(2, 5)"), "kotlin");
    assert!(!changed[0].key.is_empty());
    assert_ne!(result[0].key, changed[0].key);
    // A bracket opened and closed inside the prefix leaves the start alone.
    assert_eq!(
        statements("f(a) shouldBe 1", "scala")[0].text,
        "f(a) shouldBe 1"
    );
}

#[test]
fn rust_char_literal_holding_a_quote_does_not_open_a_string() {
    let source = "assert_eq!(c, '\"');\nassert!(ok);\n";
    let result = statements(source, "rust");
    assert_eq!(
        result.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
        ["assert_eq!(c, '\"');", "assert!(ok);"]
    );
    let escaped = statements("assert_eq!(c, '\\'');\nassert!(ok);", "rust");
    assert_eq!(escaped.len(), 2);
    assert_eq!(escaped[0].text, "assert_eq!(c, '\\'');");
    let unicode = statements("assert_eq!(c, '\\u{22}'); assert!(ok);", "rust");
    assert_eq!(unicode.len(), 2);
}

#[test]
fn rust_raw_strings_end_at_their_matching_hashes() {
    let backslash = statements("assert_eq!(p, r\"C:\\dir\\\");\nassert!(ok);", "rust");
    assert_eq!(backslash.len(), 2);
    assert_eq!(backslash[0].text, "assert_eq!(p, r\"C:\\dir\\\");");
    let hashed = statements("assert_eq!(s, r#\"a\"b;\"#);\nassert!(ok);", "rust");
    assert_eq!(hashed.len(), 2);
    assert_eq!(hashed[0].text, "assert_eq!(s, r#\"a\"b;\"#);");
    assert_eq!(
        statements("assert_eq!(r, 1);", "rust")[0].text,
        "assert_eq!(r, 1);"
    );
}
