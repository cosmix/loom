# Typed Config Values: Known Gaps

> Accepted gaps in the config read-path
> read-path seam. See [Typed Config Values](../architecture/config-value-types.md) for the
> architecture.

- **No control-character stripping in the TUI's value cell.** `tui/render.rs`'s `value_cell` renders
  `ConfigValue::Text` straight into a ratatui `Cell`. Harmless today because no `KEYS` entry uses
  `ValueKind::String` and the TUI only edits the operator's own `~/.loom/config.toml`; strip control
  characters before the first `String`-kind key is registered.
- **No non-interactive way to unset a user-tier config key.** `user_config::write::unset`
  (`write.rs:61`) is reached only from the dashboard's `POST` `value:null` path
  (`config_api/apply.rs:33`); the CLI and TUI never call it. An operator can only clear a user-tier
  key by editing `~/.loom/config.toml` directly or through the settings page. Also: a bare negative
  CLI value needs `--` (`loom config -k context.ceiling_tokens -- -1`) because clap reads `-1` as a
  flag.
- **`#[serde(untagged)] ConfigValue` cannot carry the registry's error wording for a malformed JSON
  number.** A `POST /api/config` body with a `Number` field of `-1` or `1.5` fails all three untagged
  variants before any validator runs, so the response is serde's generic "data did not match any
  variant" instead of the registry's "is not a u32 (expected a non-negative integer)". Accepted as a
  wire-format limitation, not a bug to fix reactively.
- **TUI test coverage cannot drive a `ValueKind::String` row through `ConfigState::cycle`.** Rows are
  built only from the static `KEYS` registry (no key has kind `String`), and `ConfigState`'s fields
  are private to `tui::state`, unreachable from the sibling `tui::tests` module. `String`'s
  refuse-to-cycle behaviour is covered only indirectly (sharing the `Number` arm in `cycle`, plus an
  `opens_editor(&ValueKind::String)` assertion). Closing this needs either a registered `String` key
  or a `pub(super)` test constructor for `ConfigState`.
