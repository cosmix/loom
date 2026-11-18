//! `frontmatter_head`: the boundary rule is `split_frontmatter_raw`'s.

use super::frontmatter_head;

fn head_of(document: &str) -> anyhow::Result<Option<String>> {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("handoffs")).unwrap();
    std::fs::write(temp.path().join("handoffs/h.md"), document).unwrap();
    frontmatter_head(temp.path(), "h.md")
}

/// A closing delimiter with trailing whitespace or a CRLF ends the frontmatter, as everywhere
/// else, so the request is read instead of retried as unfinished.
#[test]
fn a_closer_with_trailing_whitespace_or_crlf_ends_the_frontmatter() {
    for closer in ["--- \n", "---\r\n", "---\n"] {
        let head = head_of(&format!("---\norigin: agent_ceiling\n{closer}# Body\n"));
        assert_eq!(
            head.unwrap().as_deref(),
            Some("origin: agent_ceiling\n"),
            "{closer:?}"
        );
    }
}

#[test]
fn an_unclosed_frontmatter_is_unfinished_and_a_plain_document_has_none() {
    assert!(head_of("---\norigin: agent_ceiling\n").is_err());
    assert_eq!(head_of("# Just a document\n").unwrap(), None);
}
