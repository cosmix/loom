# Checkout Identity

> Persisted checkout identity: pin path and inode, never st_dev

## The Persisted Checkout Identity Pinned st_dev and Broke After a Reboot

**What happened:** after a macOS reboot every command that loads `main-checkout-identity.json` failed with `checkout identity directory changed: <repo>`: `loom map`, the knowledge brief's source graph, and `loom plan verify --dry-run`. The checkout and `.git` inodes were unchanged; only the device number differed (16777233 at capture, 16777232 after the reboot).

**Why:** `git/worktree/main_identity.rs::Directory` pinned `(path, st_dev, st_ino)` in a file that outlives the process. `st_dev` is assigned when a volume is mounted, so an APFS volume can get a different one on every boot.

**Prevention:** a dev/inode identity compared within one process (lock files, tty markers, cache fingerprints) may keep `st_dev`. One written to disk and read by a later process pins the canonical path and inode only. A directory replaced at the same path gets a new inode, and reusing the old inode from another volume needs a mount at that path.

**Fix:** `Directory` stores `path` and `inode`; `identity_survives_a_device_number_change` (`main_identity_tests.rs`) rewrites the recorded device numbers and requires the identity to still verify. An anchor written by an older build still loads, because serde ignores its `device` field.
