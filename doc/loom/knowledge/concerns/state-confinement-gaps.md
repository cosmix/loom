# State Confinement Gaps

> Shared package caches

## Open Gap (2026-09-13)

One accepted risk remains open; no plan addresses it:

- **Shared package caches** stay session-writable and are executed by the operator's own builds.
  Every capsule's `allowWrite` carries `sandbox/package_caches.rs::PACKAGE_MANAGER_CACHE_WRITE_PATHS`:
  cargo's `registry`, `git` and lock files, rustup's `downloads`, `tmp` and `update-hashes`, and the
  bun, npm, pnpm, yarn, deno, uv, pip and go caches. Cargo does not re-verify extracted sources, so
  an edited `build.rs` under `~/.cargo/registry/src/<registry>/<crate>-<version>/` runs at the
  operator's next host build of any project using that crate; the same holds for any cache a host
  tool reads without re-hashing it (the go build cache, uv's `archive-v0`). A full copy per
  session is not an option: on the machine where this was measured the cargo registry held
  1.75 GB, bun 30 GB, npm 7.7 GB and uv 62 GB, and ext4 has no reflinks.
