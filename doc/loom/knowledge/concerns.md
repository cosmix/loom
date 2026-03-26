# Concerns & Technical Debt

> Technical debt, warnings, issues, and improvements needed.
> This file is append-only - agents add discoveries, never delete.
>
> **Related files:** [mistakes.md](mistakes.md) for lessons learned, [architecture.md](architecture.md) for context.

## Architecture Concerns

### Layering Violations (2026-01-29)

> **Full details:** See [architecture.md § Review Findings - Layering Violations](architecture.md#review-findings---layering-violations-2026-01-29)

Critical violations where lower layers import from higher layers:

- daemon imports commands (mark_plan_done_if_all_merged)
- orchestrator imports commands (check_merge_state)
- git/worktree imports orchestrator (hook config)
- models imports plan/schema (type definitions)

## Security Concerns

### Release Asset Verification Gap

Only binary files are signature-verified via minisign. Non-binary release assets lack verification:

- CLAUDE.md.template
- agents.zip
- skills.zip

**Recommended:** Add SHA256 checksum verification for all release assets.

## Code Quality Concerns

### Code Consolidation Needed

> **Full details:** See [conventions.md § Code Consolidation Opportunities](conventions.md#code-consolidation-opportunities-2026-01-29)

Key duplications needing consolidation:

- parse_stage_from_markdown: 4 copies
- branch_name_for_stage: 22+ inline format!() calls
- extract_yaml_frontmatter: 2 copies
- compute_level: 4 copies in status modules

### Debug Output in Production

`eprintln!` statements with 'Debug:' prefix in production code (complete.rs, orchestrator.rs). Should use tracing crate with log levels.

## ReDoS Potential in Plan Pattern Regex

User-provided regex patterns in plan files (failure_patterns, wiring patterns) are compiled and executed without complexity checks. While mitigated by trust model (plan authors = trusted), consider adding regex timeout or complexity limits for defense in depth.

Files: src/verify/baseline/capture.rs:76-79, src/verify/baseline/compare.rs:155-158

## Bootstrap Settings Backup Risk

`bootstrap.rs:write_bootstrap_sandbox()` keeps the settings.local.json backup in memory only (`Option<String>`). If the process is killed between writing sandbox settings and restoring the original, user settings are permanently lost. Low probability since bootstrap is interactive, but a disk-based temp backup would be more robust.

## Bootstrap Tool Restriction Scope

`bootstrap.rs:57` uses `Bash(loom knowledge*)` which allows all knowledge subcommands (init, check, gc, show) not just `update`. Harmless since other subcommands are read-only, but could be tightened to `Bash(loom knowledge update*)` for principle of least privilege.
