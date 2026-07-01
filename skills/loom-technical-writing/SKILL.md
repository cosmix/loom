---
name: loom-technical-writing
description: Professional technical documentation writing for software projects. Use for READMEs, user guides, tutorials, migration guides, changelogs, API documentation, error messages, and release notes. Covers style, tone, voice, clarity, and audience-appropriate writing.
triggers:
  - technical writing
  - documentation
  - docs
  - readme
  - guide
  - tutorial
  - changelog
  - migration guide
  - write docs
  - document code
  - documentation style
  - writing guide
  - tone
  - voice
  - clarity
  - concise
  - user documentation
  - developer documentation
  - API writing
  - API docs
  - error messages
  - release notes
  - technical communication
  - writing for developers
  - Diataxis
---

# Technical Writing

## Overview

Style, voice, clarity, and structure for software documentation. This skill owns HOW to write well; for the required structure of each artifact (README, ADR, changelog, API reference, docstrings) see `loom-documentation`. Always read the code before writing — never document from assumption.

## Diátaxis: pick one mode per document

The load-bearing framework. Docs fail mostly by mixing modes — a tutorial that digresses into API tables, a reference padded with narrative. Each document serves ONE of four user needs:

| Mode        | User is…            | Serves       | Voice                          | Anti-pattern if mixed          |
| ----------- | ------------------- | ------------ | ------------------------------ | ------------------------------ |
| Tutorial    | learning            | acquisition  | "we will…", hand-held, safe    | reference detail derails it    |
| How-to      | working toward goal | application  | "to do X, do Y", imperative    | teaching concepts slows it     |
| Reference   | looking something up| information  | neutral, exhaustive, dry       | opinions/steps bloat it        |
| Explanation | trying to understand| understanding| discursive, "why", trade-offs  | step lists flatten the "why"   |

Rules: title how-tos by the goal ("Deploy to staging", not "Deployment"). Tutorials must succeed on a clean machine end-to-end. Reference mirrors code structure and stays complete. Explanation (ADRs, design docs) argues the "why". When a page wants to do two jobs, split it and cross-link.

## Know your audience

- **Developers** — API details, code examples, technical depth.
- **End users** — tasks and outcomes; minimal jargon.
- **Operators/DevOps** — deploy, config, monitoring, troubleshooting.
- **New contributors** — onboarding, architecture overview, contribution workflow.

## Prose style

- **Active voice.** "The function returns X" not "X is returned".
- **Imperative for instructions.** "Run the command" not "You should run".
- **Present tense** for current behavior.
- **One idea per sentence.** Split any sentence with two clauses joined by "and/but/which"; aim 15-25 words. Delete throat-clearing ("It should be noted that…").
- **Front-load.** Lead each paragraph, section, and sentence with its most important point; put the conclusion first, support after. Readers scan — reward the scan.
- **Specific, not hedged.** "Returns an empty list" not "returns the result"; "does X" not "might do X". Every "usually/typically/generally" hides an unstated condition — state it.
- **Minimalism.** Cut words that don't change meaning: "in order to"→"to", "is able to"→"can", "at this point in time"→"now". If a sentence survives deletion, delete it.
- **Parallel structure** in lists; consistent terminology (one term per concept — never alternate "user/account/profile" for the same thing).
- **Define terms on first use**; expand acronyms once.

```text
BEFORE: In order to be able to configure the timeout, it is possible for
        the user to make use of the --timeout flag if desired.
AFTER:  Set the timeout with the --timeout flag.
```

## Docs-as-code

Treat docs like source: in the repo next to the code, changed in the same PR, reviewed, linted, and tested in CI.

- Markdown/MDX under version control; docs PR reviewed like code.
- Lint prose and structure (markdownlint, Vale) in CI; fail the build on broken internal links.
- Test code samples — extract and run them, or use doctests (rustdoc, `pytest --doctest-modules`) so examples can't silently rot.
- Version docs with the code they describe; update docs in the same commit as the behavior change (stale docs are worse than none).
- Generate reference where possible (typedoc, rustdoc, OpenAPI) so it can't drift.

## Error messages

User-facing errors are documentation. Structure: **what happened → why → what to do.** Be specific; give an actionable next step; never blame the user; keep a neutral tone.

```text
GOOD:
Error: Configuration file not found at './config.json'
The app looks for config.json in the current directory.
Fix: create config.json in your project root, or pass --config /path/to/config.json.

BAD:
Error: null reference exception
Invalid input
```

Prefer "The file was not found" over "You didn't provide a file".

## Release notes vs changelog

A **changelog** is exhaustive reference (see `loom-documentation`). **Release notes** are curated communication — lead with user impact, group by Features / Improvements / Bug Fixes / Breaking Changes, and provide a migration path for every breaking change.

```markdown
# v2.5.0 — January 2026

## Highlights
One paragraph on the most important change and why users care.

## Breaking Changes
### Changed authentication flow
**Impact:** all API clients must update auth code.
Before: `client.authenticate(token)`
After:  `client.authenticate({ token, type: 'bearer' })`
**Migration deadline:** v3.0.
```

## Migration guide

Structure: overview (why migrate) → breaking changes as before/after code pairs → numbered step-by-step → deprecation timeline (when old is removed). Make the happy-path upgrade copy-pasteable.

## Anti-patterns

- Mixing Diátaxis modes in one page (most common failure).
- Passive voice and hedging that obscure who does what and what actually happens.
- Wall-of-text with no front-loading; burying the key point in paragraph three.
- Inconsistent terminology for one concept.
- Untested examples; docs updated in a separate PR from the code (they drift).

## Verify before done

- [ ] Document serves exactly one Diátaxis mode, titled/voiced to match.
- [ ] Active voice, present tense, imperative instructions; no hedging.
- [ ] Front-loaded: key point first in each section/paragraph/sentence.
- [ ] Terminology consistent; acronyms expanded on first use.
- [ ] Code samples run; internal links resolve; artifact structure matches `loom-documentation`.
- [ ] Written for the identified audience at the right depth.
