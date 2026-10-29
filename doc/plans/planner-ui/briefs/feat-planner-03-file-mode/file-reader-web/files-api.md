# file-reader-web / files-api: data hooks and atoms for file mode

Lane: codex, units R1 (`api.ts`), R2 (`atoms.ts`), R3 (`address.ts`) of `codex-units.md`, wave A.
Each unit owns its file and test only. Read
`doc/plans/briefs/feat-planner-03-file-mode/common.md` first, then plan 2's `web/src/plan/api/{client.ts, socket.ts}`,
`web/src/plan/state/atoms.ts`, and `web/src/plan/api/schema-files.ts` (stage `file-server`).

## Files you own

`web/src/plan/files/api.ts`, `web/src/plan/files/api.test.ts`, `web/src/plan/files/atoms.ts`,
`web/src/plan/files/atoms.test.ts`, `web/src/plan/files/address.ts`, `web/src/plan/files/address.test.ts`.

## api.ts

- `fetchTree(): Promise<ApiResult<FileTree>>` (`GET /api/tree`), `fetchDoc(path): Promise<ApiResult<DocView>>`
  (`GET /api/doc?path=`), `searchDocs(q, scope): Promise<ApiResult<SearchResults>>`,
  `fetchHeadDiff(path)`, `fetchTurnDiff(turn, path)` (`DiffTexts`), and
  `saveDoc(path, text, openedHash, withErrors): Promise<ApiResult<SaveResult>>` (POST text body,
  `contentType: "text"`; 409 and 422 bodies are `SaveResult` values, not errors: return them as
  `ok: true`). Query values with `encodeURIComponent`.
- `useFileSocket(store)`: subscribes to `tree_changed`, `doc_changed`, `validation` through plan 2's
  `subscribe(type, handler)` and writes the atoms below; and registers plan 2's `onResync`
  handler, which refetches the tree and, when a document is open, its `DocView` (the result goes
  through the same stale path as a `doc_changed`, so the "changed just now" note and the merge
  offer still work), and its validation with it. Test: a resync after a missed `doc_changed`
  refetches and marks the open document stale.

## atoms.ts

`treeAtom: FileTree | null`, `openDocAtom: { doc: DocView; seenHash: string } | null`,
`docChangedAtom: Record<string, string | null>` (path -> latest hash from `doc_changed`),
`validationAtom: Record<string, ValidationView>`, `previousOpenTextAtom: { path: string; text: string } | null`
(the text the reader had on screen before the last in-place update; the "changed while open"
diff's before side), derived `openDocIsStaleAtom` (latest hash differs from `seenHash`).

## address.ts

`/files/<path relative to doc/>#<anchor>` addresses (section 06 "Layout and addresses"):
`toAddress(path, anchor?)`, `parseAddress(location): { path: string; anchor: string | null; diff: "turn" | "open" | "head" | null; turn: string | null }`
(query `?diff=<kind>&turn=<id>`), `resolveLink(fromPath, href): { kind: "doc"; path; anchor } | { kind: "anchor"; anchor } | { kind: "external"; href } | { kind: "outside"; target } | { kind: "text" }`
following section 06 "Links" and section 12 "Links". Classification never uses a regex on the raw
string: `#x` (after trimming) -> anchor; otherwise parse with `new URL(href, base)` where base is a
synthetic `https://doc.invalid/<fromPath>` (the browser's own URL parser strips tabs, newlines and
leading spaces exactly as navigation would), and classify by the parsed result: `protocol` in
`http:`, `https:`, `mailto:` with a host other than `doc.invalid` -> external (a protocol-relative
`//evil.example` lands here); `https:` with host `doc.invalid` -> a relative link whose pathname,
normalised, stays under `doc/` -> doc (leaving `doc/` -> `outside`); every other protocol
(`javascript:`, `data:`, `file:`, `vbscript:`, anything) -> `text`; a parse failure -> `text`. The
raw `href` is never placed in an `href` or `title`; only the parsed, classified value is.
Path helpers (binding; plans 5, 9, 11 use them): `docRel(path)` strips a leading `doc/`,
`repoRel(path)` adds it; `toAddress(path, anchor?)` accepts either form; `parseAddress` returns
`path` repo-relative. Query values are encoded with `encodeURIComponent` both ways, so a turn id
such as `3f9a/t2` round-trips.

## Tests

address.test.ts covers every `resolveLink` kind including `JaVaScRiPt:`, `java\nscript:`,
`java&#x09;script:` decoded (`java\tscript:`), `" javascript:"` with a leading space, `\u0000javascript:`,
`//evil.example` (external), `<https://x>`-style autolink targets, `docRel`/`repoRel` round trips,
a turn id with `/` through `toAddress`/`parseAddress`,
`../../outside.md` from `plans/x.md`, `#anchor`, and an encoded path round trip. api.test.ts: save
maps 200/409/422 to `SaveResult` and 403 to an error. atoms.test.ts: stale detection.

## Done when

`cd web && node_modules/.bin/tsc -b --noEmit`, `oxlint --deny-warnings`, `oxfmt --check`,
`vitest run src/plan/files` pass.
