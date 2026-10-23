//! The top-level `mod` items of one Rust file, the outer attributes above
//! each, the file's inner attributes, and whether it parsed cleanly
//! (Decision 3). Only top-level items count: a `mod` inside an inline module
//! or a function body is not a module declaration of this file.

/// One attribute, `#[...]` or `#![...]`.
#[cfg_attr(not(feature = "source-graph"), allow(dead_code))]
pub(super) struct Attribute {
    /// The attribute as written.
    pub text: String,
    /// Its path (`cfg`, `path`, `rustfmt::skip`), whitespace removed and the
    /// `r#` of each segment stripped, as rustc resolves it. `None` when it
    /// cannot be read (a comment inside the path, `$crate` or a leading `::`,
    /// a shebang that rustc may read as an attribute): callers refuse it.
    pub path: Option<String>,
}

impl Attribute {
    /// The text with whitespace removed and the `r#` of every raw identifier
    /// stripped, for comparing with a plain spelling such as `#[cfg(test)]`.
    /// A comment stays in, so a spelling holding one never compares equal.
    pub fn normalised(&self) -> String {
        let compact: String = self.text.split_whitespace().collect();
        let mut normalised = String::with_capacity(compact.len());
        let mut rest = compact.as_str();
        while let Some(next) = rest.chars().next() {
            let in_word = normalised.chars().next_back().is_some_and(is_word);
            match rest.strip_prefix("r#") {
                Some(after) if !in_word && after.starts_with(is_word) => rest = after,
                _ => {
                    normalised.push(next);
                    rest = &rest[next.len_utf8()..];
                }
            }
        }
        normalised
    }
}

/// One top-level `mod` item.
#[cfg_attr(not(feature = "source-graph"), allow(dead_code))]
pub(super) struct ModItem {
    /// The module name, without a `r#` prefix.
    pub name: String,
    /// `mod name { ... }`: declared inline rather than out of line.
    pub inline: bool,
    /// The outer attributes above the item, in source order.
    pub attributes: Vec<Attribute>,
}

/// What the chain check reads from one file.
#[cfg_attr(not(feature = "source-graph"), allow(dead_code))]
pub(super) struct ParsedFile {
    pub mods: Vec<ModItem>,
    /// Every top-level inner attribute (`#![...]`).
    pub inner_attributes: Vec<Attribute>,
    /// The parse tree holds an error or a missing node.
    pub has_error: bool,
}

/// Parse `source` with tree-sitter-rust.
#[cfg(feature = "source-graph")]
pub(super) fn parse(source: &str) -> Result<ParsedFile, String> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .map_err(|error| format!("cannot load the Rust grammar: {error}"))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "the Rust parser produced no tree".to_string())?;
    let root = tree.root_node();
    let mut parsed = ParsedFile {
        mods: Vec::new(),
        inner_attributes: Vec::new(),
        has_error: root.has_error(),
    };
    let mut cursor = root.walk();
    for node in root.named_children(&mut cursor) {
        match node.kind() {
            "mod_item" => parsed.mods.push(mod_item(node, source)),
            "inner_attribute_item" => parsed.inner_attributes.push(attribute(node, source)),
            // tree-sitter lexes `#!/**/[..]` and `#! [..]` at the start of a
            // file as a shebang, where rustc reads an inner attribute; no
            // chain file needs a shebang, so it stands as an unreadable one.
            "shebang" => parsed.inner_attributes.push(Attribute {
                text: text(node, source).trim_end().to_string(),
                path: None,
            }),
            _ => {}
        }
    }
    Ok(parsed)
}

/// Without tree-sitter nothing can be parsed: the caller fails closed.
#[cfg(not(feature = "source-graph"))]
pub(super) fn parse(_source: &str) -> Result<ParsedFile, String> {
    Err(super::NO_SOURCE_GRAPH.to_string())
}

#[cfg(feature = "source-graph")]
fn mod_item(node: tree_sitter::Node<'_>, source: &str) -> ModItem {
    let name = node
        .child_by_field_name("name")
        .map(|name| text(name, source))
        .unwrap_or_default();
    ModItem {
        name: name.strip_prefix("r#").unwrap_or(&name).to_string(),
        inline: node.child_by_field_name("body").is_some(),
        attributes: outer_attributes(node, source),
    }
}

/// The `attribute_item`s directly above `node`, walking back over its
/// preceding siblings: every comment (doc comments included) is skipped, and
/// the first node that is neither an attribute nor a comment ends the walk.
#[cfg(feature = "source-graph")]
fn outer_attributes(node: tree_sitter::Node<'_>, source: &str) -> Vec<Attribute> {
    let mut attributes = Vec::new();
    let mut sibling = node.prev_sibling();
    while let Some(previous) = sibling {
        match previous.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => attributes.push(attribute(previous, source)),
            _ => break,
        }
        sibling = previous.prev_sibling();
    }
    attributes.reverse();
    attributes
}

/// An `attribute_item` or `inner_attribute_item`: its text and the path of
/// the `attribute` inside it. A comment is an extra that tree-sitter places
/// as a named child before the `attribute` or before its path, so both are
/// found by kind, never by position.
#[cfg(feature = "source-graph")]
fn attribute(item: tree_sitter::Node<'_>, source: &str) -> Attribute {
    let path = first_named(item, |child| child.kind() == "attribute")
        .and_then(|attribute| first_named(attribute, |child| !is_comment(child)))
        .and_then(|path| normalised_path(&text(path, source)));
    Attribute {
        text: text(item, source),
        path,
    }
}

/// The first named child of `node` that `wanted` accepts.
#[cfg(feature = "source-graph")]
fn first_named<'tree>(
    node: tree_sitter::Node<'tree>,
    wanted: impl Fn(&tree_sitter::Node<'tree>) -> bool,
) -> Option<tree_sitter::Node<'tree>> {
    let mut cursor = node.walk();
    let found = node.named_children(&mut cursor).find(wanted);
    found
}

#[cfg(feature = "source-graph")]
fn is_comment(node: &tree_sitter::Node<'_>) -> bool {
    matches!(node.kind(), "line_comment" | "block_comment")
}

/// `raw` as a path of identifiers joined by `::`, whitespace removed and each
/// segment's `r#` stripped; `None` when a segment is anything else (empty,
/// a comment, `$crate`).
#[cfg(feature = "source-graph")]
fn normalised_path(raw: &str) -> Option<String> {
    let compact: String = raw.split_whitespace().collect();
    let segments: Vec<&str> = compact
        .split("::")
        .map(|segment| segment.strip_prefix("r#").unwrap_or(segment))
        .collect();
    segments
        .iter()
        .all(|segment| !segment.is_empty() && segment.chars().all(is_word))
        .then(|| segments.join("::"))
}

fn is_word(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}

#[cfg(feature = "source-graph")]
fn text(node: tree_sitter::Node<'_>, source: &str) -> String {
    source
        .get(node.byte_range())
        .unwrap_or_default()
        .to_string()
}
