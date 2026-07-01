---
name: loom-dependency-scan
description: Scan project dependencies for CVEs, outdated packages, and license compliance across npm, pip, cargo, go, maven, and other ecosystems. Use for vulnerability scanning, SBOM generation, supply chain analysis, and automated dependency updates.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Bash
triggers:
  - dependency scan
  - vulnerability
  - CVE
  - Snyk
  - Dependabot
  - Renovate
  - npm audit
  - cargo audit
  - pip-audit
  - safety
  - outdated packages
  - SBOM
  - software bill of materials
  - license compliance
  - supply chain
  - dependency confusion
  - typosquat
  - security advisory
  - transitive dependency
  - lock file
---

# Dependency Scan

CVEs, outdated packages, license compliance, and supply-chain risk across ecosystems. Deep-dependency companion to `loom-security-scan` (fast pre-commit/CI scanning) and `loom-security-audit` (methodology/compliance).

## Workflow

1. **Enumerate** — parse manifests + lockfiles; separate direct vs transitive. No lockfile → builds aren't reproducible (fix first).
2. **Scan** — CVEs against advisory DBs (below); note severity, affected/fixed versions, and the dependency *path*.
3. **Assess reachability** — a CVE in an unimported/dev-only path is lower priority than one on a hot code path. `govulncheck` and Snyk reason about reachability; `npm audit` does not.
4. **Remediate** — minimal safe bump to the fixed version; prefer patch/minor; verify tests. Pin the result in the lockfile.

## Scanning Commands

```bash
# JS      npm audit --audit-level=high   |  osv-scanner -r .
# Python  pip-audit                       |  safety check
# Rust    cargo audit                     |  cargo deny check advisories
# Go      govulncheck ./...               |  go list -m all | nancy sleuth
# Ruby    bundle audit --update
# Java    mvn org.owasp:dependency-check-maven:check
# .NET    dotnet list package --vulnerable --include-transitive
# PHP     composer audit
# Any     osv-scanner -r .   (lockfile-driven, OSV DB, all major ecosystems in one)
```

⚠ `--vulnerable`/audit tools only see what the **lockfile** pins — an unpinned range (`^1.2.0`) may resolve differently in CI. Scan the committed lockfile, and regenerate it before scanning if manifests changed.

## Supply-Chain Attacks (the high-signal risks)

Registry malware now outpaces classic CVEs. Defenses are structural, not just "run audit":

- **Dependency confusion** — you have an internal package `@acme/utils`; an attacker publishes `@acme/utils` to the *public* registry with a higher version. A misconfigured resolver prefers the public one and runs attacker code at install. Defend: scope internal packages to your private registry, configure the client to **never** fall back to public for those scopes (npm `.npmrc` `@acme:registry=…`, pip `--index-url` not `--extra-index-url`), and claim/reserve your names on the public registry.
- **Typosquatting** — `reqeusts`, `lodahs`, `python-dateutil` vs `dateutil`. Review every *new* dependency name character-by-character; watch install-time scripts. Prefer well-known packages with history and many maintainers.
- **Namespace / maintainer hijack** — a legit package gets a malicious release after an account takeover or a maintainer handoff. Defend: pin exact versions + lockfile hash integrity; delay auto-adopting brand-new releases (Renovate `stabilityDays`); watch for sudden maintainer changes.
- **Malicious install scripts** — npm `postinstall`, pip `setup.py` run arbitrary code at install. Use `npm ci --ignore-scripts` where feasible; audit scripts of new deps.
- **Integrity** — commit lockfiles with hashes (`package-lock.json`, `Cargo.lock`, `poetry.lock`, `pip-compile --generate-hashes`). Hashes turn a hijacked re-publish of an existing version into a hard install failure.

## SBOM

Machine-readable inventory of every component — the prerequisite for "are we affected by CVE-X?" incident response and license tracking.

```bash
syft . -o spdx-json > sbom.spdx.json                 # universal (files + many ecosystems)
syft . -o cyclonedx-json > sbom.cdx.json             # CycloneDX flavor
trivy image --format spdx-json myimage:tag           # container image contents
cargo cyclonedx  |  cyclonedx-npm  |  cyclonedx-py   # ecosystem-native generators
```

Generate in CI per release and store it; then `grype sbom:./sbom.cdx.json` re-checks an existing SBOM against today's advisory DB without rebuilding.

## License Compliance

```bash
npx license-checker --onlyAllow 'MIT;Apache-2.0;BSD-2-Clause;BSD-3-Clause;ISC'
cargo deny check licenses      #  pip-licenses  |  scancode-toolkit (deep)
```

| Category | Examples | Risk |
| -------- | -------- | ---- |
| Permissive | MIT, Apache-2.0, BSD, ISC | Safe; Apache-2.0 adds patent grant |
| Weak copyleft | MPL-2.0, LGPL | OK if dynamically linked / file-level; check linking |
| Strong copyleft | GPL, **AGPL** | May force source disclosure — AGPL triggers on network use (SaaS) |
| Unknown / missing | — | Block until resolved; unlicensed = all-rights-reserved |

## Automated Updates (Renovate)

```json
{
  "extends": ["config:recommended", ":semanticCommits"],
  "vulnerabilityAlerts": { "labels": ["security"], "enabled": true },
  "packageRules": [
    { "matchUpdateTypes": ["major"], "automerge": false },
    { "matchUpdateTypes": ["minor", "patch"], "matchCurrentVersion": "!/^0/", "automerge": true },
    { "matchDepTypes": ["devDependencies"], "automerge": true, "groupName": "dev deps" }
  ],
  "minimumReleaseAge": "3 days",
  "prConcurrentLimit": 5
}
```

⚠ `minimumReleaseAge`/`stabilityDays` deliberately delays adopting fresh releases — the window in which hijacked/malicious versions are typically caught and yanked. Never auto-merge majors; `^0.x` is pre-1.0 where minors can break.

## Verification Checklist

- [ ] Lockfile present, committed, with integrity hashes; scan targets the lockfile (not just manifest ranges)
- [ ] Every ecosystem in the repo scanned; transitive deps included
- [ ] Findings ranked by **reachability + severity**, not raw CVSS; each has a fixed-version upgrade path
- [ ] Internal packages scoped to a private registry with **no public fallback** (dependency-confusion closed)
- [ ] New dependencies eyeballed for typosquats and reviewed for install scripts
- [ ] SBOM generated for the release and stored
- [ ] Licenses checked against an allowlist; no AGPL/GPL surprises for a proprietary/SaaS product
- [ ] Auto-update bot enabled with a release-age delay and no major auto-merge

For pre-commit/CI wiring see `loom-security-scan`; for exploit-context risk rating and compliance framing see `loom-security-audit`.
