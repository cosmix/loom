# E2: labelled corpora for java, csharp, ruby, php, c, cpp

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 2, alongside E1, after E3 has delivered the evaluator.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` section 14.1 in
  full, and sections 5.1 (the `.h` rule), 6.2 and 7 (rules and path conventions per
  dialect).

## Goal

The same as E1 (read `e1-corpora-core.md` for the corpus rules, which apply
unchanged), for the six grammar-pack dialects.

## Files you own (write)

- `loom/tests/fixtures/source/labeled/{java,csharp,ruby,php,c,cpp}/**`

Read-only: everything else, and `loom/tests/edge_quality_contracts.rs` (frozen).

## Per-language requirements beyond E1's list

- **java:** `src/main/java/<pkg>/...` layout, two packages, a static import, a wildcard
  import, overloads, an inner class, `this.m()`.
- **csharp:** two namespaces (one block, one file-scoped), a `using` alias, a
  `partial class` split across two files (receiver binding across files),
  overloads.
- **ruby:** `lib/` layout, `require_relative`, a module containing a class, a class
  reopened in a second file, `self.m`, a dynamic receiver, `attr_reader` (not a
  call). It also holds the `require` vs `require_relative` pair fixed in the plan's
  E2 amendment: `app/x.rb` and `lib/x.rb` both define `helper`, and `app/a.rb`
  (`require_relative 'x'`) and `app/b.rb` (`require 'x'`) each call `helper()`.
- **php:** PSR-4 layout `src/App/...`, a namespace, a `use ... as` alias, a group use,
  a trait, `$this->m()`, `self::m()`, `static::m()`, a `require_once` of a relative
  path.
- **c:** a header with prototypes and a `.c` file with the definitions, a
  `#include "x.h"`, a `#include <stdio.h>` (external), a static helper, a function
  pointer call (dynamic).
- **cpp:** a namespace, a class declared in a `.h` header and defined out of class in a
  `.cpp` file (`void W::run() {}`), overloads, `this->m()`, and a template function
  called as `f<int>(x)`. Label that call's target as the template function's id: a
  compiler binds it, and `normalize_call` drops the `<int>`.

Each label records what the compiler binds (design 14.1).

## Traps

- Labels never go easy on loom. A declaration the extractor misses is a recall miss,
  which the threshold (`declaration_recall: 1.0`) turns into a stage failure that must
  be fixed in the extractor, never in the label.
- The `sig8` suffix only appears on ids that collide. Compute it for overloads.
- Use a real extension for every syntax-error file (`broken.java` and so on); only
  `.rs` is special under `tests/`. List it under `syntax_error_files:` in
  `labels.yaml`: `Thresholds::check` fails a corpus without one, and also one that
  lacks a `target`, an `external`, an `ambiguous` or an `impact` label (E1's rules
  apply unchanged).

## Order of work and check

Follow E1's order of work: write the labels first, run the evaluator second, and never
edit a label to match loom.

`cargo run --manifest-path loom/Cargo.toml --quiet -- map --eval-edges loom/tests/fixtures/source/labeled 2>&1 | tail -60`

## Report

Report:

- each corpus's files;
- the label counts;
- the evaluator's output for your six dialects;
- each disagreement, classified as a loom defect or a label fix, with the rule that
  decided it.
