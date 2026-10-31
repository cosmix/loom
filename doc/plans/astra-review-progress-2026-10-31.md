# Astra review repair progress

Source: [review](../astra-code-review-2026-10-31.md). Consult this ledger before assigning work. Implemented means the code and focused regression are present; integrated verification is pending. Partial means further acceptance work remains. All findings not listed below remain open and unassigned unless noted as active.

Verification: the library suite compiled and ran 10,959 tests: 10,948 passed, six failed, and five were ignored. All six failures passed focused reruns after fixture and expectation corrections. The full suite has not been repeated on the corrected tree.

| Finding | Status |
| --- | --- |
| RT-01 | Implemented; verification pending |
| RT-02, RT-03 | Done — resolver race and merge acceptance passed |
| RT-12 | Done — filesystem and genuine restore regressions passed |
| RT-14 | Done — filesystem boundary comments corrected |
| RT-15 | Done — filesystem work-directory suite passed |
| RT-16 | Done — stage-service, socket and spool regressions passed |
| RT-17 | Done — filesystem and memory relay tests passed |
| RT-13 | Done — filesystem suite passed; replacement sync failure handled |
| RT-18 | Done — ledger and repeated-restart tests passed |
| RT-19 | Done — 14 completion-commit tests passed |
| RT-20 | Done — compatibility facade deprecated; 16 tests passed |
| RT-04 | Done — merge classification and adapter regressions passed |
| RT-21 | Done — 129 permissions tests passed |
| RT-22 | Done — permission constant deduplicated and prose corrected |
| INT-01 | Done — five placement tests passed |
| INT-02 | Done — wrapper and Rust preflight tests passed |
| INT-03 | Done — installation process ownership tests passed |
| INT-04 | Partial — deprecated compatibility replica retained; shell case parity incomplete |
| RT-27, RT-28 | Implemented; verification pending |
| VER-01 | Partial — exact selectors implemented; PHPUnit, Pest and Swift runner behavior unverified |
| VER-02 | Done — policy binding, budget and expiry regressions passed |
| VER-04 | Implemented; verification pending |
| VER-05 | Done — scoped comment and diagnostic corrections |
| VER-06 | Done — Markdown and frontmatter tests passed |
| VER-03 | Done — test compilation, preserved v2 and freeze suites passed |
| VER-R01, VER-R02, VER-R03 | Implemented; verification pending |
| CTX-01, CTX-02, CTX-03 | Done — 1,018 context tests passed; one ignored |
| CTX-04 | Done — language and project suites passed |
| DASH-01 | Done — browser keyboard regression passed |
| DASH-02 | Done — handshake and shell focused tests passed |
| DASH-03 | Done — graph tests and web typecheck passed |
| DASH-04 | Done — remote bind and startup diagnostics tests passed |
| DASH-05 | Done — web tests, typecheck and legacy Rust regression passed |
| DASH-06 | Partial — bounded body reads verified; in-flight configuration commit race remains |
| DASH-07 | Done — theme preview, web tests and typecheck passed |
| DASH-08 | Done — focused web tests passed |
| INT-E01 | Done — updater installation tests passed |
| INT-E02 | Done — HTTPS-only client regression passed |
| INT-E03 | Done — live terminal shutdown regression passed |
| INT-E04 | Done — configuration comments corrected |
| INT-E05 | Done — direct-mode signal and lifecycle tests passed |
| INT-E06 | Done — runtime configuration projection matrix passed |
| RBR-01 | Done — wait, forwarding, classifier tests and production check passed |
| RBR-02 | Done — parent discovery tests and production check passed |
| RBR-03, RBR-09 | Implemented; verification pending |
| CON-B-01 | Partial — guardian recovery verified; legacy bare-PGID records fail closed and may leave criteria running |
| CON-B-02 | Partial — a freeze admission I/O error after the spool pre-read can still become a refusal |
| CON-B-03, CON-B-04, CON-B-05, CON-B-06, CON-B-07 | Implemented; verification pending |
| CON-B-10, CON-B-11, CON-B-12, CON-B-13, CON-B-15 | Implemented; verification pending |

Additional scoped work: provision-option validation, NUL-delimited worktree listing, and four Git comment corrections are implemented; verification is pending. CON-B-01 retains a legacy bare-PGID cleanup limitation. VER-01 has runner-specific verification outstanding.
