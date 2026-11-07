//! The out-of-line modules of a Rust file that rustc compiles from a
//! `#[path = "<rel>"]` attribute under the test cfg: the declarations a
//! moved test file can be collected through (the cross-file move check of the
//! test-integrity scan).

use super::items::{self, Attribute, ModItem};

/// Attributes that leave a declaration compiled whenever the test cfg is on.
const LINT_ATTRIBUTES: [&str; 4] = ["allow", "expect", "warn", "deny"];

/// The `(module name, path attribute value)` of every top-level out-of-line
/// `mod` of `text` carrying exactly one `#[path = "<rel>"]` whose other
/// attributes are only `#[cfg(test)]` or a lint attribute (`allow`, `expect`,
/// `warn`, `deny`). Any other attribute (`cfg(any())`, a feature gate,
/// `cfg_attr`) leaves the module not collected. Declarations inside comments,
/// strings, macro bodies and inline modules are not top-level items, so they
/// are never returned. A file that does not parse cleanly yields nothing.
pub(crate) fn path_modules(text: &str) -> Vec<(String, String)> {
    let Ok(parsed) = items::parse(text) else {
        return Vec::new();
    };
    if parsed.has_error {
        return Vec::new();
    }
    parsed.mods.iter().filter_map(path_module).collect()
}

fn path_module(item: &ModItem) -> Option<(String, String)> {
    if item.inline {
        return None;
    }
    let mut path = None;
    for attribute in &item.attributes {
        if attribute.path.as_deref() == Some("path") {
            if path.replace(path_value(attribute)?).is_some() {
                return None;
            }
        } else if !compiled_under_test(attribute) {
            return None;
        }
    }
    Some((item.name.clone(), path?))
}

fn compiled_under_test(attribute: &Attribute) -> bool {
    attribute.normalised() == "#[cfg(test)]"
        || attribute
            .path
            .as_deref()
            .is_some_and(|path| LINT_ATTRIBUTES.contains(&path))
}

/// The string literal of `#[path = "<value>"]`; `None` for any other
/// spelling (raw string, escape, comment inside the attribute).
fn path_value(attribute: &Attribute) -> Option<String> {
    let inner = attribute
        .text
        .trim()
        .strip_prefix("#[")?
        .strip_suffix(']')?
        .trim()
        .strip_prefix("path")?
        .trim_start()
        .strip_prefix('=')?
        .trim();
    let value = inner.strip_prefix('"')?.strip_suffix('"')?;
    (!value.contains(['"', '\\'])).then(|| value.to_string())
}

#[cfg(all(test, feature = "source-graph"))]
#[path = "tests_path_mods.rs"]
mod tests;
