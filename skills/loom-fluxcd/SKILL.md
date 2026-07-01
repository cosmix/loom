---
name: loom-fluxcd
description: GitOps continuous delivery toolkit for Kubernetes with Flux CD. Use for declarative deployments, Helm chart automation, Kustomize overlays, image update automation, multi-tenancy, and Git-based continuous delivery.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - flux
  - fluxcd
  - gitops
  - kustomization
  - helmrelease
  - gitrepository
  - helmrepository
  - imagerepository
  - imagepolicy
  - image automation
  - source controller
  - continuous delivery
  - kubernetes deployment automation
  - helm automation
  - kustomize automation
  - git sync
  - declarative deployment
---

# Flux CD GitOps Toolkit

## Overview

Declarative GitOps CD: a set of specialized controllers continuously reconcile cluster state toward Git. Flux is CRD-centric with no first-party UI — you drive it with the `flux` CLI and YAML. The controllers, and the CRDs each owns:

| Controller | CRDs | Role |
| --- | --- | --- |
| source-controller | `GitRepository`, `OCIRepository`, `HelmRepository`, `HelmChart`, `Bucket` | Fetch + verify + cache artifacts |
| kustomize-controller | `Kustomization` | Build/apply overlays, prune, health-check, `dependsOn` ordering |
| helm-controller | `HelmRelease` | Install/upgrade/rollback charts, drift detection |
| notification-controller | `Provider`, `Alert`, `Receiver` | Outbound alerts + inbound webhooks |
| image-reflector / image-automation | `ImageRepository`, `ImagePolicy`, `ImageUpdateAutomation` | Scan registries, select tags, commit back to Git |

The controller separation matters: a `Kustomization` failure is a kustomize-controller concern; a chart failure is helm-controller. Debug the right one.

## When Argo CD vs Flux

Both are CNCF-graduated GitOps controllers; the choice is architectural, not feature-parity.

| Concern | Flux CD | Argo CD |
| --- | --- | --- |
| Ordering | `dependsOn` between Kustomizations/HelmReleases + `healthChecks` gate the next | `argocd.argoproj.io/sync-wave` annotations *within* one Application |
| Composition | Kustomization tree: a Kustomization applies more Kustomizations | App-of-apps: one root Application recursing into children |
| Drift correction | Continuous reconciliation always re-applies desired state; `prune: true` GCs by `.status.inventory`; HelmRelease drift detection is opt-in | Opt-in `selfHeal` reverts drift; `prune` deletes Git-removed resources |
| Fan-out | No native generator; per-tenant Kustomizations + image automation | ApplicationSet generators (cluster/git/matrix/PR/SCM) |
| Image updates | First-class ImageRepository/ImagePolicy/ImageUpdateAutomation, commits back to Git | Not built-in (separate Argo CD Image Updater) |
| Interface | CLI/CRD-centric (`flux` CLI, no first-party UI) | Web UI-centric (topology, manual sync buttons) |
| Multi-cluster | Typically one Flux per cluster pulling its own path | One control plane syncs many clusters |

Rule of thumb: **Flux** for a lean controller set, Git-native image automation, and dependency ordering expressed as CRDs; **Argo CD** when operators want a visual sync/health console and generator-driven multi-cluster fan-out. They coexist.

## Install & Bootstrap

```bash
curl -s https://fluxcd.io/install.sh | sudo bash      # or: brew install fluxcd/tap/flux
flux --version && flux check --pre                      # cluster preflight

export GITHUB_TOKEN=<token>
flux bootstrap github \
  --owner=<user> --repository=<repo> --branch=main \
  --path=clusters/production --personal \
  --components-extra=image-reflector-controller,image-automation-controller \
  --read-write-key            # REQUIRED if image automation must push commits (see gotcha)
# GitLab: flux bootstrap gitlab --owner=<group> ... (same flags)
```

Bootstrap (not `flux install`) writes the components into Git so they are version-controlled and self-managed — required to patch controller args via Kustomize (concurrency, lockdown flags). Validate manifests pre-commit with `kubectl apply --dry-run=server -f clusters/production/`.

## Repository Structure

