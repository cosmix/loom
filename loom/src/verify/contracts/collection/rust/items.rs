//! The top-level `mod` items of one Rust file, the outer attributes above
//! each, the file's inner attributes, and whether it parsed cleanly
//! (Decision 3). Only top-level items count: a `mod` inside an inline module
//! or a function body is not a module declaration of this file.

/// One attribute, `#[...]` or `#![...]`.
#[cfg_attr(not(feature = "source-graph"), allow(dead_code))]
pub(super) struct Attribute {
    /// The attribute as written.
    pub text: String,
    /// Its path (`cfg`, `path`, `rustfmt::skip`), whitespace removed.
    pub path: String,
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
/// the `attribute` inside it (the first named child, before any arguments).
#[cfg(feature = "source-graph")]
fn attribute(item: tree_sitter::Node<'_>, source: &str) -> Attribute {
    let path = item
        .named_child(0)
        .and_then(|attribute| attribute.named_child(0))
        .map(|path| text(path, source))
        .unwrap_or_default();
    Attribute {
        text: text(item, source),
        path: path.split_whitespace().collect(),
    }
}

#[cfg(feature = "source-graph")]
fn text(node: tree_sitter::Node<'_>, source: &str) -> String {
    source
        .get(node.byte_range())
        .unwrap_or_default()
        .to_string()
}
