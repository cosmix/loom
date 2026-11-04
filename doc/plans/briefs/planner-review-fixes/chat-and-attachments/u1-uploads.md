# U1 — file attachments: upload route, storage, delivery (issue 8, server side)

Tier: sonnet (`loom-software-engineer`). This is security-sensitive: follow every rule below exactly, and report anything you had to decide that is not written here. Read `common.md` first, D9 in particular.

## Route

- `POST /api/attachments`, Role `HolderOrTake` (`routes.rs` `Role`, gated as `Take::TakeIfFree`), written `role: Role::HolderOrTake` in its `RouteSpec` in `uploads/mod.rs` (a plan wiring check greps `Role::HolderOrTake` there). A fresh New work page normally has no holder: `canAuthorAtom` enables the form then, the user attaches before Start, and the chat start takes authoring only later (`chat/dispatch.rs` `take_authoring`). `Role::Holder` would answer that first upload 403 `NoHolder`. `TakeIfFree` takes authoring when nobody holds it, as `/api/pressure/start` and file save do; the lease is the ordinary authoring lease, so the later start finds this browser already holding and the away timer releases it if the user leaves. It still refuses another browser (`NotHolder`), read-only mode and `stopping`. The module declares it in `uploads::routes()` (the path literal `"/api/attachments"` lives in `uploads/mod.rs`), and `routes.rs` `all_routes` gains one line, `routes.extend(super::uploads::routes());`, as every other module does. `routes.rs` holds no path literals.
- Add `BodyKind::Bytes { max: usize, types: &'static [&'static str] }` (`routes.rs` `BodyKind`). In `gate.rs` it is refused with 415 unless `Content-Type`, with parameters stripped, is one of `types`, and with 413 when `Content-Length` exceeds `max`, before any body byte is read, as the other kinds are. The planner reads the body by the length the gate checked (`plan_server/request.rs`), so no `web_server` change is needed; do not raise `web_server::http::MAX_BODY_BYTES`.
- Types: `image/png`, `image/jpeg`, `image/gif`, `image/webp`, `application/pdf`, `text/plain`, `text/markdown`. Max 10 MiB.
- Handler: new module `loom/src/plan_server/uploads/` (`mod.rs` holds the routes, the handler and `install`; storage and tests sit in files beside it).
  1. Check the content. Magic bytes: PNG `89 50 4E 47 0D 0A 1A 0A`, JPEG `FF D8 FF`, GIF `GIF87a`/`GIF89a`, WebP `RIFF....WEBP`, PDF `%PDF-`. Text must be valid UTF-8 with no NUL byte. A mismatch is 422 with a one-line reason.
  2. Name. The `name` query parameter (`ApiRequest` query, `QueryParams::get`, already percent-decoded) is display-only. Do NOT add an `X-Loom-Filename` header or any `RequestHead` field: four unowned test literals list every `RequestHead` field.
     - keep the final path component;
     - drop Cc and Cf characters (control and bidi/format) with `plan_server/presence.rs`'s filter: make its `is_format` `pub(crate)` and reuse it (its 32-character cap is not yours; cap at 120 characters);
     - a missing or non-UTF-8 name, or one that ends up empty, becomes `attachment`.
  3. Store at `<runtime>/attachments/<id>.<ext>` (`ctx.runtime().dir.join("attachments")`). The id is the first 16 lowercase hex characters of the content's sha256 (`sha2` and `hex` are already dependencies). The ext comes from the media type, never from the name. Create the directory with `safe_fs::safe_create_dir_all` at 0700. Write a temp file in the same directory with `safe_fs::open_safely` (`O_CREAT|O_EXCL|O_NOFOLLOW`, mode 0600), then rename. An existing file with the same id is reused and its sidecar's `stored_at` is refreshed through `touch`.
  7. Export `pub(crate) fn touch(runtime_dir: &Path, id: &str) -> io::Result<bool>` from `uploads/` (re-exported as `crate::plan_server::uploads::touch`): it checks the id grammar, rewrites the sidecar's `stored_at` to now when the file exists, and returns whether it did. The kickoff stage's draft store calls it on every draft write that references a file attachment, so a file referenced by an unfinished draft lives as long as the draft (both sweeps are 30 days).
  4. A sidecar `<id>.json` holds `{name, media_type, bytes, stored_at}` and never a path; `resolve` re-derives `<id>.<ext>` from the allow-listed media type.
  5. Store cap: refuse with 507 and a one-line reason when the stored files would exceed 256 MiB in total (any holder could otherwise fill the disk with 10 MiB uploads).
  6. Answer `{id, name, media_type, bytes}`.
- Cleanup: `uploads::install(ctx)`, one new line in `routes.rs` `install_all`, removes attachment files and sidecars whose `stored_at` is older than 30 days. It tolerates a missing directory, logs the count, and turns a failure to remove into a warning, never a startup error. Test it with a fixed clock. (`plan_server/startup.rs` only formats startup lines and is not touched.)

## Attachment kind and delivery