```text
├── clusters/{production,staging}/
│   ├── flux-system/           # bootstrapped components (managed by Flux itself)
│   ├── infrastructure.yaml     # sources + Kustomizations for infra
│   └── apps.yaml               # sources + Kustomizations for apps
├── infrastructure/{base,overlays/{production,staging}}/   # ingress, cert-manager, ...
└── apps/{base,overlays/{production,staging}}/
```

Multi-tenant repos add `tenants/{base,overlays}/<team>/` (namespace + RBAC + `GitRepository`/`Kustomization`) referenced from `clusters/<env>/tenants/`.

## GitRepository & Kustomization

```yaml
apiVersion: source.toolkit.fluxcd.io/v1
kind: GitRepository
metadata: { name: flux-system, namespace: flux-system }
spec:
  interval: 1m0s
  ref: { branch: main }
  url: https://github.com/org/repo
  secretRef: { name: flux-system }
  ignore: |                    # optional: shrink the artifact
    /*
    !/apps/production/
```

```yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata: { name: apps, namespace: flux-system }
spec:
  interval: 10m0s
  retryInterval: 2m0s          # ⚠ defaults to interval — set independently (see gotcha)
  dependsOn: [{ name: infrastructure }]
  sourceRef: { kind: GitRepository, name: flux-system }
  path: ./apps/production
  prune: true
  wait: true                   # ⚠ health-checks ALL resources; silently ignores .healthChecks
  timeout: 5m0s
  postBuild:
    substitute: { cluster_name: production }
    substituteFrom:
      - { kind: ConfigMap, name: cluster-vars }
```

`postBuild.substitute`/`substituteFrom` replace `${var}` tokens in the built manifests (variable names must match `^[_[:alpha:]][_[:alpha:][:digit:]]*$` — hyphens/dots silently skip). The referenced ConfigMap/Secret:

```yaml
kind: ConfigMap
metadata:
  name: cluster-vars
  namespace: flux-system
  labels: { reconcile.fluxcd.io/watch: Enabled }   # ⚠ else edits ignored until next tick (see gotcha)
data: { cluster_name: production, domain: example.com }
```

## Dependency & Ordering

Flux orders reconciliation with `dependsOn` (a Kustomization/HelmRelease waits for the named object to become Ready) combined with `healthChecks`/`wait`. This is Flux's answer to Argo sync-waves and app-of-apps, expressed as a CRD graph.

```yaml
# crds (prune:false) -> cert-manager (healthCheck) -> ingress-nginx (dependsOn cert-manager)
kind: Kustomization
metadata: { name: cert-manager, namespace: flux-system }
spec:
  dependsOn: [{ name: crds }]
  path: ./infrastructure/cert-manager
  healthChecks:
    - { apiVersion: apps/v1, kind: Deployment, name: cert-manager, namespace: cert-manager }
  # ...sourceRef, interval
```

CRD Kustomizations should set `prune: false` so a transient source error never GCs your CRDs (and every CR with them). Cross-namespace `dependsOn` names the namespace: `dependsOn: [{ name: shared-ingress, namespace: flux-system }]`.

## Helm Integration

