//! `loom config` — read or write the user config at `~/.loom/config.toml`.
//!
//! Never reads or writes a workspace config: this command needs no git repo
//! and no `.loom/work/`. The workspace fallback tier that makes
//! `terminal.backend`/`context.ceiling_tokens` visible to `loom run` lives in
//! `crate::fs::work_dir::read_terminal_config`/`read_context_config`, on the
//! opposite (read) side of the workspace/user split from this module.

use anyhow::Result;

use crate::cli::types_config::ConfigArgs;
use crate::user_config::{keys, UserConfig};

#[cfg(test)]
mod tests;

/// Dispatch a parsed `loom config` invocation.
///
/// The work below is split into functions that return the exact `String`
/// they want printed rather than printing themselves, so tests can assert on
/// output without touching stdout. This is the only function that prints.
pub fn execute(args: ConfigArgs) -> Result<()> {
    let output = if args.list {
        list()?
    } else if args.print {
        print_resolved()?
    } else {
        match (args.key, args.value) {
            (Some(key), Some(value)) => set_key(&key, &value)?,
            (Some(key), None) => print_key(&key)?,
            (None, _) => print_resolved()?,
        }
    };
    print!("{output}");
    Ok(())
}

/// `loom config -k <key>` — the bare resolved value, nothing else.
fn print_key(key: &str) -> Result<String> {
    let spec = keys::spec(key)?;
    let config = UserConfig::load_strict()?;
    let (value, _origin) = config.value_of(spec);
    Ok(format!("{value}\n"))
}

/// `loom config -k <key> <value>` — validate, write, then report the change.
///
/// The old and new values come back from [`crate::user_config::set`] itself,
/// captured inside its single locked read-modify-write, rather than from a
/// separate load-before/load-after here — two concurrent `set_key` calls
/// racing a bare load-set-load would otherwise be able to print an `old ->
/// new` line naming a value neither invocation wrote.
fn set_key(key: &str, raw_value: &str) -> Result<String> {
    let spec = keys::spec(key)?;
    let value = spec.parse(raw_value)?;
    let (old, new) = crate::user_config::set(spec, value)?;
    Ok(format!("{}: {old} -> {new}\n", spec.name))
}

/// `loom config --list` — every key, its resolved value, and its origin,
/// column-aligned.
fn list() -> Result<String> {
    let config = UserConfig::load_strict()?;
    let rows: Vec<(&str, String, String)> = keys::KEYS
        .iter()
        .map(|spec| {
            let (value, origin) = config.value_of(spec);
            (spec.name, value, origin.to_string())
        })
        .collect();
    let key_width = rows.iter().map(|(k, ..)| k.len()).max().unwrap_or(0);
    let value_width = rows.iter().map(|(_, v, _)| v.len()).max().unwrap_or(0);
    let mut out = String::new();
    for (key, value, origin) in rows {
        out.push_str(&format!(
            "{key:key_width$}  {value:value_width$}  {origin}\n"
        ));
    }
    Ok(out)
}

/// `loom config --print` (also the no-flags default) — the resolved config as
/// TOML.
fn print_resolved() -> Result<String> {
    let config = UserConfig::load_strict()?;
    Ok(config.to_toml_string())
}
