//! Contract: the software developer agent carries the `Skill` tool so it loads the skills its
//! brief names, and stays a small file.

use std::fs;
use std::path::Path;

fn frontmatter_value<'a>(frontmatter: &'a str, key: &str) -> Option<&'a str> {
    frontmatter.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == key).then(|| value.trim())
    })
}

#[test]
fn software_developer_carries_the_skill_tool() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../agents/loom-software-developer.md");
    let body = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));

    let rest = body
        .strip_prefix("---\n")
        .expect("the agent file opens with frontmatter");
    let (frontmatter, _) = rest
        .split_once("\n---")
        .expect("the frontmatter has a closing marker");

    assert_eq!(frontmatter_value(frontmatter, "model"), Some("haiku"));
    assert_eq!(frontmatter_value(frontmatter, "effort"), Some("high"));
    let tools: Vec<&str> = frontmatter_value(frontmatter, "tools")
        .expect("a tools line")
        .split(',')
        .map(str::trim)
        .collect();
    assert_eq!(
        tools,
        ["Read", "Edit", "Write", "Glob", "Grep", "Bash", "Skill"]
    );
    assert!(
        body.len() < 2_000,
        "the agent file is {} bytes, the limit is under 2,000",
        body.len()
    );
}
