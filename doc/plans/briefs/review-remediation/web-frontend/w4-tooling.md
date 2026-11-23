# web-frontend / w4-tooling: bundle sentinel and build wiring, dependency groups, hooks lint rule

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/web.md`, sections CR-13, CR-64, CR-78.b.

All paths below are relative to `web/` unless they start with `doc/`. Run commands from `web/`.

This worker starts after the other three web workers have finished: its dependency commands rewrite `node_modules` and the lockfile, and a concurrent `vitest run` would fail while packages are being removed and re-added.

## Findings covered

- CR-13: `node scripts/bundle-check.mjs` fails on the committed `dist/` because the last `forbidText` sentinel of `plan.html` occurs in no chunk (`This feature has no metaplan, so no Plan follow-ups can start.`); no script runs the check, so the sentinel rotted unnoticed.
- CR-64: `clsx`, `tailwind-merge` and the build-only packages `shadcn`, `tailwindcss`, `@tailwindcss/vite`, `tw-animate-css` sit under `dependencies`.
- CR-78.b: `.oxlintrc.json` does not enable `react/rules-of-hooks`.

## Files

Own (write):
- `scripts/bundle-rules.json`
- `scripts/bundle-check.test.mjs`
- `package.json` (through `bun` commands only)
- `bun.lock` (through `bun` commands only)
- `.oxlintrc.json`
- `src/aurora-ui/README.md`

Read only: `scripts/bundle-check.mjs`, `src/aurora-ui/shared/lib/cn.ts`, `src/plan/run/run-dialog.tsx`.
Frozen (contract tests, never edit): none for this worker.

## Changes

### 1. CR-13: sentinel

`scripts/bundle-rules.json`, entry `plan.html`, last element of `forbidText`: replace `This feature has no metaplan, so no Plan follow-ups can start.` with `Commit the changes under doc/ first`. That text is in `src/plan/run/run-dialog.tsx` (unique among non-test sources) and in the lazily loaded chunk `dist/assets/handoff-dialogs.js`, which `plan.html` must not carry in its initial closure. Confirm both with `rg -c "Commit the changes under doc/ first" src/plan/run/run-dialog.tsx dist/assets/handoff-dialogs.js` before you edit. `dist/` needs no rebuild for the check to pass (verified against the committed `dist/` with this single substitution: 2 entries, 26 chunks, no violation).

### 2. CR-13: wire the check into the build

Use `bun pm pkg set` (it is the package manager's own editor; never hand-edit):

```text
bun pm pkg set scripts.build="tsc -b && vite build && node scripts/bundle-check.mjs" scripts.bundle-check="node scripts/bundle-check.mjs"
```

`bun pm pkg set` rewrites the file without indentation; run `node_modules/.bin/oxfmt package.json` afterwards and check `git diff package.json` shows only the two script lines. Do not add the check to the `check` script (it needs a built `dist/`).

### 3. CR-13: regression test

In `scripts/bundle-check.test.mjs`, inside `describe("bundle-check CLI")`, add a test that runs the script with no override against the real committed `dist/` and expects exit status 0 and empty stderr. Spawn it like the existing `run` helper does (`spawnSync(process.execPath, [resolve(scriptDir, "bundle-check.mjs")], { cwd: webDir, encoding: "utf8", env })`) but build `env` from `process.env` without `LOOM_BUNDLE_CHECK_DIST`, so an outer override cannot hide a stale sentinel. Skip it with `it.skipIf(...)` when `dist/.vite/manifest.json` (under `webDir`) does not exist; add `existsSync` to the existing `node:fs` import. It fails today with `plan.html: sentinel ... occurs in no chunk`.

### 4. CR-64: dependency groups (do this after items 1 to 3 and 5)

First confirm nothing imports the two packages: `rg -n 'from "(clsx|tailwind-merge)"' src scripts` finds nothing (`src/aurora-ui/shared/lib/cn.ts` already re-exports `cn` from the `cn` package; the validation report describes an older version of that file). Then, in this order:

```text
bun remove clsx tailwind-merge
bun remove shadcn tailwindcss @tailwindcss/vite tw-animate-css
bun add -d --exact shadcn@4.23.0 tailwindcss@4.3.5 @tailwindcss/vite@4.3.5
bun add -d tw-animate-css@^1.4.0
```

`class-variance-authority` keeps `clsx` as its own transitive dependency; that is fine. Never edit `package.json` or `bun.lock` by hand. If a `bun` command cannot reach the registry, stop and report the exact error; do not work around it. After the commands, `package.json` must list those four packages only under `devDependencies`, with the exact versions above for `shadcn`, `tailwindcss`, `@tailwindcss/vite` and `^1.4.0` for `tw-animate-css`, and must not list `clsx` or `tailwind-merge`. Run `node_modules/.bin/oxfmt package.json` if the file lost its formatting.

`src/aurora-ui/README.md`: the paragraph says "the one local change is `shared/ui/button.tsx` ... and the storage prefix in `shared/atoms/theme.ts`". Rewrite that clause to "the local changes are" and add `shared/lib/cn.ts`, which re-exports `cn` from the `cn` package so the kit needs no second class-merging library. Keep the paragraph one sentence and plain.

### 5. CR-78.b: hooks rule

`.oxlintrc.json` gets a `plugins` entry and rules. Setting `plugins` replaces oxlint's default set, so list the defaults too. Add at the top level:

```json
"plugins": ["unicorn", "typescript", "oxc", "react"],
```

and extend `rules` (next to `no-restricted-imports`) with `"react/rules-of-hooks": "error"` and these seven rules set to `"off"`, because enabling the `react` plugin switches on its correctness rules and they would raise 73 warnings under `--deny-warnings` that this item does not own: `react/refs`, `react/set-state-in-effect`, `react/immutability`, `react/static-components`, `react/globals`, `react/jsx-key`, `react/exhaustive-deps`. Append one entry to the existing `overrides` array: `{ "files": ["**/*.test.tsx", "**/*.test.ts"], "rules": { "react/rules-of-hooks": "off" } }` (test files call helpers named `use*` at the top level, for example `useFakeClock`; do not rename those helpers). Contract tests named `*.contract.test.ts(x)` match that glob.

Verify with `node_modules/.bin/oxlint --deny-warnings` (exit 0, no output). If it reports a `react/...` warning that is not one of the seven, report it with its location instead of turning more rules off. If it reports a `rules-of-hooks` error in a non-test file, that is a real defect from this or another worker's change: report it with its location.

## Interfaces shared with other workers

None. `package.json` has one owner (this worker). The sentinel `plan socket: invalid frame` (index.html entry) must keep occurring in a chunk; `w2-sockets` keeps that text in `src/plan/api/socket.ts`.

## Tests

- CR-13: the new CLI test in item 3 (fails before, passes after item 1).
- CR-64 and CR-78.b: no unit test; the proof is the commands below plus a probe: write a throwaway file outside the repository tree (use `$TMPDIR`) with a conditional `useState` call and run `node_modules/.bin/oxlint -c .oxlintrc.json <that file>`; the hooks error must appear, and be gone for a file named `*.test.tsx`. Delete the probe.

## Proof

```text
node_modules/.bin/vitest run scripts
node_modules/.bin/oxlint --deny-warnings
node_modules/.bin/oxfmt --check
node_modules/.bin/tsc -b --noEmit
d=$(mktemp -d .dist-w4.XXXXXX) && node_modules/.bin/vite build --outDir "$d" --emptyOutDir > /dev/null && LOOM_BUNDLE_CHECK_DIST="$d" node scripts/bundle-check.mjs; s=$?; rm -rf "$d"; echo status=$s
```

The last command proves the dependency moves left the build working and the check green on a fresh build; it must print `status=0`. Do not rebuild `web/dist` itself: the main agent does that once after every worker is done.

## Traps

- `bun remove` followed by `bun add -d` must re-add the same versions; a caret on a pinned package changes the manifest's meaning.
- Never run the dependency commands while another worker's tests are running (this worker starts after them for that reason).
- Enabling the `react` plugin without the seven `off` rules fails `oxlint --deny-warnings` on 73 existing findings; leaving out `unicorn`, `typescript` and `oxc` from `plugins` silently drops their rules.
- The CLI test must not inherit `LOOM_BUNDLE_CHECK_DIST`; the existing tests set it on purpose.
- Do not touch `web/dist`; if `bundle-check` fails on the committed `dist/` for a reason other than the sentinel, report it.
