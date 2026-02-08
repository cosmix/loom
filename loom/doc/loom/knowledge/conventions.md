# Coding Conventions

> Discovered coding conventions in the codebase.
> This file is append-only - agents add discoveries, never delete.

(Add conventions as you discover them)

## Re-export Visibility for Private Modules

When a module is private (`mod foo`, not `pub mod foo`), types defined within it
that need to be accessed from outside must be re-exported from the parent module
using `pub use foo::TypeName`. Example: `display/mod.rs` re-exports `SessionInfo`
from the private `stages` module.

## Test-Only Types

Types only used in `#[cfg(test)]` code should be gated with `#[cfg(test)]` themselves.
This prevents dead code in production builds and makes intent clear.
