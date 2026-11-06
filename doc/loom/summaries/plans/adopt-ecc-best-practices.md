---
kind: document
of: doc/plans/PLAN-adopt-ecc-best-practices.md
file: sha256:5f5bac94266c9219606d480b324514e33136dd73cf77343ab33293ecff4bb41b
cites:
  "doc/plans/PLAN-adopt-ecc-best-practices.md#12-config-protection-hook": "sha256:4f14adf255b26ab935a95619db5042c87cee3972793fd86e8cc5161f15ae6c5b"
  "doc/plans/PLAN-adopt-ecc-best-practices.md#13-file-size-enforcement-hook": "sha256:490b58779a4062d43a02acf90c66c6bb6e38973051ea6afc944dd1ba771a1769"
  "doc/plans/PLAN-adopt-ecc-best-practices.md#14-debug-print-audit-hook": "sha256:24f8dd6ab02f84379a1ca3d4f454fa195764dad6b4d97dae243cd17c1749cc49"
  "doc/plans/PLAN-adopt-ecc-best-practices.md#21-hook-profile-system": "sha256:cff97045558425bf6e0900582d676675f80c2b5daa7c3149ed877b73d4b748cb"
  "doc/plans/PLAN-adopt-ecc-best-practices.md#22-dependency-security-in-pre-commit": "sha256:b77a99bf4f7ad7def6a2d7bbdba574630134e24504ba3130f86a7e129d90eeeb"
  "doc/plans/PLAN-adopt-ecc-best-practices.md#23-rust-specific-rules-file": "sha256:e3e2d21d7797bb13dd5e617afa2b3bb9ce029e2ca3cb42f5013e39deba27b125"
  "doc/plans/PLAN-adopt-ecc-best-practices.md#context": "sha256:aebbb6c0e177264fc0d78d52f88e95fea41a34c4191db5e1b449e09dcaf03b6f"
  "doc/plans/PLAN-adopt-ecc-best-practices.md#implementation-approach": "sha256:d49e589bc96f8e00604f1e2fd0cbc96d60fb9dd476aff92566f142cb29a13fd0"
  "doc/plans/PLAN-adopt-ecc-best-practices.md#tier-1-high-value-address-real-gaps": "sha256:db091131042e2d6e590255706836b437511258530f083f593fed4000b80a6a2b"
  "doc/plans/PLAN-adopt-ecc-best-practices.md#tier-3-lower-value-nice-to-have-defer": "sha256:6577b5d334400081301c15a777788a7d8c6f85196eb23b4fa6f0a44862dedfe8"
  "doc/plans/PLAN-adopt-ecc-best-practices.md#verification": "sha256:bdc564cc9be7cf4199923b795d3999725d91315e9b6d6ca48130c3f81e5e91c9"
  "doc/plans/PLAN-adopt-ecc-best-practices.md#what-loom-already-does-better-than-ecc": "sha256:fffd3830651a7052f6499a871774bd4523c69ebb5a0fec7f8ea535cd4e50cc2a"
  "doc/plans/PLAN-adopt-ecc-best-practices.md#what-not-to-adopt-and-why": "sha256:1d3c1bb70dff6fbd34106636b4a2d4be92b4f66e7ca03569120c4adfc552d04b"
---

# Adopt Best Practices from Everything Claude Code

## Context

The plan identifies which Everything Claude Code patterns would improve loom's workflow and filters out what loom already handles better. [Context](../../../plans/PLAN-adopt-ecc-best-practices.md#context)

Loom's orchestration, signal system, knowledge management, and verification are already more sophisticated than ECC's equivalents. [What Loom Already Does Better Than ECC](../../../plans/PLAN-adopt-ecc-best-practices.md#what-loom-already-does-better-than-ecc)

The largest gains come from ECC's enforcement hooks and Rust-specific quality automation, which loom lacks. [Context](../../../plans/PLAN-adopt-ecc-best-practices.md#context)

## Recommendations

Tier 1 adds post-edit Rust hooks that run cargo fmt, background clippy, and cargo check after edits, because formatting and lint issues currently surface only at commit time. [Tier 1: High Value (address real gaps)](../../../plans/PLAN-adopt-ecc-best-practices.md#tier-1-high-value-address-real-gaps)

A config protection hook blocks Edit and Write changes to rustfmt, clippy, deny, and toolchain configuration files. [Config Protection Hook](../../../plans/PLAN-adopt-ecc-best-practices.md#12-config-protection-hook)

A file size hook blocks Write calls that create files over 400 lines and warns on Edit calls that push a file over the limit. [File Size Enforcement Hook](../../../plans/PLAN-adopt-ecc-best-practices.md#13-file-size-enforcement-hook)

A debug print audit hook warns on `eprintln!("Debug` and `dbg!()` in modified Rust files without blocking them. [Debug Print Audit Hook](../../../plans/PLAN-adopt-ecc-best-practices.md#14-debug-print-audit-hook)

Tier 2 adds a hook profile selected by `LOOM_HOOK_PROFILE` with minimal, standard, and strict levels. [Hook Profile System](../../../plans/PLAN-adopt-ecc-best-practices.md#21-hook-profile-system)

Tier 2 also adds `cargo audit` to the pre-commit chain when Cargo.toml or Cargo.lock changes. [Dependency Security in Pre-Commit](../../../plans/PLAN-adopt-ecc-best-practices.md#22-dependency-security-in-pre-commit)

A Rust rules file at `.claude/rules/rust.md` would cover ownership, error handling, testing, unsafe policy, and iterator preferences. [Rust-Specific Rules File](../../../plans/PLAN-adopt-ecc-best-practices.md#23-rust-specific-rules-file)

Tier 3 items such as session-end memory extraction, desktop notifications, cost tracking, and governance capture are deferred to separate plans. [Tier 3: Lower Value (nice to have, defer)](../../../plans/PLAN-adopt-ecc-best-practices.md#tier-3-lower-value-nice-to-have-defer)

The plan rejects adopting ECC's agent system, skill system, slash commands, session persistence, TDD workflow, and model routing. [What NOT to Adopt (and Why)](../../../plans/PLAN-adopt-ecc-best-practices.md#what-not-to-adopt-and-why)

## Implementation and Verification

Phase 1 creates five hook scripts and updates settings.json, and each hook can be developed and tested in isolation. [Implementation Approach](../../../plans/PLAN-adopt-ecc-best-practices.md#implementation-approach)

Verification runs each hook on simulated JSON input and confirms that blocked edits and warnings behave as specified. [Verification](../../../plans/PLAN-adopt-ecc-best-practices.md#verification)
