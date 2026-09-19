//! How an `rg` or `grep` command line splits into options, patterns and
//! paths. Shared by the hazard lints, which must not read a pattern as a
//! path, and the base-tree evaluator, which reproduces simple searches.

use super::shell_lex::Word;

/// Short options that take a value, for `rg` and for `grep`.
const RG_VALUE_FLAGS: &str = "ABCEMTdefgjmrt";
const GREP_VALUE_FLAGS: &str = "ABCDdefm";

/// Long options of either tool that take their value as the next word.
const LONG_VALUE_OPTIONS: &str = "--glob --iglob --type --type-not --max-count --max-depth \
     --context --after-context --before-context --file --regexp --replace --encoding --threads \
     --include --exclude --exclude-dir";

/// The parsed arguments of an `rg` or `grep` command.
#[derive(Debug, Default)]
pub(super) struct SearchArgs {
    /// Whether the tool is `rg` rather than `grep`.
    pub is_rg: bool,
    /// Every short-option letter given, including value-taking ones.
    pub short_flags: String,
    /// Every long option given (the text before any `=`), and `--` if present.
    pub long_options: Vec<String>,
    /// Each pattern as its argv index and the byte offset it starts at in that word.
    pub patterns: Vec<(usize, usize)>,
    /// Argv indices of the path operands.
    pub operands: Vec<usize>,
}

impl SearchArgs {
    /// Parse `argv` when its command is `rg` or `grep`.
    pub(super) fn parse(argv: &[&Word]) -> Option<Self> {
        let is_rg = match argv.first()?.command_name() {
            "rg" => true,
            "grep" => false,
            _ => return None,
        };
        let mut args = SearchArgs {
            is_rg,
            ..SearchArgs::default()
        };
        let mut positionals = Vec::new();
        let mut options_done = false;
        let mut idx = 1;
        while let Some(word) = argv.get(idx) {
            let value = word.value.as_str();
            if options_done || !value.starts_with('-') || value == "-" {
                positionals.push(idx);
            } else if value.starts_with("--") {
                options_done = value == "--";
                idx += args.long_option(value, idx);
            } else {
                idx += args.short_cluster(&value[1..], idx);
            }
            idx += 1;
        }
        let from_file = args.short_flags.contains('f') || args.has_long("--file");
        if args.patterns.is_empty() && !from_file && !positionals.is_empty() {
            args.patterns.push((positionals.remove(0), 0));
        }
        args.operands = positionals;
        Some(args)
    }

    /// Whether the word at argv index `idx` holds a pattern.
    pub(super) fn is_pattern(&self, idx: usize) -> bool {
        self.patterns.iter().any(|&(at, _)| at == idx)
    }

    /// Whether this is `rg` given `-r`/`--replace`, which grep users read as recursive.
    pub(super) fn replaces(&self) -> bool {
        self.is_rg && (self.short_flags.contains('r') || self.has_long("--replace"))
    }

    fn has_long(&self, name: &str) -> bool {
        self.long_options.iter().any(|option| option == name)
    }

    /// Record the long option at argv index `at`; returns how many following
    /// words it consumes as its value.
    fn long_option(&mut self, value: &str, at: usize) -> usize {
        let (name, inline) = match value.split_once('=') {
            Some((name, _)) => (name, true),
            None => (value, false),
        };
        if name == "--regexp" {
            let pattern = if inline {
                (at, name.len() + 1)
            } else {
                (at + 1, 0)
            };
            self.patterns.push(pattern);
        }
        self.long_options.push(name.to_string());
        let takes_next = !inline && LONG_VALUE_OPTIONS.split_whitespace().any(|o| o == name);
        usize::from(takes_next)
    }

    /// Record the short-option cluster (without its `-`) at argv index `at`;
    /// returns how many following words it consumes as a value.
    fn short_cluster(&mut self, cluster: &str, at: usize) -> usize {
        let value_flags = if self.is_rg {
            RG_VALUE_FLAGS
        } else {
            GREP_VALUE_FLAGS
        };
        for (offset, flag) in cluster.char_indices() {
            self.short_flags.push(flag);
            if !value_flags.contains(flag) {
                continue;
            }
            let value_start = offset + flag.len_utf8();
            let attached = value_start < cluster.len();
            if flag == 'e' {
                let pattern = if attached {
                    (at, value_start + 1)
                } else {
                    (at + 1, 0)
                };
                self.patterns.push(pattern);
            }
            return usize::from(!attached);
        }
        0
    }
}
