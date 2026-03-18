# Concerns & Technical Debt

> Technical debt, warnings, issues, and improvements needed.
> This file is append-only - agents add discoveries, never delete.

(Add concerns as you discover them)

## self_update not aligned with non-destructive install (2026-03-18)

`loom self-update` (self_update/mod.rs) still:

1. Writes CLAUDE.md.template directly to CLAUDE.md (correct behavior, but should back up existing first)
2. Wipes entire agents/ and skills/ directories (should use per-item loom-* pattern like install.sh)

Not urgent since project is unreleased and self-update requires GitHub releases. Fix when self-update is next worked on.