- `loom/src/plan_session/attachments/mod.rs`: `AttachmentKind` gains `File`.
  - `AttachmentInput` is a struct with required `path` and `quote` and `deny_unknown_fields`, and `wire/tests_chat.rs` checks serialize(parse(fixture)) == fixture for every entry of `chat-commands.json`, whose source inputs serialize `"anchor": null` and the other null keys. Skipping empty fields would break those entries. So `AttachmentInput` becomes `#[serde(untagged)] enum AttachmentInput { File(FileAttachmentInput), Document(DocumentAttachmentInput) }`, with `File` tried first and `deny_unknown_fields` on both arms. `FileAttachmentInput { kind, id, name }`; `DocumentAttachmentInput` is today's struct.
  - `Attachment` (the parsed chip) gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub file: Option<FileMeta>`, `FileMeta { id, name, media_type, bytes }`. Existing wire fixtures keep their bytes.
  - Every struct literal of either type changes, and each is yours: `attachments/tests.rs`, `attachments/tests/resolve.rs`, `attachments/parse.rs`, `attachments/resolve.rs`, `plan_session/model/tests.rs`, `plan_session/transcript/tests_server.rs`, `plan_server/chat/tests_dispatch.rs`, `plan_server/wire/chat.rs`, `loom/tests/plan_chat_operator/support.rs`, and the two planned frozen edits (common.md "Planned edits of frozen tests"): `transcript/paste_contract_tests.rs` and `tests/plan_session_bridge_core_contracts/records.rs`, each gaining `file: None,` and nothing else. `rg -n 'Attachment(Input)? \{' loom/src loom/tests` before you finish; report any site this list missed.
  - `wire/tests_chat.rs`: add `File` to `attachment_kind_variants_in_fixtures`.
  - `resolve.rs` maps the id to the stored path through the sidecar. It refuses an id that is not 16 lowercase hex characters, and an id with no stored file. The client never supplies a path.
- `format.rs`: a file attachment renders in the `<loom-attachments>` block as `[n] file <absolute path> (<media_type>, <size>) "<name>"`, with no quote. The name goes through the block's `escape_text`, so a `"` in a name cannot forge a chip line. `parse.rs` parses it back into an `Attachment` whose `file` is set (name from the last quoted span). Round-trip both in tests. The chip sent to browsers carries `file` and never the absolute path: `path` is empty for a file chip.
- `plan_server/chat/dispatch.rs` `checked()`: file attachments count toward `MAX_ATTACHMENTS` (10). Their bytes do not count toward `MAX_DELIVERED_BYTES`, because the file travels by path. `dispatch.rs` is 392 lines: put the file handling in `uploads/` or `attachments/` and add at most a few lines here.
- Sandbox:
  - `loom/src/plan_session/sandbox/reads.rs` `RUNTIME_GRANTS` gains the string literal `"attachments"` (as `"claims"` is) as a read-only `allowRead` carve-out. `sandbox/filesystem.rs` gains NO write grant for it.
  - `sandbox/mod.rs` `ensure_runtime_grants` also creates `attachments/` (0700), because srt binds grant paths from the host before each launch. Add a `RuntimePaths.attachments` field in `planning/runtime.rs` if that is the cleaner seam.
  - Add `guard_allows_reads_of_attachments_and_denies_writes` to `loom/tests/plan_session_sandbox_contracts.rs` beside `guard_denies_reads_of_the_runtime_lock_and_allows_drafts`: a planned addition, nothing else in the file changes.
  - Add the real-sandbox probe `an_attachment_is_readable_and_not_writable` to `loom/tests/plan_session_confinement.rs`. It skips in a stage like the file's other probes; the operator runs it.
- Wire shapes (C1 builds the browser side to exactly these; repeat them in your report):
  - input: `{"kind":"file","id":"0123456789abcdef","name":"sketch.png"}`;
  - transcript chip: the existing chip object with `"kind":"file"`, `"path":""` and `"file":{"id":…,"name":…,"media_type":…,"bytes":…}`.
  - `web/src/plan/api/schema-chat.ts` and the web fixtures `chat-commands.json` and `chat-deltas.json` are C1's. C1 adds a `file` input example and an `item_added` user item carrying one file chip, which your `wire/tests_chat.rs` parity tests read.

## Security (reviewer (a) checks these)

- The gate refuses 415 and 413 before reading the body.
- Magic-byte and UTF-8 checks; a mismatch is 422.
- The display name never reaches a path; the id grammar is checked before any path is built.
- Directory 0700, files 0600, `O_EXCL|O_NOFOLLOW` temp writes, rename into place.
- The 256 MiB store cap.
- The sandbox grant is read-only, with no write grant.
- The client never sends a path, and browsers never receive the absolute path.

## Tests

- The frozen stage contract `upload-stores-by-content-and-refuses-a-lie`.
- `an_upload_before_any_start_takes_authoring` in `loom/src/plan_server/tests/` (real server, as the stage contract is served, a plan `after_stage` greps the name): with NO `/api/test/take`, a first browser's PNG upload answers 200 and that browser then holds authoring; a second browser's upload answers 403; a server in read-only mode answers the first upload 403.
- Unit tests for: each magic check; `touch` on a present and a missing id; the name sanitizer, with bidi, `../`, a `"` and an empty name; the 413 and 415 refusals in `gate_tests.rs`; the 507 store cap; the unknown-id refusal in `resolve`; the format/parse round trip; the 30-day cleanup with a fixed clock; a re-upload refreshing `stored_at`.

## Run once at the end

`cargo test --manifest-path loom/Cargo.toml --lib -- plan_server::uploads:: plan_server::gate plan_server::tests:: plan_server::wire:: plan_session::attachments:: plan_session::sandbox::` (one command; filters after `--`). The integration targets run in the main agent's stage gate.
