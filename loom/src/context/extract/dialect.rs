//! The dialect table: which languages the source graph knows, by extension.
//!
//! Always compiled, with no `cfg`, so a build without a grammar pack can still
//! name the dialects it cannot parse and report them as gaps instead of
//! mislabelling them as unknown files. Stage and skill file detection
//! (`crate::language`) shares this extension registry; project detection uses
//! separate manifest evidence rules.

use std::path::Path;

use crate::context::source_graph::NodeLanguage;

/// A cargo feature bundle providing a set of tree-sitter grammars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrammarPack {
    /// Rust, TypeScript, TSX, JavaScript, Python and Go.
    Core,
    /// Java, C#, Ruby and PHP.
    WaveB,
    /// C and C++.
    WaveC,
}

impl GrammarPack {
    /// The cargo feature that compiles this pack in.
    pub fn feature(self) -> &'static str {
        match self {
            GrammarPack::Core => "source-graph",
            GrammarPack::WaveB => "source-graph-wave-b",
            GrammarPack::WaveC => "source-graph-wave-c",
        }
    }

    /// Whether this build includes the pack's grammars.
    pub fn compiled(self) -> bool {
        match self {
            GrammarPack::Core => cfg!(feature = "source-graph"),
            GrammarPack::WaveB => cfg!(feature = "source-graph-wave-b"),
            GrammarPack::WaveC => cfg!(feature = "source-graph-wave-c"),
        }
    }
}

/// One row of the dialect table.
#[derive(Debug)]
pub struct DialectSpec {
    /// Equals [`NodeLanguage::as_str`] of `language`.
    pub id: &'static str,
    /// Tag stamped on the dialect's nodes.
    pub language: NodeLanguage,
    /// Resolution family; cross-file resolution never binds across families.
    pub family: &'static str,
    /// Lowercase file extensions without the dot.
    pub extensions: &'static [&'static str],
    /// Grammar crate and version, e.g. `tree-sitter-java 0.23.5`.
    pub grammar: &'static str,
    /// The pack that provides the grammar.
    pub pack: GrammarPack,
    /// Receiver spellings that mean "the enclosing type".
    pub self_receivers: &'static [&'static str],
    /// Whether an unqualified call may name a member of the enclosing type.
    pub bare_calls_reach_members: bool,
}

/// Every dialect the source graph names, one row each.
pub static DIALECTS: &[DialectSpec] = &[
    DialectSpec {
        id: "rust",
        language: NodeLanguage::Rust,
        family: "rust",
        extensions: &["rs"],
        grammar: "tree-sitter-rust 0.24.2",
        pack: GrammarPack::Core,
        self_receivers: &["self", "Self"],
        bare_calls_reach_members: false,
    },
    DialectSpec {
        id: "typescript",
        language: NodeLanguage::TypeScript,
        family: "ecmascript",
        extensions: &["ts", "mts", "cts"],
        grammar: "tree-sitter-typescript 0.23.2",
        pack: GrammarPack::Core,
        self_receivers: &["this"],
        bare_calls_reach_members: false,
    },
    DialectSpec {
        id: "tsx",
        language: NodeLanguage::Tsx,
        family: "ecmascript",
        extensions: &["tsx"],
        grammar: "tree-sitter-typescript 0.23.2 (tsx)",
        pack: GrammarPack::Core,
        self_receivers: &["this"],
        bare_calls_reach_members: false,
    },
    DialectSpec {
        id: "javascript",
        language: NodeLanguage::JavaScript,
        family: "ecmascript",
        extensions: &["js", "mjs", "cjs", "jsx"],
        grammar: "tree-sitter-javascript 0.25.0",
        pack: GrammarPack::Core,
        self_receivers: &["this"],
        bare_calls_reach_members: false,
    },
    DialectSpec {
        id: "python",
        language: NodeLanguage::Python,
        family: "python",
        extensions: &["py", "pyi"],
        grammar: "tree-sitter-python 0.25.0",
        pack: GrammarPack::Core,
        self_receivers: &["self", "cls"],
        bare_calls_reach_members: false,
    },
    DialectSpec {
        id: "go",
        language: NodeLanguage::Go,
        family: "go",
        extensions: &["go"],
        grammar: "tree-sitter-go 0.25.0",
        pack: GrammarPack::Core,
        self_receivers: &[],
        bare_calls_reach_members: false,
    },
    DialectSpec {
        id: "java",
        language: NodeLanguage::Java,
        family: "java",
        extensions: &["java"],
        grammar: "tree-sitter-java 0.23.5",
        pack: GrammarPack::WaveB,
        self_receivers: &["this"],
        bare_calls_reach_members: true,
    },
    DialectSpec {
        id: "csharp",
        language: NodeLanguage::CSharp,
        family: "csharp",
        extensions: &["cs"],
        grammar: "tree-sitter-c-sharp 0.23.5",
        pack: GrammarPack::WaveB,
        self_receivers: &["this"],
        bare_calls_reach_members: true,
    },
    DialectSpec {
        id: "ruby",
        language: NodeLanguage::Ruby,
        family: "ruby",
        extensions: &["rb", "rake", "gemspec"],
        grammar: "tree-sitter-ruby 0.23.1",
        pack: GrammarPack::WaveB,
        self_receivers: &["self"],
        bare_calls_reach_members: true,
    },
    DialectSpec {
        id: "php",
        language: NodeLanguage::Php,
        family: "php",
        extensions: &["php"],
        grammar: "tree-sitter-php 0.24.2 (php)",
        pack: GrammarPack::WaveB,
        self_receivers: &["$this", "self", "static"],
        bare_calls_reach_members: false,
    },
    DialectSpec {
        id: "c",
        language: NodeLanguage::C,
        family: "c",
        extensions: &["c"],
        grammar: "tree-sitter-c 0.24.2",
        pack: GrammarPack::WaveC,
        self_receivers: &[],
        bare_calls_reach_members: false,
    },
    DialectSpec {
        id: "cpp",
        language: NodeLanguage::Cpp,
        family: "c",
        extensions: &["cc", "cpp", "cxx", "hh", "hpp", "hxx", "h"],
        grammar: "tree-sitter-cpp 0.23.4",
        pack: GrammarPack::WaveC,
        self_receivers: &["this"],
        bare_calls_reach_members: true,
    },
];

/// The dialect owning `path`'s extension, matched case-insensitively.
pub fn dialect_for_path(path: &Path) -> Option<&'static DialectSpec> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    DIALECTS
        .iter()
        .find(|dialect| dialect.extensions.contains(&extension.as_str()))
}

/// The dialect with id `id`.
pub fn dialect_by_id(id: &str) -> Option<&'static DialectSpec> {
    DIALECTS.iter().find(|dialect| dialect.id == id)
}