`HelmRepository` (or `OCIRepository`) provides charts; `HelmRelease` installs them. helm-controller runs real Helm (unlike Argo's `helm template`), so `helm history`/`rollback` work.

```yaml
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata: { name: my-app, namespace: apps }
spec:
  interval: 10m0s
  chart:
    spec:
      chart: my-app
      version: "1.0.x"
      sourceRef: { kind: HelmRepository, name: my-charts, namespace: flux-system }
  dependsOn:
    - { name: cert-manager, namespace: cert-manager }
  install: { remediation: { retries: 3 } }
  upgrade:
    remediation: { retries: 3, remediateLastFailure: true }   # ⚠ default flips to true when retries>0
    cleanupOnFail: true
  test: { enable: true }
  rollback: { cleanupOnFail: true, recreate: true }
  values: { replicas: 2 }
  valuesFrom:                    # ⚠ a valuesFrom entry with targetPath outranks inline values
    - { kind: ConfigMap, name: app-config, valuesKey: values.yaml }
    - { kind: Secret,    name: app-secrets, valuesKey: secrets.yaml }
```

Prefer `chartRef` + `OCIRepository` over `chart.spec` for shared/pinned/signed charts (see Expert Practices). Private `HelmRepository` uses `secretRef` to a Secret with `stringData.{username,password}`.

## Secret Management (SOPS)

Flux decrypts SOPS-encrypted manifests inline during Kustomization apply.

```bash
age-keygen -o age.agekey && age-keygen -y age.agekey     # private + public key
cat age.agekey | kubectl create secret generic sops-age \
  --namespace=flux-system --from-file=age.agekey=/dev/stdin
sops --encrypt --in-place secret.yaml                     # per .sops.yaml rules
```

```yaml
# .sops.yaml — encrypt only the data fields, per path
creation_rules:
  - path_regex: .*/production/.*\.yaml
    encrypted_regex: ^(data|stringData)$
    age: age1ql3z...   # comma-separate multiple recipients for team access
---
# Kustomization decrypts:
spec:
  decryption:
    provider: sops
    secretRef: { name: sops-age }
```

Alternatives: **External Secrets Operator** (pull from AWS SM/Vault/GCP via `SecretStore`+`ExternalSecret`) — preferred for cloud secret managers; **Sealed Secrets** — Kubernetes-native one-way encryption.

## Image Automation

Three resources form the loop: **ImageRepository** (scans a registry) → **ImagePolicy** (selects a tag) → **ImageUpdateAutomation** (commits the new tag to Git). Manifests carry a marker comment the automation rewrites.

```yaml
apiVersion: image.toolkit.fluxcd.io/v1
kind: ImageRepository
metadata: { name: my-app, namespace: flux-system }
spec:
  image: ghcr.io/org/my-app
  interval: 5m0s
  provider: aws        # ⚠ prefer workload identity over secretRef (see Security)
---
apiVersion: image.toolkit.fluxcd.io/v1
kind: ImagePolicy
metadata: { name: my-app, namespace: flux-system }
spec:
  imageRepositoryRef: { name: my-app }
  policy: { semver: { range: 1.0.x } }        # or alphabetical/numerical (below)
  filterTags:                                  # ⚠ non-matching tags are dropped, no fallback
    pattern: "^main-[a-f0-9]+-(?P<ts>[0-9]{10})$"
    extract: "$ts"
---
apiVersion: image.toolkit.fluxcd.io/v1
kind: ImageUpdateAutomation
metadata: { name: my-app, namespace: flux-system }
spec:
  interval: 1m0s
  sourceRef: { kind: GitRepository, name: flux-system }
  git:
    checkout: { ref: { branch: main } }
    push: { branch: image-updates }             # omit for direct commit; set for PR-based flow
    commit:
      author: { email: fluxcdbot@users.noreply.github.com, name: fluxcdbot }
      messageTemplate: "Automated image update [ci skip]"
  update: { path: ./apps/production, strategy: Setters }
```

```yaml
# Deployment marker the automation rewrites:
image: ghcr.io/org/my-app:1.0.0 # {"$imagepolicy": "flux-system:my-app"}
```

Policy types: `semver` (releases, `1.0.x`/`>=1.0.0`), `alphabetical` (branch tags via `filterTags`), `numerical` (build numbers). Strategy: **enable automation in dev/staging with direct commit; use `push.branch` (PR review) for production.**

## Multi-Tenancy

**RBAC alone does NOT make Flux multi-tenant.** A default install lets a tenant reference Sources/Secrets in other namespaces, pull arbitrary remote Kustomize bases, and (if it omits `serviceAccountName`) reconcile with the controller's cluster-wide identity. Three controller flags, applied as bootstrap Kustomize patches in `clusters/<env>/flux-system/`, close these vectors and are **mandatory**:

- `--no-cross-namespace-refs=true` (kustomize/helm/notification/image-* controllers) — blocks cross-namespace refs to Sources, Secrets, events.
- `--no-remote-bases=true` (kustomize-controller) — blocks fetching remote Kustomize bases over HTTPS (which bypass source verification/caching).
- `--default-service-account=default` (kustomize/helm) — resources without `spec.serviceAccountName` fall back to the powerless namespace `default` SA instead of the controller identity.

```yaml
# clusters/<env>/flux-system/kustomization.yaml — patches the bootstrapped components
patches:
  - patch: |
      - { op: add, path: /spec/template/spec/containers/0/args/-, value: --no-cross-namespace-refs=true }
    target: { kind: Deployment, name: "(kustomize-controller|helm-controller|notification-controller|image-reflector-controller|image-automation-controller)" }
  - patch: |
      - { op: add, path: /spec/template/spec/containers/0/args/-, value: --no-remote-bases=true }
    target: { kind: Deployment, name: kustomize-controller }
  - patch: |
      - { op: add, path: /spec/template/spec/containers/0/args/-, value: --default-service-account=default }
    target: { kind: Deployment, name: "(kustomize-controller|helm-controller)" }
```

With these set, every tenant Kustomization/HelmRelease MUST declare `spec.serviceAccountName`, bound via a namespace-scoped **RoleBinding** to a custom `Role` or the built-in `admin` ClusterRole — **never a ClusterRoleBinding, never `cluster-admin`.**

```yaml
# Per-tenant: Namespace + ServiceAccount + RoleBinding(admin, namespace-scoped) + GitRepository + Kustomization
kind: RoleBinding
metadata: { name: team-a-reconciler, namespace: team-a }
roleRef: { apiGroup: rbac.authorization.k8s.io, kind: ClusterRole, name: admin }   # RIGHTS SCOPED TO team-a
subjects: [{ kind: ServiceAccount, name: team-a-reconciler, namespace: team-a }]
---
kind: Kustomization
metadata: { name: team-a-apps, namespace: team-a }
spec:
  interval: 10m
  serviceAccountName: team-a-reconciler
  sourceRef: { kind: GitRepository, name: team-a-repo }
  path: ./apps
  prune: true
```

## Multi-Cluster

Hub-and-spoke: one Flux reconciles remote clusters via `kubeConfig.secretRef`, or (more common) one Flux per cluster pulling its own `clusters/<env>/` path. Per-cluster variance is expressed with `postBuild.substitute`, not branching.

```yaml
kind: Kustomization
metadata: { name: cluster-staging, namespace: flux-system }
spec:
  path: ./clusters/staging
  prune: true
  sourceRef: { kind: GitRepository, name: flux-system }
  kubeConfig: { secretRef: { name: staging-kubeconfig } }   # remote-cluster credential
```

## Notifications

`Provider` (endpoint) + `Alert` (event filter) for outbound; `Receiver` for inbound webhooks (Git push → immediate reconcile).

```yaml
apiVersion: notification.toolkit.fluxcd.io/v1beta3
kind: Provider
metadata: { name: slack, namespace: flux-system }
spec: { type: slack, channel: flux-notifications, secretRef: { name: slack-webhook-url } }
---
apiVersion: notification.toolkit.fluxcd.io/v1beta3
kind: Alert
metadata: { name: failures, namespace: flux-system }
spec:
  providerRef: { name: slack }
  eventSeverity: error
  eventSources: [{ kind: Kustomization, name: "*" }, { kind: HelmRelease, name: "*", namespace: "*" }]
  exclusionList: [".*health check failed.*"]
---
apiVersion: notification.toolkit.fluxcd.io/v1
kind: Receiver
metadata: { name: github-receiver, namespace: flux-system }
spec:
  type: github
  events: [ping, push]
  secretRef: { name: github-webhook-token }
  resources: [{ kind: GitRepository, name: flux-system }]
```

## Operations & CLI

```bash
flux get all                                   # or: flux get kustomization <name> / helmrelease -n <ns>
flux reconcile kustomization apps --with-source   # force sync incl. re-fetch
flux reconcile helmrelease my-app -n apps
flux suspend|resume kustomization apps            # pause/resume reconciliation
flux logs --level=error --all-namespaces
flux export source git --all > sources.yaml       # DR backup; also kustomization/helmrelease
flux migrate -f <path> -v <target-version>        # mechanically rewrite manifests before CRD upgrade
```

Reconciliation intervals: infra `1h`, apps `10m`, dev `1m-5m`, sources `1m-5m`. Interval is drift-detection cadence (min 60s); tune `retryInterval` separately for failure recovery.

## Troubleshooting

| Symptom | First moves |
| --- | --- |
| Kustomization stuck Progressing | `flux get kustomization <n>`; `kubectl describe kustomization <n> -n flux-system`; `kubectl logs -n flux-system deploy/kustomize-controller` |
| HelmRelease failed | `flux get helmrelease <n> -n <ns>`; `helm history <n> -n <ns>`; `kubectl logs -n flux-system deploy/helm-controller` |
| Image not updating | check ImageRepository/ImagePolicy status; logs of image-reflector + image-automation controllers; is the deploy key read-write? |
| Source failing | `flux get source git flux-system`; `kubectl logs -n flux-system deploy/source-controller`; `flux reconcile source git flux-system` |

Debug logging: patch a controller Deployment adding `--log-level=debug` to args (same JSON-patch shape as the concurrency patch below).

## Performance

Tune controller concurrency with the `--concurrent` arg (no `flux install` flag for it) as a Git-stored Kustomize patch:

```yaml
# clusters/<env>/flux-system/kustomization.yaml
patches:
  - patch: |
      - { op: add, path: /spec/template/spec/containers/0/args/-, value: --concurrent=10 }
    target: { kind: Deployment, name: "(kustomize-controller|helm-controller)" }
```

Reduce load with higher `interval` on stable resources, a higher `retryInterval`, and `GitRepository.spec.ignore` to shrink clones.

## Expert Practices: Idioms, Anti-Patterns & Gotchas

The patterns above get a cluster running; this captures the non-obvious behavior that separates a working install from a correct one. Most are **silent failures** — no error, just wrong behavior.

### Currency: stable API versions

**Use stable APIs; betas are removed in Flux 2.7+ with no compatibility shim.** After a CRD upgrade any beta `apiVersion` is rejected:

- `HelmRelease` → `helm.toolkit.fluxcd.io/v2` (stable since 2.3).
- `HelmRepository`/`HelmChart`/`OCIRepository` → `source.toolkit.fluxcd.io/v1`.
- `ImageRepository`/`ImagePolicy`/`ImageUpdateAutomation` → `image.toolkit.fluxcd.io/v1` (promoted in 2.7, Sep 2025, which removed the betas).

The v2 `HelmRelease` API dropped three fields with no in-place equivalent: `.spec.chart.spec.valuesFile` (use plural `valuesFiles`), and `postRenderers.kustomize.patchesJson6902`/`patchesStrategicMerge` (both unified into `patches`). Rewrite mechanically with `flux migrate` before upgrading controllers.

### Idioms

**Prefer `chartRef` + `OCIRepository` over `chart.spec` for shared/pinned/signed charts.** `chart.spec` creates a hidden managed `HelmChart` per HelmRelease, pinnable only by version. `chartRef` points at an existing `OCIRepository`/`HelmChart` so multiple releases share one source, supports **digest pinning** (immutable deploys) and Cosign/notation verification. Mutually exclusive with `chart.spec`; `HelmRepository type: oci` is in maintenance mode. Switching an existing release to `chartRef` is a Helm **upgrade** (not reinstall) and GCs the old HelmChart.

```yaml
kind: OCIRepository
spec:
  url: oci://ghcr.io/stefanprodan/charts/podinfo
  ref: { digest: "sha256:a0d3..." }          # immutable pin
  verify: { provider: cosign, secretRef: { name: cosign-pub } }
---
kind: HelmRelease
spec:
  chartRef: { kind: OCIRepository, name: podinfo-chart, namespace: flux-system }
```

**Set `retryInterval` independently from `interval`.** Orthogonal timers: `interval` is steady-state drift detection (min 60s), `retryInterval` is failure recovery, defaulting to `interval` when unset. An `interval: 1h` resource waits a full hour to retry a transient failure unless you lower `retryInterval`.

**Label referenced ConfigMaps/Secrets `reconcile.fluxcd.io/watch: Enabled`.** By default Flux re-reconciles only on the interval tick, so editing a ConfigMap in `postBuild.substituteFrom` or a Secret in `valuesFrom` isn't picked up until the next scheduled reconcile (possibly hours). The label (Flux 2.7) makes the controller watch and reconcile immediately.

### Gotchas (silent failures)

**`wait: true` silently ignores `healthChecks` — they are mutually exclusive.** With `wait: true` the Kustomization health-checks *all* reconciled resources and `.spec.healthChecks` is ignored — setting both gives a false sense of targeted gating. To gate on a named subset, leave `wait` unset and use `healthChecks` alone.

**`postBuild` substitution traps.** (a) Runs only if at least one `substitute`/`substituteFrom` is defined — otherwise `${var:=default}` passes through literally. (b) Var names must match `^[_[:alpha:]][_[:alpha:][:digit:]]*$` — a hyphen/dot means silent skip. (c) An undefined `${VAR}` with no default becomes an empty string, so a typo `${cluster_rgion}` silently corrupts a URL. (d) Quote numbers/booleans to avoid YAML coercion. Harden with `--feature-gates=StrictPostBuildSubstitutions=true` and validate via `flux build kustomization --strict-substitute`.

**Renaming a `prune: true` Kustomization (or moving resources between two) deletes its workloads.** Flux tracks owned resources in `.status.inventory` by name+namespace; rename the object and the whole inventory is GC'd then recreated — a momentary outage. Safe procedure: `prune: false`, reconcile, verify the renamed object is Ready and owns the resources, then re-enable prune. Per-resource opt-out: `kustomize.toolkit.fluxcd.io/prune: disabled`.

**HelmRelease drift detection is `Disabled` by default.** helm-controller does NOT correct out-of-band `kubectl` edits unless `spec.driftDetection.mode` is set — divergence is silent until the next Helm action. `warn` logs via events; `enabled` corrects via server-side dry-run apply. Companion trap: once enabled, any legitimate mutator (HPA on `/spec/replicas`, VPA, cert-manager CA) gets reverted every cycle — add `driftDetection.ignore` paths. Start with `warn` to discover them.

```yaml
spec:
  driftDetection:
    mode: enabled
    ignore:
      - { paths: ["/spec/replicas"], target: { kind: Deployment } }   # HPA-managed
```

**HelmRelease `valuesFrom` with `targetPath` has the HIGHEST precedence — above inline `spec.values`.** `valuesFrom` entries merge left-to-right, then inline `values` overwrites — BUT a `valuesFrom` entry with `targetPath` overwrites everything before it, including inline values. (Also: deleting a ConfigMap/Secret referenced in `valuesFrom` changes inputs and triggers a Helm upgrade.)

**`upgrade.remediation` defaults are asymmetric.** `install.remediation.remediateLastFailure` defaults `false`; `upgrade.remediation.remediateLastFailure` defaults `false` UNLESS `.retries > 0`, when it flips to `true` — so merely adding an upgrade retry count silently enables last-failure rollback. Be explicit, pair with `cleanupOnFail`, avoid `retries: -1` on a broken chart.

**HelmRelease release name is silently SHA-256-truncated past 53 chars.** Flux composes `[<targetNamespace>-]<HelmRelease.name>`; over Helm's 53-char DNS-label limit it becomes first-40-chars + dash + first-12 of a SHA-256 hash. `helm list`/`history` then won't show the expected name. Set `spec.releaseName` explicitly when the composed name could approach 53 chars.

**`kubectl rollout restart` on a Flux-managed resource churns.** It adds `restartedAt`; the next reconcile removes it (not in Git) and redeploys — a loop. Use the Flux field manager: `kubectl rollout restart deploy/my-app -n apps --field-manager=flux-client-side-apply`. (Any `kubectl edit` is likewise reverted — intentional drift correction.)

**`filterTags.extract` drops non-matching tags entirely — no fallback.** `pattern` selects candidate tags; `extract` supplies a derived sort value (e.g. captured timestamp) — it does not rename or fall back. A wrong regex yields zero candidates and "no latest image", not all-tags. Companion: `digestReflectionPolicy: Always` requires an `interval`; `IfNotPresent`/`Never` forbid it.

**Image automation needs a read-write deploy key; re-bootstrapping does NOT rotate it.** `flux bootstrap` creates a read-only key by default, so image-automation-controller silently fails to push without `--read-write-key`. Re-running bootstrap with the flag does NOT overwrite the existing `flux-system` Secret — delete it first, then re-bootstrap:

```bash
kubectl delete secret flux-system -n flux-system
flux bootstrap github --read-write-key ...      # Secret recreated with a write key
```

Also: `ImageUpdateAutomation` evaluates only `ImagePolicy` objects in its **own namespace** — cross-namespace policy refs are unsupported.

**The two `Kustomization` kinds are different objects.** `kustomization.kustomize.toolkit.fluxcd.io` is the Flux CR (a reconciliation unit sourcing from a GitRepository, optionally applying an overlay); `kustomization.kustomize.config.k8s.io` is the native kustomize file. The Flux CR's `spec.path` points at a directory containing the config-kind `kustomization.yaml` — it orchestrates, not replaces. Native fields (`resources`, `patches`, `configMapGenerator`) belong in the file, never the Flux CR `spec`.

### Anti-patterns

**Never bind a tenant reconciler to `cluster-admin`** (or any `ClusterRoleBinding`) — it defeats namespace isolation. Use a namespace-scoped `RoleBinding` to a custom `Role` or the built-in `admin` ClusterRole, plus the lockdown flags.

**`force: true` is a temporary escape hatch, not a setting.** It makes the controller delete-then-recreate resources when an immutable-field patch fails — bypassing Kubernetes immutability guards for EVERY managed resource. Left on, it removes protection against accidental data loss on stateful workloads. Prefer the per-resource annotation `kustomize.toolkit.fluxcd.io/force: enabled` on the one object, then remove it.

### Security

**Multi-tenancy is not enforced by default** — `--no-cross-namespace-refs`, `--no-remote-bases`, `--default-service-account` are mandatory; omitting any one leaves a privilege-escalation path RBAC alone does not close (see Multi-Tenancy).

**Ban Kustomize remote bases in production.** Bases pointing at external URLs are fetched at reconcile time over HTTPS, outside Flux's artifact pipeline: no crypto verification, no caching (refetched every cycle), no immutability, absent from source history — a supply-chain risk. Disable with `--no-remote-bases=true`; replace with a Flux `OCIRepository`/`GitRepository` pinned by digest.

**Use workload identity instead of static credential Secrets.** Flux 2.7 completed object-level Kubernetes Workload Identity for all cloud-authenticating APIs (GitRepository, OCIRepository, ImageRepository, Bucket, Kustomization, HelmRelease, Provider) on AWS (EKS IRSA), Azure (AKS WI), GCP (GKE WI). Set `.spec.provider: aws|azure|gcp` so the controller fetches short-lived OIDC tokens instead of reading a static Secret — no rotation burden, smaller blast radius.

## Decision Points

| Choice | Take A when | Take B when |
| --- | --- | --- |
| GitRepository vs HelmRepository | custom manifests / Kustomize / charts in Git | public/private Helm chart repo |
| Kustomization vs HelmRelease | raw manifests, overlays, ConfigMaps/Secrets | packaged charts with values |
| Image automation | direct commit (dev/staging) | `push.branch` PR review (prod), or disabled (manual gate) |
| Secrets | SOPS (Git-native, small teams) | ESO (cloud managers) / Sealed Secrets |
| chart source | `chartRef`+OCIRepository (shared/signed/digest-pinned) | `chart.spec` (quick, per-release) |

## Verification Checklist

Before declaring a Flux change done:

- [ ] All manifests on stable API versions (`helm/v2`, `source/v1`, `image/v1`); `flux migrate` run before any CRD upgrade.
- [ ] `retryInterval` set independently on high-`interval` resources.
- [ ] ConfigMaps/Secrets in `substituteFrom`/`valuesFrom` labeled `reconcile.fluxcd.io/watch: Enabled`.
- [ ] No Kustomization sets both `wait: true` and `healthChecks`.
- [ ] `postBuild` substitution hardened (`StrictPostBuildSubstitutions` or `--strict-substitute` in CI); numbers/bools quoted.
- [ ] CRD Kustomizations use `prune: false`; renames done with prune temporarily off.
- [ ] Multi-tenant clusters have all three lockdown flags; every tenant resource declares `serviceAccountName`; no `ClusterRoleBinding`/`cluster-admin`.
- [ ] Image automation bootstrapped with `--read-write-key`; policies live in the automation's own namespace.
- [ ] HelmRelease drift detection deliberately set (`warn`/`enabled` with `ignore` paths) where drift matters.
- [ ] Cloud-auth resources use `spec.provider` workload identity, not static Secrets.

## References

- [Flux Docs](https://fluxcd.io/flux/) · [Guides](https://fluxcd.io/flux/guides/) · [Security](https://fluxcd.io/flux/security/)
- [Flagger (progressive delivery)](https://flagger.app/) · [GitOps Principles](https://opengitops.dev/)
