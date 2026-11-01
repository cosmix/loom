//! ECMAScript `this` ownership at a call site, before graph nodes are built.

use tree_sitter::Node;

use super::QueryHarness;

/// What syntax tells us about a call's `this` without adding fields to the
/// persisted edge. Class heritage and computed keys run in the outer scope.
#[derive(Clone, Copy, Default)]
pub(super) struct ReceiverContext {
    pub(super) shadowed: bool,
    pub(super) outer_types: usize,
}

#[derive(Default)]
struct ReceiverWalk {
    context: ReceiverContext,
    lexical_owner: bool,
}

/// Ordinary functions, object methods and class expressions own `this`;
/// arrows inherit it. Heritage and computed keys use the surrounding receiver.
pub(super) fn receiver_context(harness: &dyn QueryHarness, node: Node<'_>) -> ReceiverContext {
    if !matches!(
        harness.node_language().as_str(),
        "javascript" | "typescript" | "tsx"
    ) {
        return ReceiverContext::default();
    }
    let mut walk = ReceiverWalk::default();
    let mut current = node.parent();
    while let Some(node) = current {
        if let Some(shadowed) = walk.boundary(node) {
            walk.context.shadowed = shadowed;
            return walk.context;
        }
        current = node.parent();
    }
    walk.context
}

impl ReceiverWalk {
    /// `Some` ends the search at the nearest receiver owner. Computed keys
    /// and heritage skip their owner because they run in the outer scope.
    fn boundary(&mut self, node: Node<'_>) -> Option<bool> {
        match node.kind() {
            "class_heritage" | "computed_property_name" => {
                self.lexical_owner = true;
                None
            }
            "function_declaration"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration" => Some(true),
            "method_definition" => match node.parent() {
                Some(parent) if parent.kind() == "class_body" => None,
                _ if self.lexical_owner => {
                    self.lexical_owner = false;
                    None
                }
                _ => Some(true),
            },
            "class_declaration" => {
                if !self.lexical_owner {
                    return Some(false);
                }
                self.context.outer_types += 1;
                self.lexical_owner = false;
                None
            }
            "class" => {
                if !self.lexical_owner {
                    return Some(true);
                }
                self.lexical_owner = false;
                None
            }
            "object" => {
                self.lexical_owner = false;
                None
            }
            _ => None,
        }
    }
}
