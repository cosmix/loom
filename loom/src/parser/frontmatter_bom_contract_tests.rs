//! Contract: the shared frontmatter splitter skips a leading byte-order mark.

use crate::parser::frontmatter::{extract_frontmatter_raw, split_frontmatter_raw};

#[test]
fn a_byte_order_mark_does_not_hide_the_frontmatter() {
    let content = "\u{FEFF}---\nname: x\n---\nbody";

    let (yaml, body) = split_frontmatter_raw(content).expect("frontmatter after a BOM splits");
    assert_eq!((yaml, body), ("name: x\n", "body"));

    let extracted = extract_frontmatter_raw(content).expect("frontmatter after a BOM extracts");
    assert_eq!(extracted, "name: x\n");
}
