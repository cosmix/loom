---
name: loom-refactoring
description: Restructures existing code to improve readability, maintainability, and performance without changing behavior. Use for extracting methods/classes, removing duplication, applying design patterns, improving organization, and reducing technical debt. Not for bug fixes (use loom-debugging) or new features.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - refactor
  - restructure
  - rewrite
  - clean up
  - simplify
  - extract
  - inline
  - rename
  - move
  - split
  - merge
  - decompose
  - modularize
  - decouple
  - technical debt
  - code smell
  - DRY
  - SOLID
  - improve code
  - modernize
  - reorganize
---

# Refactoring

## Overview

Change the internal structure of code without changing its observable behavior. Restructuring and behavior change are two different activities — never do both in one commit.

## The cardinal rule

**Behavior-preserving, tests green before AND after.** If you can't prove behavior is unchanged, you're not refactoring — you're rewriting, and you need different discipline (characterization tests, feature flags; see `/loom-code-migration`).

Corollaries an expert never violates:

1. **Tests are the safety net.** Run them before you touch anything (establish green). If there are none for the code you're changing, write **characterization tests** first (below) — do not refactor untested legacy blind.
2. **Tiny reversible steps.** One mechanical move at a time; re-run tests after each. When something breaks you know exactly which step did it and can `git checkout` a single step, not an hour of work.
3. **Separate refactor commits from behavior-change commits.** A `refactor:` commit must be a no-op at runtime — reviewers can trust the diff without re-verifying logic. Mixing a bug fix into a rename hides the fix and poisons `git bisect`. (Commit mechanics: `/loom-git-workflow`.)
4. **Don't change tests and production code in the same step.** If a refactor "requires" editing a test's assertions, behavior changed — stop and reconsider.

## Not this skill

- Fixing a bug → `/loom-debugging` (that *is* a behavior change; write the failing test first).
- Adding a feature → implement it, then refactor separately.
- Migrating framework/API/schema → `/loom-code-migration` (behavior *does* move; needs parallel-run/rollback).

## Name the smell, then apply the matching move

Refactoring is not "make it nicer" — identify a specific smell and apply its canonical move. If you can't name the smell, don't refactor.

| Smell                    | Signal                                             | Move                                                        |
| ------------------------ | -------------------------------------------------- | ---------------------------------------------------------- |
| Long method (>50 LOC)    | Scrolling to read one function; comment-delimited sections | Extract Method; Decompose Conditional                      |
| Large class (>300 LOC)   | Many unrelated fields/methods; low cohesion        | Extract Class; Move Method                                 |
| Duplicated logic         | Same block in 2+ places (will drift)               | Extract Method/Function, then hoist to shared location     |
| Feature envy             | Method uses another object's data more than its own | Move Method to the class that owns the data                |
| Primitive obsession      | Bare strings/ints for domain concepts; validation scattered | Introduce Value Object / newtype                           |
| Data clumps              | Same 3-4 params travel together everywhere         | Introduce Parameter Object                                 |
| Shotgun surgery          | One conceptual change forces edits in many files   | Consolidate the responsibility into one module             |
| Divergent change         | One module changed for many unrelated reasons      | Split by axis of change (SRP)                              |
| Deep nesting (>3)        | Arrow-shaped code                                  | Guard clauses; Extract Method                              |
| Type-code switch         | `switch`/`match` on a type tag, repeated           | Replace Conditional with Polymorphism                      |
| Magic number/string      | Unexplained literal                                | Named constant                                             |
| Speculative generality   | Abstraction/param with one caller "for later"      | Inline it — YAGNI                                          |

**Inverse move matters too:** over-abstraction is a smell. Inline needless indirection; a single-implementation interface or a one-caller helper usually earns removal, not preservation.

## Workflow

```text
1. Green baseline   → run the full suite; if red, stop (fix or characterize first)
2. Name the smell   → pick ONE move from the table
3. Apply one step   → smallest mechanical edit; prefer IDE/tool rename over hand-edit
4. Re-run tests     → green? commit. red? git checkout -- <files> and rethink
5. Repeat           → next step
6. Final gate       → full suite + lint + build green; diff is behavior-neutral
```

