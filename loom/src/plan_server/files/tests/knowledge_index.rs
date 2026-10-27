use super::super::knowledge_index::{parse_index, IndexTopic};

#[test]
fn parses_topic_tables_in_order_and_unescapes_blurbs() {
    let index = r#"
## Tier 1 — Summaries
| [architecture.md](architecture.md) | Summary | 12 |
## Tier 2 — Topics
### architecture
| Topic | Blurb | Lines |
| --- | --- | --- |
| [first](architecture/first.md) | One \| two | 12 |
| [second](architecture/second.md) | Second topic | 3 |
### patterns
| [third](patterns/third.md) | Third | 4 |
## Other
| [ignored](patterns/ignored.md) | No | 5 |
"#;
    assert_eq!(
        parse_index(index),
        vec![
            topic("architecture", "first", "One | two"),
            topic("architecture", "second", "Second topic"),
            topic("patterns", "third", "Third"),
        ]
    );
}

#[test]
fn skips_invalid_rows_and_category_mismatches() {
    let index = r#"
## Tier 2 — Topics
| [no-heading](x/no-heading.md) | No | 1 |
### x
| [wrong](other/wrong.md) | No | 1 |
| [bad](x/bad.md) | No | many |
| [missing](x/missing.md) | No |
| [valid](x/valid.md) | Yes | 10 |
"#;
    assert_eq!(parse_index(index), vec![topic("x", "valid", "Yes")]);
    assert!(parse_index("").is_empty());
}

fn topic(category: &str, slug: &str, blurb: &str) -> IndexTopic {
    IndexTopic {
        category: category.to_owned(),
        slug: slug.to_owned(),
        path: format!("{category}/{slug}.md"),
        blurb: blurb.to_owned(),
    }
}
