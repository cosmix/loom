//! Same-file binding of calls and references.
//!
//! Extraction sees one file, so it binds only what that file proves: a
//! `self`/`this` call to exactly one member of the enclosing type
//! (`Receiver`), and a spelling with exactly one definition in lexical scope
//! (`LocalName`). Everything else stays a `Syntax` edge, keeping the ids an
//! ambiguous spelling could mean as candidates for whole-graph resolution.

use std::collections::BTreeMap;

use crate::context::source_graph::{
    syntax_confidence, EdgeProvenance, ImportBinding, SourceEdge, SourceEdgeKind, SourceNodeKind,
    Span, MAX_CANDIDATES,
};

use super::collect::{enclosing, Reference};
use super::QueryHarness;

/// ECMAScript ordinary functions own `this`; arrows inherit it. Inspect the
/// syntax rather than definitions, since anonymous functions have no graph node.
pub(super) fn receiver_shadowed(harness: &dyn QueryHarness, node: tree_sitter::Node<'_>) -> bool {
    if !matches!(
        harness.node_language().as_str(),
        "javascript" | "typescript" | "tsx"
    ) {
        return false;
    }
    let mut current = node.parent();
    while let Some(node) = current {
        match node.kind() {
            "function_declaration"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration" => return true,
            "method_definition" | "class_declaration" | "class" => return false,
            _ => current = node.parent(),
        }
    }
    false
}

/// What the file's dialect says about binding a call locally.
pub(super) struct BindingRules {
    /// Receiver spellings that mean "the enclosing type".
    pub(super) self_receivers: &'static [&'static str],
    /// Whether an unqualified call may name a member of the enclosing type.
    pub(super) bare_calls_reach_members: bool,
    /// Whether a self receiver outside every type names the file's top level,
    /// so its call binds like a plain one (Ruby's `main` object).
    pub(super) top_level_self: bool,
}

/// What binding needs to know about one definition, keyed by its final id.
pub(super) struct DefinitionInfo {
    pub(super) kind: SourceNodeKind,
    pub(super) scope: Vec<String>,
    /// Final id of the innermost enclosing definition; `None` at file level.
    pub(super) parent: Option<String>,
    /// Whether the definition carried a `@definition.qualifier`.
    pub(super) qualified: bool,
}

/// Scope bookkeeping produced while walking definitions, read by the binding
/// rules for every site that follows.
#[derive(Default)]
pub(super) struct DefinitionScopes {
    /// Span of every definition, for locating which one encloses a site.
    pub(super) spans: Vec<(Span, String)>,
    /// Every final id answering to a spelling — its bare name and each
    /// scope-qualified suffix — in source order.
    pub(super) by_spelling: BTreeMap<String, Vec<String>>,
    pub(super) by_id: BTreeMap<String, DefinitionInfo>,
}

/// Everything the binding rules read, for one file.
pub(super) struct Binder<'a> {
    pub(super) scopes: &'a DefinitionScopes,
    pub(super) imports: &'a [ImportBinding],
    pub(super) rules: &'a BindingRules,
    pub(super) file_id: &'a str,
}

/// The outcome of binding one site.
enum Binding {
    /// Bound to one definition with this provenance.
    Bound(String, EdgeProvenance),
    /// Unresolved; the ids the spelling could mean, possibly none.
    Unbound(Vec<String>),
}

/// Call edges. A call on a self receiver binds to a member of the enclosing
/// type; a call on any other receiver, or on a self receiver no type
/// encloses, is left to the resolver; a plain or qualified call binds by
/// lexical scope.
pub(super) fn call_edges(calls: &[Reference], binder: &Binder, edges: &mut Vec<SourceEdge>) {
    for call in calls {
        let caller = binder.caller(call.site);
        let caller = caller.as_deref();
        let binding = match call.receiver.as_deref() {
            Some(_) if call.receiver_shadowed => Binding::Unbound(Vec::new()),
            Some(receiver) if binder.rules.self_receivers.contains(&receiver) => binder
                .receiver(caller, &call.symbol)
                .unwrap_or_else(|| binder.top_level_self(caller, &call.symbol)),
            Some(_) => Binding::Unbound(Vec::new()),
            None => binder.plain(caller, &call.symbol),
        };
        edges.push(binder.edge(caller, SourceEdgeKind::Calls, call, binding));
    }
}

