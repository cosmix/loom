# Commit Convention

> Grouped Conventional Commits, no attribution, no trailers

## Required Commit Practice

- Group changes by logical purpose. Keep each fix or feature with its relevant
  tests; separate unrelated changes instead of collecting all work into one commit.
- Use Conventional Commit messages: `type(scope): description`, such as
  `fix(skills): discover project types for skill routing`.
- Never add AI attribution to commit subjects, bodies, or trailers, including
  generated-by boilerplate or AI co-author trailers.

Confirmed by the project owner on 2026-09-09.

## Stage Commit Messages Carry No Trailers, Even When the Harness Asks (2026-09-19)

The harness attribution reminder asks for `Co-Authored-By` and `Claude-Session` trailers. The
`loom-hooks/commit-filter.sh` hook blocks the WHOLE Bash call when a `Co-Authored-By` line names Claude or
Anthropic, and a `git add` chained in the same call does not run either. This file's rule wins: write stage commit
messages with no trailers and no attribution line.
