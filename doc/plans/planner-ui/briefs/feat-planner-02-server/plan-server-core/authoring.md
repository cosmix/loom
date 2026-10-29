# plan-server-core / authoring: the authoring holder and presence

Tier: opus (security-sensitive state machine). Wave 1, parallel with `ws-hub` and
`runtime-startup`, after `foundation`. Read `doc/plans/briefs/feat-planner-02-server/common.md` first.

Spec to read: `doc/product/section_12.md` "Browser identity and the authoring holder",
"Display names"; `doc/product/section_02.md` "Several people, one author";
`doc/product/section_07.md` lines on the grace period ("Grace period: 10 minutes").

## Files you own

`loom/src/plan_server/authoring.rs`, `loom/src/plan_server/authoring_tests.rs`,
`loom/src/plan_server/presence.rs`, `loom/src/plan_server/presence_tests.rs`. Uses
`plan_server::ids` and `plan_server::wire::{presence, server}` (written by `foundation`, read-only).
The orchestrator has already declared every wave-1 module in `plan_server/mod.rs` (including
`mod authoring_tests;` and `mod presence_tests;` under `#[cfg(test)]`); never edit that file.
Create your files first: until a sibling worker's files exist the crate does not build, so a
build failure naming a sibling's module is not yours; wait a minute and rerun.

## authoring.rs

```text
pub const GRACE: Duration = Duration::from_secs(600);
pub enum Take { HolderOnly, TakeIfFree }
pub enum Refusal { NotHolder { holder: PublicId }, ReadOnlyMode, Stopping }
    // the integrator's PlanContext::require_authoring turns these into the section 08 wording,
    // e.g. "Another browser holds authoring (Dana)", "loom plan is in read-only mode",
    // "loom plan is stopping"
pub enum FreeReason { GraceExpired, Released(&'static str) }
pub enum AuthoringEvent { Taken { public_id: PublicId }, Away { since: SystemTime }, Back, Freed { reason: FreeReason } }
pub struct Authoring { .. }   // Mutex inside; Send + Sync
impl Authoring {
    pub fn new(read_only: bool) -> Self;
    pub fn require(&self, browser: &BrowserId, public_id: &PublicId, policy: Take, stopping: bool) -> Result<(), Refusal>;
    pub fn is_holder(&self, browser: &BrowserId) -> bool;
    pub fn holder(&self) -> Option<(PublicId, Option<SystemTime>)>;   // away_since
    pub fn view(&self, names: &Presence) -> AuthoringView;
    pub fn release(&self, reason: FreeReason);
    pub fn release_if(&self, holder: &PublicId, reason: FreeReason) -> bool;   // frees only when that browser still holds
    pub fn subscribe(&self, f: Box<dyn Fn(&AuthoringEvent) + Send + Sync>);
    pub fn connection_opened(&self, browser: &BrowserId);
    pub fn connection_closed(&self, browser: &BrowserId, now: SystemTime);
    pub fn tick(&self, now: SystemTime);   // frees at away_since + GRACE
}
```

Rules:

- The holder is a `BrowserId` (the HttpOnly cookie value), never a public id or a name. Every
  tab of the holding browser holds authoring.
- `require(.., TakeIfFree, ..)`: refused with `ReadOnlyMode` when constructed read-only, with
  `Stopping` when `stopping`; when nobody holds, records `browser` and fires `Taken`; when another
  browser holds (away or not), `NotHolder { holder: <its public id> }`.
  `HolderOnly`: Ok only for the holder; never takes.
- Away: the holder's open-connection count reaching 0 records `away_since = now` and fires `Away`;
  a connection of the holder opening while away clears it and fires `Back`. A non-holder browser
  can never clear it.
- `tick(now)`: when `away_since + GRACE <= now`, clear the holder and fire
  `Freed { GraceExpired }`. Time is passed in, never read inside, so tests drive it.
- `release` clears the holder and fires `Freed { reason }`; no-op when nobody holds.
  `release_if(holder, reason)` does the same only when the current holder's public id is `holder`,
  and returns whether it freed. Plans 7 and 11 record the public id that started their session or
  run and call `release_if` with it, so a browser that took authoring later is never freed.
- Fire subscriber callbacks after dropping the internal lock (a callback may call `view`).

## presence.rs

```text
pub struct Presence { .. }   // Mutex inside
impl Presence {
    pub fn new() -> Self;
    pub fn public_id(&self, browser: &BrowserId) -> PublicId;      // stable per browser for the process
    pub fn set_name(&self, browser: &BrowserId, raw: &str);
    pub fn name_of(&self, browser: &BrowserId) -> String;          // "Viewer N" when unnamed or duplicate
    pub fn set_reading(&self, conn: ConnId, path: Option<String>);
    pub fn opened(&self, conn: &ConnCtx);
    pub fn closed(&self, conn: ConnId);
    pub fn list(&self, authoring: &Authoring) -> PresenceList;      // browsers with >= 1 tab or the holder while away
}
pub fn sanitize_name(raw: &str) -> String;
```

- `sanitize_name`: drop every char in Unicode general category Cc or Cf (covers the bidi
  controls U+202A..U+202E, U+2066..U+2069, U+200E, U+200F, U+061C), trim, then keep at most 32
  `char`s (never slice bytes). There is no Unicode-category crate in `loom/Cargo.toml`: match the
  Cf code points explicitly (list them in one const with a comment naming the Unicode table) and
  `char::is_control` for Cc.
- "Viewer N": N is assigned once per browser in first-seen order, starting at 1, and used when
  the sanitized name is empty or equals another connected browser's name (the later one shows
  as "Viewer N").
- `reading` is repo-relative (`doc/...`, the wire path rule): it must start with `doc/` and the
  rest must pass `planning::paths::check_relpath_syntax` (plan 1); otherwise store `None`. Test both.

## Tests (in the two `_tests.rs` files)

Holder takes; second browser refused while held and while away; grace frees at exactly
600 s and not at 599 s; a second tab of the holder closing does not mark away while another tab
is open; a holder tab reopening inside grace fires `Back`; read-only refuses `TakeIfFree`;
`HolderOnly` never takes; `release_if(A)` after grace expiry freed A and browser B took authoring
leaves B holding and returns false, while `release_if(B)` frees B; `sanitize_name` on `"\u{202E}Dana\u{0007}"`, a 40-char name of
multi-byte chars (exactly 32 chars kept), an all-control name (empty -> "Viewer N"); duplicate
names.

## Done when

`cargo test --manifest-path loom/Cargo.toml --lib plan_server::authoring` and
`plan_server::presence` pass. Report any signature you had to change, with the reason.
