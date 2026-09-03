---
name: loom-skills
description: Loads a catalogued loom domain skill on demand; the argument is the full skill name, e.g. loom-rust, or omit it to browse the catalog table below.
---

# Loom Skills Catalog Loader

## Overview

Nine loom mechanics skills stay installed into `~/.codex/skills/`, indexed by
Codex with their `description:` kept resident in every request. The other
fifty-three domain skills install instead into
`~/.codex/loom-skill-catalog/<name>/SKILL.md` — a directory Codex does not scan,
so they cost nothing resident. This skill is the bridge: it loads one of those
catalogued skills on demand.

## How to Load

Invoked with an argument, read `~/.codex/loom-skill-catalog/<argument>/SKILL.md`
in full with `cat` or, for a long file, with
`sed -n '<first>,<last>p'`, then follow it as if it had been loaded directly.
The argument is the full skill name, for example `loom-rust`.

**Several names may be passed at once**, separated by whitespace or commas, for
example `loom-ci-cd loom-rust`. Split the argument and load EACH name by the
rules below, in the order given. Never treat the whole string as one skill
name — `~/.codex/loom-skill-catalog/loom-ci-cd loom-rust/SKILL.md` is not a
path that can exist, and reporting it as a missing catalog is wrong.

If a name's path does not exist, try `~/.codex/skills/<name>/SKILL.md` — an
install made with `--skills all` keeps every skill in the indexed directory
and has no catalog directory at all.

If neither path exists for a name, say which name did not resolve and continue
with the ones that did. Report the catalog as not installed ONLY when
`~/.codex/loom-skill-catalog/` is itself absent AND the name is missing from
`~/.codex/skills/` — a single unresolved name means a bad name, not a broken
install.

Invoked with no argument, show the table below and pick the skill that fits the
task.

## Catalog

| Skill | Summary |
| --- | --- |
| `loom-accessibility` | Web accessibility, WCAG compliance, inclusive design |
| `loom-api-design` | Designs REST/GraphQL/RPC APIs for consistency and scale |
| `loom-api-documentation` | OpenAPI/Swagger docs, auth flows, SDK guides |
| `loom-argocd` | GitOps CD for Kubernetes with Argo CD |
| `loom-auth` | OAuth2/JWT/RBAC auth patterns, sessions, MFA |
| `loom-background-jobs` | Job queues, scheduled jobs, worker pools, retries |
| `loom-caching` | Caching strategies: cache-aside, TTL, stampede prevention |
| `loom-ci-cd` | CI/CD pipelines across GitHub Actions, GitLab, Jenkins |
| `loom-code-migration` | Safe code migrations, framework upgrades, codemods |
| `loom-concurrency` | Concurrency/async patterns across Rust, Python, TS, Go |
| `loom-crossplane` | Crossplane infra-as-code on Kubernetes APIs |
| `loom-database-design` | Schema/data model design: relational, NoSQL, warehouse |
| `loom-data-validation` | Schema validation, input sanitization, output encoding |
| `loom-data-visualization` | Charts, dashboards, and reports across domains |
| `loom-dependency-scan` | Scans dependencies for CVEs, license issues, SBOM |
| `loom-diagramming` | Mermaid diagrams: architecture, sequence, ERD, C4 |
| `loom-docker` | Dockerfiles, compose, multi-stage builds, hardening |
| `loom-documentation` | READMEs, architecture docs, changelogs, ADRs |
| `loom-e2e-testing` | E2E testing with Playwright, Cypress, Selenium |
| `loom-error-handling` | Error handling: Result/Option, retries, circuit breakers |
| `loom-event-driven` | Event-driven: queues, pub/sub, event sourcing, sagas |
| `loom-feature-flags` | Feature flags: rollouts, A/B tests, kill switches |
| `loom-fluxcd` | GitOps CD for Kubernetes with Flux CD |
| `loom-git-workflow` | Git branching, commits, merges, conflict resolution |
| `loom-golang` | Idiomatic Go: goroutines, channels, testing, modules |
| `loom-grafana` | Grafana dashboards, panels, LogQL/TraceQL queries |
| `loom-i18n` | i18n/l10n: translations, locale formatting, RTL |
| `loom-istio` | Istio service mesh: mTLS, routing, canary, Envoy |
| `loom-karpenter` | Karpenter node autoscaling and cost optimization |
| `loom-kubernetes` | K8s manifests, Helm, RBAC, operators, troubleshooting |
| `loom-kustomize` | Kustomize overlays, patches, ConfigMap/Secret gen |
| `loom-logging-observability` | Structured logging, tracing, metrics, alerting |
| `loom-model-evaluation` | ML model evaluation: metrics, CV, drift monitoring |
| `loom-performance-testing` | Load/perf testing with k6, locust, JMeter, Gatling |
| `loom-prometheus` | PromQL, scrape configs, alerting/recording rules |
| `loom-prompt-engineering` | LLM prompt design: few-shot, security |
| `loom-python` | Idiomatic Python: FastAPI, Django, pandas, pytest |
| `loom-rate-limiting` | Rate limiting: token/leaky bucket, quotas, Redis |
| `loom-react` | React 19+ SPAs: hooks, Jotai, routing, Bun/Vite/Oxc |
| `loom-refactoring` | Restructures code without changing behavior |
| `loom-rust` | Idiomatic Rust: ownership, async/tokio, cargo, serde |
| `loom-search` | Full-text search: Elasticsearch, OpenSearch, Meilisearch |
| `loom-security-audit` | Deep security audits: OWASP, compliance, hardening |
| `loom-security-scan` | Quick security checks: secrets, deps, containers |
| `loom-serialization` | Serialization: JSON, YAML, protobuf, schema evolution |
| `loom-sql-optimization` | SQL query optimization, indexing, EXPLAIN analysis |
| `loom-technical-writing` | Technical writing: READMEs, guides, changelogs |
| `loom-terraform` | Terraform/OpenTofu IaC, modules, state, workspaces |
| `loom-testing` | Test implementation: unit, integration, e2e, TDD/BDD |
| `loom-test-strategy` | Test strategy: pyramid, coverage, flaky diagnosis |
| `loom-threat-model` | Threat modeling: STRIDE, DREAD, PASTA, attack trees |
| `loom-typescript` | Type-safe TypeScript: generics, strict mode, zod/trpc |
| `loom-webhooks` | Webhooks: HMAC verification, retries, idempotency |

## Note

The catalogued file is the complete skill, not a summary of it — reading it
is equivalent to having loaded the skill directly.
