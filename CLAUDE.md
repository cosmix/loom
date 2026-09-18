# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

**⚠️ KNOWLEDGE-FIRST:** `doc/loom/knowledge/INDEX.md` is the map of this project's curated knowledge. Read it before exploring the tree, then only the sections it points to; pull a specific question with `loom knowledge context --query` (the sections come back quoted). Inside a loom stage the signal's Knowledge Brief comes first.

## Project Overview

Loom is a self-propelling agent orchestration CLI written in Rust. It coordinates Claude Code sessions across git worktrees, enabling parallel task execution with automatic crash recovery and context handoffs.

This is an unreleased project under active development NO BACKWARDS COMPATIBILTY OR MIGRATION ROUTINES SHOULD BE ADDED AT THIS STAGE.

## Build Commands

```bash
cd loom
cargo build                    # Development build
cargo build --release          # Release build
cargo test                     # Run all tests
cargo test stage_transitions   # Run single test file
cargo test --test e2e          # Run end-to-end tests
cargo clippy -- -D warnings    # Lint with warnings as errors
cargo fmt --check              # Check formatting
```

Tests use `serial_test` for isolation - many tests cannot run in parallel.

## Plans

Look in doc/plans for the plan files!

## Remember your Mandatory Rules

Claude.md rules are mandatory and supersede any Plan Mode or other Claude instructions!

Ignore the test-project, unless the user explicitly asks you to read or work with it to test a loom feature.

Remember: NEVER write plans to ~/.claude/plans. It is strictly forbidden.
