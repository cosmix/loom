# W4 — `AGENTS.md.template`, the codex-side doctrine

Tier: codex `gpt-5.6-sol`, effort `xhigh` — the top codex tier, because this is doctrine
authoring rather than plumbing, and the document you are writing is addressed to codex.

## File you own (write)

- `AGENTS.md.template` — new file, repository root. Nothing else.

Read-only: `hooks/codex-forward.sh` (lines 32-80 hold the preamble already sent to codex on every
forwarded task), `CLAUDE.md.template` (the Claude-side equivalent — a source of rules, **not** a
document to copy).

## Why this file exists

`loom install-assets` writes it, with a timestamp header, to `~/.codex/AGENTS.md`. That path was
verified to load: writing a marker there and running `codex debug prompt-input` shows the marker
in the model-visible prompt. Codex never reads `CLAUDE.md`. So today a codex session starts with
no loom doctrine at all unless `hooks/codex-forward.sh` prepends it — which only covers tasks
forwarded through loom's wrapper, not an interactive `codex` the user starts themselves.

## Two constraints that shape everything

1. **It is global.** `~/.codex/AGENTS.md` loads in every codex session in every repository on the
   machine, including ones with nothing to do with loom. Write it so it degrades gracefully:
   universal rules (tool preferences, writing style, no attribution, complete code) stated flatly;
   loom-specific rules explicitly conditioned — "when your task comes from a loom stage", "in a
   loom worktree". A rule that reads as nonsense in an unrelated repository is a defect.
2. **It has a hard size budget: 12288 bytes.** A test asserts it
   (`AGENTS_MD_TEMPLATE_MAX_BYTES` in `loom/src/assets/mod.rs`, written by another worker this
   stage). The external cap is codex's `project_doc_max_bytes`, which defaults to 32768 bytes and
   truncates past it; loom holds itself well under that because this text is paid on every session.
   For scale: `CLAUDE.md.template` is 28,193 bytes and would nearly hit the external cap on its
   own. Do not copy it.

## What to cover

Ordered roughly by how often it will save a run:

- **Navigate with the source graph, do not page files.** `loom map --find-all`, `--outline`,
  `--impact`, `loom knowledge context --query "…" --budget-tokens 1500`, then `rg` for literal
  text and `sed -n 'a,bp'` for the lines a lookup named. Quote the exact command forms; codex has
  no Read tool and will otherwise `cat` whole files. Include the two warnings these commands emit
  inside a worktree (`could not refresh …`) and that they are not failures — the answer comes from
  the published base layer, which reflects the branch point, so a file you already edited must be
  read directly.
- **Do not read `CLAUDE.md`** — it instructs a different agent. **Do not read
  `doc/loom/knowledge/` file by file** — it is a ~200k-token corpus and
  `loom knowledge context --query` is how it is queried.
- **Write scope.** Only the files the task assigns. Never anything under `.loom/` or the legacy
  `.work/` — `.loom/work` is a symlink to state shared with every parallel stage.
- **Never run `git`** — not add, commit, checkout, stash or restore. An orchestrator commits.
- **Do not verify** when the task came from a loom stage: no full build, test suite, linter,
  formatter or type-checker, and never a looping check. At most one narrowly-scoped check over
  the files you changed, run once, skipped if unsure.
- **Finish by reporting** files changed, assumptions made, anything unresolved.
- **Tools:** `rg` not `grep`, `fd` not `find`; `uv` and `bun`/`bunx`, never `pip` or `npm`; never
  hand-edit a dependency manifest — use `cargo add`, `bun add`, `uv add`, `go get`.
- **No placeholders.** No stubs, `pass` bodies, empty functions or pseudocode; decompose instead.
- **Plans** belong in `./doc/plans/`, never in `~/.claude/plans/` or any `.claude/plans` path.
- **No attribution.** Never mention any AI system in code, commits, documentation or comments.
- **Commits** follow Conventional Commits (`type(scope): description`) when a session is ever in a
  position to write one.
- **Size limits:** 400 lines a file, 50 a function, 300 a class.
- **Memory** goes through `loom memory note` / `loom memory decision`, never any editor-managed
  auto-memory directory.
- **Writing style:** laconic by default; no throat-clearing, no fake candour, no contrastive
  "not X, but Y" constructions, no closing summary that restates the paragraph above.

## Relationship to `hooks/codex-forward.sh`

The wrapper's preamble stays exactly as it is — it is the per-task stage contract for a forwarded
unit of work, and it is the one channel an orchestrator writing a prompt cannot forget. This file
is the standing doctrine for any codex session on the machine. Overlap on the hard prohibitions
(no git, no `.loom/` writes, no verification) is deliberate belt-and-braces; **contradiction is
not**. Read the wrapper's text and make sure nothing here weakens or restates it differently.

Do not add a line telling codex to read `AGENTS.md`, and do not reference the wrapper by path — the
file must make sense to a codex session that has never heard of loom's forwarding lane.

## Format

Plain markdown. Every fenced block carries a language tag. No frontmatter — codex reads the file
as prose. Do not add the `# ─── claude-loom | updated …` banner; `loom install-assets` prepends it.

## Done means

The file exists, is at most 12288 bytes (`wc -c AGENTS.md.template`), and reads correctly both
inside a loom worktree and in an unrelated repository.

## Constraints the graph will not show you

- Do not run `git`. Do not touch any file but this one.