Prefer tool-driven moves when available: LSP/IDE "rename symbol" and "extract function" are behavior-preserving by construction and update all references — safer than `rg`+Edit. Use `rg` first to size the blast radius of any rename.

## Characterization tests (before touching legacy)

When code has no tests, capture *what it currently does* (bugs and all) before changing structure. You are pinning behavior, not asserting correctness.

```python
# Feed representative + edge inputs, snapshot whatever comes out.
# The point is a tripwire: if a refactor changes output, the test breaks.
@pytest.mark.parametrize("inp", [normal, empty, boundary, weird_unicode])
def test_characterize_legacy(inp):
    assert legacy_process(inp) == SNAPSHOT[inp]   # golden-master
```

For wide surfaces, golden-master / approval testing (snapshot the output of many real inputs) beats hand-writing assertions. If current behavior is genuinely undefined (nondeterministic, time-dependent), pin the invariants you *can* (length, sorted-ness, schema) rather than exact values.

## Canonical moves (representative examples)

Two moves cover most day-to-day work; the rest are in the table above.

### Extract Method — the workhorse

```python
# Before: comment-delimited sections = extraction seams
def process_order(order):
    if not order.items: raise ValueError("Empty order")   # validate
    discount = order.total * 0.1 if order.customer.is_premium else 0  # discount
    order.final_total = order.total - discount; order.save()          # finalize

# After: each section named, independently testable
def process_order(order):
    validate_order(order)
    order.final_total = order.total - calculate_discount(order)
    order.save()
```

### Guard clauses — kill nesting

```python
# Before: arrow code                    # After: flat, early-return
def pay(e):                             def pay(e):
    if e.active:                            if not e.active: return 0
        if e.full_time:                     if not e.full_time:
            return e.salary                     return e.hourly_rate * e.hours
        else:                               return e.salary
            return e.hourly_rate * e.hours
    return 0
```

Replace-conditional-with-polymorphism, introduce-parameter-object, and extract-class follow the same discipline: one behavior-neutral step, tests green between each.

## Gotchas

- ⚠ **Reflection / string-keyed dispatch / serialized names.** IDE rename misses symbols referenced by string (DI containers, ORM column names, JSON keys, dynamic `getattr`). `rg` the *string* form too.
- ⚠ **Public API is not yours to "clean."** Renaming an exported symbol with external consumers is a migration, not a refactor — deprecate + shim (`/loom-code-migration`).
- ⚠ **Refactoring hot paths.** "Extract method" can add call overhead / defeat inlining in tight loops. Measure, don't guess — profile before and after (`/loom-performance-testing`).
- ⚠ **Formatter noise.** A reformat mixed into a logic-bearing diff makes review impossible and causes merge conflicts with sibling loom stages. Keep pure-format commits separate; make surgical edits.
- ⚠ **"While I'm here" scope creep.** Each changed line must trace to the named smell. Unrelated improvements → note them (`concerns.md` / `loom memory note`), don't do them now.
- ⚠ **Behavior leaks disguised as cleanup:** changing a default, tightening a type, reordering side-effecting calls, or "simplifying" a short-circuit are behavior changes. If output could differ for *any* input, it's not a refactor.

## When to stop

Commit progress and escalate (to `loom-senior-software-engineer` or a specialist) if: tests fail in a way you don't understand, scope is ballooning past the named smell, the change touches a public API or a perf-critical path, or architectural implications are unclear. A clean partial refactor beats a broken big-bang one.

## Verify before done

- [ ] Full suite green BEFORE first change (or characterization tests written)
- [ ] Each step was one behavior-neutral move; tests green between steps
- [ ] No production + test logic changed in the same step
- [ ] Refactor commits contain no behavior change; any fix/feature is a separate commit
- [ ] String/reflection/serialized references caught (not just symbol references)
- [ ] Final: suite + lint + build green; diff traces entirely to the named smell(s)
- [ ] No formatter-only churn mixed into logic commits
