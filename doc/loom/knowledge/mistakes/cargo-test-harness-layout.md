# Cargo Test Harness Layout

> Test harness binary lookup missed cargo 1.100 build/<hash>/out layout

## A Harness-Relative Binary Lookup Missed the Cargo 1.100 Test Layout

**What happened:** the six macOS native-worker boundary kernel tests looked for the `loom` they start as the Seatbelt bridge at `current_exe().parent().parent()/loom`. Cargo 1.100 puts a lib test harness at `target/<profile>/build/loom/<hash>/out/loom-<hash>`, so the lookup never found `target/debug/loom` and the tests always skipped. The nightly strict lane, which also built the binary only after the tests, failed all six. Because they had never run, nobody saw that the proxy-tunnel test failed on a real bug: `proxy/tunnel.rs::splice` dropped replies after a client half-close (`mistakes/native-worker-tunnel-half-close.md`).

**Why:** the lookup hard-coded the old `target/<profile>/deps/` depth, and a skip, which non-strict runs only print, reads like a pass.

**Prevention:** find cargo outputs from a test harness by walking its ancestors, never at a fixed depth. Anything a strict lane gates on must be built before that lane runs. A new kernel test counts as proven only once a strict run has executed it.

**Fix:** `native_worker/boundary/loom_program.rs::built_loom` takes the nearest of up to five ancestors that holds a `loom` file. `.github/workflows/nightly.yml` builds `--bin loom` before `Run tests` in the macOS lib suite.
