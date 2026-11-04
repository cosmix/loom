# H2 — remove the SSH tunnel from Share (issue 14)

Tier: haiku (`loom-mechanical-engineer`). Read `common.md` first, D16 in particular. Every decision is settled here; report anything that does not match.

## Edits

1. `loom/src/plan_server/wire/server.rs`:
   - delete the field `pub tunnel: Option<String>,` from `ShareLink`;
   - in the test `structs_reject_unknown_fields`, delete `"tunnel": null, ` from the ShareLink JSON line (`"url": "http://localhost", "bind": "loopback", "tunnel": null, "unexpected": true` becomes `"url": "http://localhost", "bind": "loopback", "unexpected": true`). The plan's `after_stage` fails on any `tunnel` left in this file.
2. `loom/src/plan_server/server_info/mod.rs`:
   - delete the `tunnel: (!bind.remote).then(|| tunnel_hint(…)),` line (about line 46);
   - delete the `use super::startup::tunnel_hint;` import (line 6) if nothing else in the file uses it;
   - fix the doc comment on line 35 to drop "A loopback bind adds the SSH tunnel command for another machine".
   - Do NOT delete `tunnel_hint` in `startup.rs`: `plan_server/run/dashboard.rs` still uses it for the dashboard link.
3. Rust tests:
   - in `loom/src/plan_server/server_info/tests.rs` and `loom/src/plan_server/tests/mod.rs`, remove the `"tunnel"` keys from the expected JSON;
   - in `tests/mod.rs`, delete the lines that read the tunnel command (`.as_str().expect("a tunnel command")` and the assertions on it), and replace `link["tunnel"].is_null()` with `assert!(link.get("tunnel").is_none())`;
   - rename `share_on_loopback_returns_tokenized_url_and_tunnel` to `share_on_loopback_returns_tokenized_url`, and `share_on_loopback_gives_the_tokenized_url_and_a_tunnel` to `share_on_loopback_gives_the_tokenized_url`;
   - keep every other assertion.
4. `web/src/plan/api/schema.ts`: remove `tunnel` from the share link schema (search `tunnel`).
5. `web/src/plan/api/fixtures/share.json`: remove the `tunnel` key.
6. `web/src/plan/header/share.tsx` (68-88 `ShareBody`): delete the `{link.bind === "loopback" && link.tunnel && (…)}` block. Keep `CopyRow` for the URL and the remote-bind plain-HTTP warning.
7. `web/src/plan/header/share.test.tsx`: replace "shows the tunnel line on a loopback bind" with a test named "shows only the link on a loopback bind", asserting:
   - `queryByText(/tunnel/i)` is null;
   - the "Copy link" button exists;
   - there is no "Copy tunnel command" button.
   Keep the remote-bind test, and remove `tunnel: null` from its `stubShare({ … })` object: the share schema is strict, so the old key would fail to parse.

## Run once at the end

`cargo test --manifest-path loom/Cargo.toml --lib -- plan_server::server_info plan_server::tests plan_server::wire::` (one command; filters after `--`). The web share tests and the schema parity test run in the main agent's stage gate.