/// Reference edges: bound by lexical scope, like a plain call.
pub(super) fn reference_edges(
    references: &[Reference],
    binder: &Binder,
    edges: &mut Vec<SourceEdge>,
) {
    for reference in references {
        let caller = binder.caller(reference.site);
        let caller = caller.as_deref();
        let binding = binder.plain(caller, &reference.symbol);
        edges.push(binder.edge(caller, SourceEdgeKind::References, reference, binding));
    }
}

impl Binder<'_> {
    /// Final id of the innermost definition enclosing `site`.
    fn caller(&self, site: Span) -> Option<String> {
        enclosing(&self.scopes.spans, site.start_byte)
    }

    fn info(&self, id: &str) -> Option<&DefinitionInfo> {
        self.scopes.by_id.get(id)
    }

    /// A call on a self receiver: `T::name` for the type `T` around the
    /// caller, bound when exactly one id has that scope. `None` when no `T`
    /// can be named.
    fn receiver(&self, caller: Option<&str>, symbol: &str) -> Option<Binding> {
        let mut target = self.receiver_type(caller?)?;
        target.extend(symbol.split("::").map(str::to_string));
        let ids: Vec<String> = self
            .scopes
            .by_spelling
            .get(&target.join("::"))
            .into_iter()
            .flatten()
            .filter(|id| self.info(id).is_some_and(|info| info.scope == target))
            .cloned()
            .collect();
        if let [id] = ids.as_slice() {
            return Some(Binding::Bound(id.clone(), EdgeProvenance::Receiver));
        }
        Some(Binding::Unbound(ids))
    }

    /// Scope of the innermost type, implementation or interface around
    /// `caller`. Failing that, the scope of a qualified innermost function
    /// minus its own name: `W` for C++ `void W::run()`.
    fn receiver_type(&self, caller: &str) -> Option<Vec<String>> {
        let mut function: Option<&DefinitionInfo> = None;
        let mut current = Some(caller);
        while let Some(id) = current {
            let info = self.info(id)?;
            if is_type_like(info.kind) {
                return Some(info.scope.clone());
            }
            if info.kind == SourceNodeKind::Function && function.is_none() {
                function = Some(info);
            }
            current = info.parent.as_deref();
        }
        let function = function.filter(|function| function.qualified)?;
        let (_, owner) = function.scope.split_last()?;
        Some(owner.to_vec())
    }

    /// A call on a self receiver no type encloses: a plain call where the
    /// dialect's top-level `self` is the file's own scope, else unbound with
    /// no candidates, like a call on any other receiver.
    fn top_level_self(&self, caller: Option<&str>, symbol: &str) -> Binding {
        if self.rules.top_level_self {
            self.plain(caller, symbol)
        } else {
            Binding::Unbound(Vec::new())
        }
    }

    /// A plain or qualified spelling. A name an import binds, and a qualified
    /// spelling whose owner this file carries only by `impl` blocks, are left
    /// to the resolver. Otherwise the spelling binds when exactly one of its
    /// ids is in lexical scope with the longest anchor, and keeps every id as
    /// a candidate when not.
    fn plain(&self, caller: Option<&str>, symbol: &str) -> Binding {
        if self.imported(symbol) || self.owner_only_implemented(symbol) {
            return Binding::Unbound(Vec::new());
        }
        let Some(ids) = self.scopes.by_spelling.get(symbol) else {
            return Binding::Unbound(Vec::new());
        };
        let segments = symbol.split("::").count();
        let ids: Vec<&String> = ids
            .iter()
            .filter(|id| segments > 1 || self.rules.bare_calls_reach_members || !self.is_member(id))
            .collect();

        let caller_scope = caller
            .and_then(|id| self.info(id))
            .map_or(&[][..], |info| info.scope.as_slice());
        let eligible: Vec<(usize, &String)> = ids
            .iter()
            .filter_map(|id| Some((self.anchor_len(id, segments, caller_scope)?, *id)))
            .collect();
        let longest = eligible.iter().map(|(len, _)| *len).max();
        let mut best = eligible.iter().filter(|(len, _)| Some(*len) == longest);
        if let (Some((_, id)), None) = (best.next(), best.next()) {
            return Binding::Bound((*id).clone(), EdgeProvenance::LocalName);
        }
        Binding::Unbound(ids.into_iter().cloned().collect())
    }

    /// Whether the spelling's first segment is the local name of an import of
    /// this file.
    fn imported(&self, symbol: &str) -> bool {
        let first = symbol.split("::").next().unwrap_or(symbol);
        self.imports
            .iter()
            .any(|binding| binding.local_name() == Some(first))
    }

    /// Whether a qualified spelling's owner, the segment before its name, is
    /// carried in this file by `impl` blocks alone. An `impl` does not define
    /// its type: `String::from` beside `impl From<Name> for String` may be
    /// std's, and only the whole graph can show a `struct String` here.
    fn owner_only_implemented(&self, symbol: &str) -> bool {
        let owner = symbol.rsplit("::").nth(1);
        let Some(ids) = owner.and_then(|owner| self.scopes.by_spelling.get(owner)) else {
            return false;
        };
        !ids.is_empty()
            && ids.iter().all(|id| {
                self.info(id)
                    .is_some_and(|info| info.kind == SourceNodeKind::Implementation)
            })
    }

    /// Whether `id` is a member of a type: its innermost enclosing definition
    /// is a type, implementation or interface, or it is a function qualified
    /// by its owner (Go `func (w *Widget) run()`). Only a dialect whose bare
    /// calls cannot reach members asks, so a C++ `void ns::f()` never does.
    fn is_member(&self, id: &str) -> bool {
        let Some(info) = self.info(id) else {
            return false;
        };
        let enclosed = info
            .parent
            .as_deref()
            .and_then(|parent| self.info(parent))
            .is_some_and(|parent| is_type_like(parent.kind));
        enclosed || (info.kind == SourceNodeKind::Function && info.qualified)
    }

    /// Length of `id`'s anchor — its scope minus the spelling's `segments` —
    /// when that anchor is a prefix of the caller's scope; `None` when the id
    /// is out of lexical scope.
    fn anchor_len(&self, id: &str, segments: usize, caller_scope: &[String]) -> Option<usize> {
        let scope = &self.info(id)?.scope;
        let anchor = &scope[..scope.len().saturating_sub(segments)];
        caller_scope.starts_with(anchor).then_some(anchor.len())
    }

    /// The edge for one site: bound, or `Syntax` with its candidates. Either
    /// way it carries the site and any receiver.
    fn edge(
        &self,
        caller: Option<&str>,
        kind: SourceEdgeKind,
        reference: &Reference,
        binding: Binding,
    ) -> SourceEdge {
        let from = caller.unwrap_or(self.file_id);
        let symbol = reference.symbol.clone();
        let edge = match binding {
            Binding::Bound(to, provenance) => {
                SourceEdge::bound(from, to, kind, symbol, reference.site, provenance)
            }
            Binding::Unbound(ids) => {
                SourceEdge::syntax(from, kind, symbol, reference.site, syntax_confidence(kind))
                    .with_candidates(candidate_list(ids))
            }
        };
        match &reference.receiver {
            Some(receiver) => edge.with_receiver(receiver.clone()),
            None => edge,
        }
    }
}

/// Kinds whose members a receiver call reaches.
fn is_type_like(kind: SourceNodeKind) -> bool {
    matches!(
        kind,
        SourceNodeKind::Type | SourceNodeKind::Implementation | SourceNodeKind::Interface
    )
}

/// Sorted, deduplicated candidates; empty past [`MAX_CANDIDATES`], where the
/// edge is plain unresolved.
fn candidate_list(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids.dedup();
    if ids.len() > MAX_CANDIDATES {
        ids.clear();
    }
    ids
}
