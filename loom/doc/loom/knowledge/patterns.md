# Architectural Patterns

> Discovered patterns in the codebase that help agents understand how things work.
> This file is append-only - agents add discoveries, never delete.

(Add patterns as you discover them)

## Shared Utilities Pattern

Generic utility functions (string manipulation, formatting, display helpers) should live in `utils.rs` at the crate root, NOT in layer-specific modules like `commands/common/`. This ensures all layers can import without violating the dependency hierarchy.

Current shared utilities in `utils.rs`:

- `truncate(s, max_chars)` — UTF-8 safe string truncation with "..." ellipsis
- `truncate_for_display(s, max_len)` — Collapses multiline + truncates with "…"
- `format_elapsed(seconds)` — Compact duration formatting
- `format_elapsed_verbose(seconds)` — Verbose duration formatting
- `cleanup_terminal()` / `install_terminal_panic_hook()` — Terminal state restoration
- `context_pct_terminal_color(pct)` / `context_pct_tui_color(pct)` — Color by context %

## Re-export Pattern for Backward Compatibility

When moving functions to a new canonical location, add `pub use` re-exports at the old location so existing in-layer callers don't need updating. Cross-layer callers should be updated to the canonical import path.

## Status Module Cleanup Pattern

When removing display sections from `loom status`:

- Remove the display function call from `execute_static` in `status.rs`
- Remove the render function call from `execute_static`
- Delete the display module file (e.g., `display/sessions.rs`)
- Delete the render module file (e.g., `render/sessions.rs`)
- Update `display/mod.rs` and `render/mod.rs` to remove re-exports
- Remove the data field from `StatusData` struct if no longer needed
- Gate test-only types/functions behind `#[cfg(test)]`

Key insight: When folding information from a removed section into another section,
use a lightweight struct (like `SessionInfo`) rather than reusing the full data type.
Build the mapping in `execute_static` and pass it to the display function.
