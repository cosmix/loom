//! Text projection for import and receiver captures.

use super::QueryHarness;
use tree_sitter::Node;

/// Preserve only receiver spellings the resolver can treat as paths. An empty
/// receiver marks an unsupported expression while keeping it a member call.
pub(super) fn receiver_text(node: Node<'_>, bytes: &[u8]) -> String {
    let range = node.byte_range();
    let raw = &bytes[range.start..range.end];
    // Reject expression syntax before copying; static paths retain their full
    // spelling, including long imported identifiers and qualifiers.
    if !raw.iter().all(|byte| {
        byte.is_ascii_alphanumeric() || *byte >= 128 || matches!(*byte, b'_' | b'$' | b'.' | b':')
    }) {
        return String::new();
    }
    let text = String::from_utf8_lossy(raw);
    let text = text.trim();
    if text
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.' | ':'))
    {
        text.to_string()
    } else {
        String::new()
    }
}

/// Replace grammar-recognized comments with whitespace before projecting
/// Rust and ECMAScript imports. Strings remain intact, including comment-like
/// text; Rust's grammar recognizes nested block comments as a single node.
pub(super) fn import_text(harness: &dyn QueryHarness, node: Node<'_>, bytes: &[u8]) -> String {
    if !matches!(
        harness.node_language().as_str(),
        "rust" | "javascript" | "typescript" | "tsx"
    ) {
        return String::from_utf8_lossy(&bytes[node.byte_range()]).into_owned();
    }
    let range = node.byte_range();
    let mut text = bytes[range.clone()].to_vec();
    let mut pending = vec![node];
    while let Some(current) = pending.pop() {
        if current.kind().contains("comment") {
            for byte in
                &mut text[current.start_byte() - range.start..current.end_byte() - range.start]
            {
                if *byte != b'\n' && *byte != b'\r' {
                    *byte = b' ';
                }
            }
        } else {
            let mut cursor = current.walk();
            pending.extend(current.children(&mut cursor));
        }
    }
    String::from_utf8_lossy(&text).into_owned()
}
