# Brief: seatbelt-grants (macOS completion sandbox)

Tier: `loom-senior-software-engineer` (opus, effort medium). Security-sensitive. Read `doc/plans/briefs/open-github-issues/common.md` first.

## Owns

`loom/src/verify/criteria/confine_seatbelt.rs`, `loom/src/verify/criteria/confine_seatbelt_tests.rs`, `loom/src/verify/criteria/confine_ancestors.rs`, `loom/src/verify/criteria/confine_host_tests.rs`.

Frozen, read-only: `loom/src/verify/criteria/seatbelt_grant_contract_tests.rs` (reads the profile through `crate::verify::criteria::confined_policy`, `confine_policy.rs:20-24`).

## Facts (verified)

- `platform_command` (`confine_seatbelt.rs:256-283`) builds `(version 1)(deny default)(allow process*)(allow sysctl-read)(allow mach-lookup)(allow file-read-metadata)(allow file-read* (literal "/"))`, then path rules (`seatbelt_path_rules` 39-79), named rules (84-114), `provision_network_rules` (171-191) in Provision mode only. There is no signal rule and no TCP bind rule.
- `confine_ancestors.rs`: `SEARCHED_ABOVE` (`Cargo.toml`, `.cargo/config.toml`, :27), `hidden_above` (32-51), `seatbelt_rules` (57-67) emitting `(deny file-read* (literal P))(deny file-read-metadata file-test-existence (literal P))`, `seatbelt_spellings` for `/var` and `/private/var`. The module docs (1-22) explain why a metadata allow is outranked only by literal denies.
- Symptoms in issue #40: `bun run` aborts `CouldntReadCurrentDirectory` (it reads the directories above the cwd); `child.kill("SIGKILL")` gets EPERM; a listener on port 0 fails. Linux bubblewrap already permits all three (pid namespace, loopback in its netns, empty parent dirs).

## Change

1. Profile, every network mode: `(allow signal (target same-sandbox))` and `(allow network-bind network-inbound (local ip "localhost:*"))`. Denied mode keeps every outbound denied; check that a loopback connect from a test to its own listener needs `network-outbound (remote ip "localhost:*")` too, and if it does, add exactly that filter and nothing wider. Never `(allow signal)` unqualified, never `(allow network*)`.
2. Ancestors: for every ancestor directory of the tree except `/`, in each `seatbelt_spellings` spelling, `(allow file-read* (literal "<dir>"))` (a literal on a directory lets it be listed and opened, never its files). Emit them before the Cargo-file denies so the denies keep winning for those files. Update the module docs.
3. Comments state the security reason for each rule (what a wider rule would expose: the daemon and other same-user processes for signals; every interface for bind; files under the ancestors for a subpath).
4. Kernel tests in `confine_seatbelt_tests.rs`, guarded by the existing confined-commands probe (`process::sandbox_probe`): kill of an own child succeeds; a listener on `127.0.0.1:0` accepts a connect from the same command; `ls ..` from the tree succeeds while `cat ../../Cargo.toml` fails; a signal to the parent shell's pid outside the sandbox fails. Name each test with `seatbelt_` so the operator's plain-shell run (`-- seatbelt`) selects them. They skip inside a stage session and in the completion host job (nested sandbox, exit 71).
5. Linux parity check in `confine_host_tests.rs`: a confined run binds and connects on loopback (guarded by the bwrap probe; skips on macOS).

## Done

`cargo test --manifest-path loom/Cargo.toml --lib verify::criteria::` passes with the frozen contracts; clippy clean. Record with `loom memory note` that the kernel tests skip in the stage and the operator runs them.

## Traps

- `confine.rs:109` revision `host-os-boundary-v9:{args:?}` changes with the profile, invalidating the pass cache: intended.
- `the_profile_denies_every_read_of_the_cargo_files_after_allowing_metadata` (`confine_seatbelt_tests.rs:66`) must keep passing.
