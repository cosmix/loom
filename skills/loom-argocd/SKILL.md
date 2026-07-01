---
name: loom-argocd
description: GitOps continuous delivery with Argo CD for Kubernetes. Use when implementing declarative GitOps workflows, application sync/rollback, multi-cluster deployments, progressive delivery, or CD automation.
allowed-tools:
  - Read
  - Edit
  - Write
  - Bash
triggers:
  - argocd
  - argo cd
  - gitops
  - application
  - sync
  - rollback
  - app of apps
  - applicationset
  - declarative
  - continuous delivery
  - CD
  - deployment automation
  - kubernetes deployment
  - multi-cluster
  - canary deployment
  - blue-green
---

# Argo CD GitOps Continuous Delivery

## Overview

Declarative GitOps CD for Kubernetes: Git is the source of truth, a controller reconciles cluster state toward it. Vocabulary you must be exact about: **target state** (Git), **live state** (cluster), **sync** (make live match target), **OutOfSync** (they differ), **health** (per-resource readiness, distinct from sync), **refresh** (recompare Git vs live), **AppProject** (RBAC/resource boundary grouping Applications).

Two facts that trip up most newcomers, stated up front:

- **OutOfSync ≠ unhealthy, and Healthy ≠ InSync.** They are orthogonal axes. A perpetually-OutOfSync-but-Healthy app almost always means a controller mutates a field after apply — fix with `ignoreDifferences` (see Expert Practices), not by re-syncing.
- **Argo CD only inflates manifests** (`helm template`, `kustomize build`) — it never runs `helm install`. `helm ls`/`helm rollback` show nothing.

## When Argo CD vs Flux

Both are CNCF-graduated GitOps controllers; the choice is architectural, not feature-parity.

| Concern | Argo CD | Flux CD |
| --- | --- | --- |
| Ordering | `argocd.argoproj.io/sync-wave` annotations *within* one Application; cross-Application needs ApplicationSet Progressive Sync | `dependsOn` between Kustomizations/HelmReleases + `healthChecks` gate the next |
| Composition | App-of-apps: one root Application recursing into child Applications | Kustomization tree: a Kustomization applies more Kustomizations |
| Drift correction | Opt-in `syncPolicy.automated.selfHeal` reverts live drift; `prune` deletes Git-removed resources | Continuous reconciliation always re-applies desired state; `prune: true` GCs by `.status.inventory` |
| Fan-out | ApplicationSet generators (cluster/git/matrix/PR/SCM) | No native generator; per-tenant Kustomizations + image automation |
| Image updates | Not built-in (separate Argo CD Image Updater) | First-class ImageRepository/ImagePolicy/ImageUpdateAutomation, commits back to Git |
| Interface | Web UI-centric (topology view, manual sync buttons, RBAC console) | CLI/CRD-centric (`flux` CLI, no first-party UI) |
| Multi-cluster | One control plane syncs many clusters | Typically one Flux per cluster pulling its own path |

Rule of thumb: **Argo CD** when operators want a visual sync/health console and generator-driven multi-cluster fan-out; **Flux** for a lean controller set, Git-native image automation, and dependency ordering expressed as CRDs. They coexist (Argo for app-team UI, Flux for platform automation).

## Install & Setup

```bash
kubectl create namespace argocd
kubectl apply -n argocd -f https://raw.githubusercontent.com/argoproj/argo-cd/stable/manifests/install.yaml
# Production: swap install.yaml -> ha/install.yaml (redis-ha, controller replicas)

kubectl -n argocd get secret argocd-initial-admin-secret -o jsonpath="{.data.password}" | base64 -d
kubectl port-forward svc/argocd-server -n argocd 8080:443

argocd login localhost:8080
argocd account update-password
argocd cluster add my-cluster-context          # register an external cluster
argocd repo add https://github.com/org/repo.git --username u --password $TOKEN
```

## Repository Structure

```text
gitops-repo/
├── apps/{base,overlays/{dev,staging,production}}/   # Kustomize bases + overlays
├── charts/myapp/                                     # Helm charts (if used)
├── argocd/{projects,applications,applicationsets}/   # Argo CRDs
└── bootstrap/root-app.yaml                           # app-of-apps entrypoint
```

| Separation | Pros | Cons |
| --- | --- | --- |
| Mono-repo | Simple, atomic cross-cutting changes | Coarse access control |
| Repo-per-env | Clear promotion path, security boundaries | Config duplication |
| Repo-per-team | Team autonomy/ownership | Cross-team coordination |

## Application Manifests

Basic Application (annotated — the fields that matter):

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: myapp
  namespace: argocd
  finalizers:
    - resources-finalizer.argocd.argoproj.io   # cascade-delete managed resources (see Deletion gotcha)
spec:
  project: default
  source:
    repoURL: https://github.com/org/repo.git
    targetRevision: HEAD          # branch, tag, or immutable SHA (prefer SHA for prod)
    path: apps/production/myapp
  destination:
    server: https://kubernetes.default.svc
    namespace: myapp-production
  syncPolicy:
    automated:
      prune: true                 # delete resources removed from Git
      selfHeal: true              # revert manual cluster edits (fights kubectl)
      allowEmpty: false           # refuse to sync an empty source (prevents mass-delete)
    syncOptions:
      - CreateNamespace=true
      - PrunePropagationPolicy=foreground
      - PruneLast=true            # prune only after new resources are Healthy
    retry:
      limit: 5
      backoff: { duration: 5s, factor: 2, maxDuration: 3m }
```

Source-type deltas (everything else identical to above). **Helm** — Argo runs `helm template`; `valueFiles` are last-wins (see Helm gotcha):

```yaml
  source:
    path: charts/myapp
    helm:
      valueFiles: [values.yaml, values-production.yaml]   # last file wins
      values: |                                            # inline, higher precedence
        replicaCount: 3
      parameters:                                          # highest precedence
        - { name: image.tag, value: v1.2.3 }
      releaseName: myapp        # ⚠ overriding this changes app.kubernetes.io/instance label
```

**Kustomize** — Argo runs `kustomize build`:

```yaml
  source:
    path: apps/overlays/production
    kustomize:
      namePrefix: prod-
      images:
        - { name: myapp, newName: myregistry.io/myapp, newTag: v1.2.3 }
      replicas:
        - { name: myapp-deployment, count: 3 }
```

## ApplicationSets

One ApplicationSet templates many Applications from a **generator**. Use for multi-cluster fan-out, multi-tenant, or discovering apps from repo structure.

| Generator | Use case |
| --- | --- |
| **cluster** | Same app to every registered cluster (matched by label) |
| **git (directories)** | One app per directory in a monorepo |
| **git (files)** | One app per config file (JSON/YAML params) |
| **list** | Static parameter list (tenant definitions) |
| **matrix** | Cross-product of two generators (apps × clusters/envs) |
| **pullRequest** | Ephemeral preview env per PR |
| **scmProvider** | Discover repos org-wide from GitHub/GitLab |

Cluster generator (fan-out to labelled clusters):

```yaml
apiVersion: argoproj.io/v1alpha1
kind: ApplicationSet
metadata: { name: cluster-apps, namespace: argocd }
spec:
  goTemplate: true
  goTemplateOptions: [missingkey=error]   # ⚠ else typos render blank targets — see gotcha
  generators:
    - clusters:
        selector: { matchLabels: { env: production } }
  template:
    metadata: { name: '{{.name}}-myapp' }
    spec:
      project: platform-team               # ⚠ hard-code, never template (privilege escalation)
      source: { repoURL: https://github.com/org/repo.git, targetRevision: main, path: apps/production/myapp }
      destination: { server: '{{.server}}', namespace: myapp }
      syncPolicy: { automated: { prune: true, selfHeal: true }, syncOptions: [CreateNamespace=true] }
```

Matrix (apps × environments — the common multi-env pattern):

```yaml
spec:
  generators:
    - matrix:
        generators:
          - git: { repoURL: https://github.com/org/apps.git, revision: HEAD, directories: [{ path: apps/* }] }
          - list:
              elements:
                - { env: dev,        cluster: https://dev.example.com,  replicas: "1" }
                - { env: production, cluster: https://prod.example.com, replicas: "3" }
  template:
    metadata: { name: '{{path.basename}}-{{env}}' }
    spec:
      project: default
      source:
        repoURL: https://github.com/org/apps.git
        targetRevision: HEAD
        path: '{{path}}'
        helm: { parameters: [{ name: replicaCount, value: '{{replicas}}' }] }
      destination: { server: '{{cluster}}', namespace: '{{path.basename}}-{{env}}' }
```

PR previews use `generators: [{ pullRequest: { github: { owner, repo, tokenRef }, labels: [preview] } }]` with `targetRevision: '{{head_sha}}'` and `namespace: myapp-pr-{{number}}`. SCM discovery uses `scmProvider.github` with `filters: [{ repositoryMatch: '.*-service$', pathsExist: [k8s/production] }]`.

## Sync Policy & Strategies

| Strategy | Use | Risk |
| --- | --- | --- |
| Automated + selfHeal | Non-prod | Low |
| Automated, no selfHeal | Staging (manual drift control) | Medium |
| Manual | Production | High |
| Sync windows | Business-hours restriction | Medium |
| Progressive (Argo Rollouts) | Gradual prod rollout | Low |

Full `syncOptions` reference (add to `syncPolicy.syncOptions`):

- `CreateNamespace=true` — create the destination namespace.
- `ServerSideApply=true` — SSA field ownership; required for large CRDs and self-managed Argo (see gotcha).
- `PruneLast=true` — prune after new resources Healthy.
- `PrunePropagationPolicy=foreground|background|orphan`.
- `RespectIgnoreDifferences=true` — honor `ignoreDifferences` *during sync*, not just diffing (see gotcha).
- `ApplyOutOfSyncOnly=true` — skip already-synced resources (faster large syncs; do NOT use right after a tracking-method change).
- `Validate=false` — skip `kubectl` schema validation.
- `Replace=true` — `kubectl replace` instead of apply (⚠ overrides ServerSideApply).

Sync windows (time-gated deploys, defined on AppProject):

```yaml
kind: AppProject
spec:
  syncWindows:
    - { kind: allow, schedule: "0 9 * * 1-5", duration: 8h, applications: ["*"] }   # business hours
    - { kind: deny,  schedule: "0 12 * * *",  duration: 2h, applications: ["*"] }    # peak-traffic freeze
    - { kind: allow, schedule: "* * * * *",   duration: 1h, manualSync: true, applications: [critical-app] }
```

`ignoreDifferences` on the Application stops sync loops from controller-mutated fields (pair with `RespectIgnoreDifferences=true`):

```yaml
spec:
  ignoreDifferences:
    - { group: apps, kind: Deployment, jsonPointers: [/spec/replicas] }    # HPA-managed
    - group: ""
      kind: Service
      jqPathExpressions: [".spec.ports[] | select(.nodePort != null) | .nodePort"]
```

> There is **no `syncWaves` field** in the Application CRD. Argo tolerates unknown fields, so such a block is silently ignored. Wave ordering is *only* the `argocd.argoproj.io/sync-wave` annotation on individual resources. For blue-green/canary with traffic shifting, use **Argo Rollouts** (separate `Rollout` CRD), not the Application spec.

## App of Apps Pattern

A root Application whose source is a directory of child Application manifests (`directory.recurse: true`), so one bootstrap sync creates the whole tree.

```text
root-app
├── infrastructure (sync-wave 0): cert-manager, ingress-nginx, external-dns
├── platform       (sync-wave 1): monitoring, logging, security
└── applications   (sync-wave 2): app1, app2, app3
```

```yaml
kind: Application
metadata:
  name: root-app
  namespace: argocd
  # ⚠ resources-finalizer intentionally OMITTED — with it, one `kubectl delete
  # application root-app` cascades through every child and wipes the environment.
spec:
  project: default
  source: { repoURL: https://github.com/org/gitops.git, targetRevision: HEAD, path: argocd/applications, directory: { recurse: true } }
  destination: { server: https://kubernetes.default.svc, namespace: argocd }
  syncPolicy: { automated: { prune: true, selfHeal: true }, syncOptions: [CreateNamespace=true] }
```

> **Restore the Application health check or wave ordering is a lie.** The built-in health assessment for the `argoproj.io/Application` CRD was **removed in Argo CD 1.8**. Without it the parent marks a child "done" the instant the Application *object* is applied — not when its workloads are Healthy — so each wave advances before the previous layer is Ready. Restore it in `argocd-cm` (see Expert Practices). Even restored, child-Application sync-waves are reliable only at *initial bootstrap*; for ongoing ordered multi-app rollouts use **ApplicationSet Progressive Syncs (RollingSync)**.

## Sync Waves & Hooks

Waves (`argocd.argoproj.io/sync-wave`, lower syncs first; default 0) order resources within one Application; Argo applies a wave, waits for Healthy, then advances. Typical: Namespace `"0"` → ConfigMap `"1"` → Deployment `"2"` → Service `"3"`.

Hooks are Jobs annotated with `argocd.argoproj.io/hook`:

| Hook | Runs |
| --- | --- |
| `PreSync` | Before sync (DB migrations) |
| `Sync` | During, with the main wave |
| `PostSync` | After all Healthy (smoke tests) |
| `SyncFail` | On sync failure (alert/rollback) |
| `PreDelete` | Before Application deletion |

`argocd.argoproj.io/hook-delete-policy`: `HookSucceeded` | `HookFailed` | `BeforeHookCreation`. Hooks also honor sync-wave annotations.

```yaml
kind: Job
metadata:
  name: db-migration
  annotations:
    argocd.argoproj.io/hook: PreSync
    argocd.argoproj.io/hook-delete-policy: HookSucceeded
    argocd.argoproj.io/sync-wave: "1"
spec:
  backoffLimit: 3
  template: { spec: { restartPolicy: Never, containers: [{ name: migrate, image: myapp:migrations, command: ["./migrate.sh"] }] } }
```

## Health Checks & Resource Customizations

Custom health for CRDs Argo doesn't understand (Lua in `argocd-cm`):

```yaml
kind: ConfigMap
metadata: { name: argocd-cm, namespace: argocd }
data:
  resource.customizations.health.cert-manager.io_Certificate: |
    hs = {}
    if obj.status ~= nil and obj.status.conditions ~= nil then
      for _, c in ipairs(obj.status.conditions) do
        if c.type == "Ready" and c.status == "True"  then return { status = "Healthy",  message = c.message } end
        if c.type == "Ready" and c.status == "False" then return { status = "Degraded", message = c.message } end
      end
    end
    return { status = "Progressing", message = "Waiting for certificate" }
  # Ignore differences globally (e.g. fields owned by another controller)
  resource.customizations.ignoreDifferences.apps_Deployment: |
    jsonPointers: [/spec/replicas]
  # Resource tracking: 3.0 default is `annotation`. `annotation+label` writes both
  # for migration compatibility. ⚠ Switching mid-lifecycle can orphan a resource if
  # the first post-upgrade sync also prunes — force a full sync (no ApplyOutOfSyncOnly) first.
  application.resourceTrackingMethod: annotation+label
  resource.exclusions: |
    - { apiGroups: ["*"], kinds: [ProviderConfigUsage], clusters: ["*"] }
```

## RBAC

AppProject bounds *what* an app may deploy and *where*; `argocd-rbac-cm` bounds *who* may act on apps.

```yaml
kind: AppProject
metadata: { name: team-a, namespace: argocd }
spec:
  sourceRepos: ["https://github.com/team-a/*"]
  destinations:
    - { namespace: "team-a-*", server: https://kubernetes.default.svc }
  clusterResourceWhitelist:                 # cluster-scoped kinds this project MAY create
    - { group: "", kind: Namespace }
  namespaceResourceBlacklist:               # namespaced kinds it may NOT create
    - { group: "", kind: ResourceQuota }
  roles:
    - name: developer
      policies:
        - p, proj:team-a:developer, applications, get,  team-a/*, allow
        - p, proj:team-a:developer, applications, sync, team-a/*, allow
      groups: [team-a-developers]
  orphanedResources: { warn: true }
```

```yaml
kind: ConfigMap
metadata: { name: argocd-rbac-cm, namespace: argocd }
data:
  # Format: p, subject, resource, action, object, effect
  policy.csv: |
    g, platform-team, role:admin
    p, role:app-deployer, applications, sync, */*, allow
    p, role:app-deployer, applications, update/*, */*, allow   # 3.0: sub-resource grants explicit
    p, role:app-deployer, applications, delete/*, */*, allow
    p, role:app-deployer, logs, get, */*, allow                # 3.0: logs now enforced
    g, deployers, role:app-deployer
  policy.default: ''      # ⚠ leave EMPTY — a default grant cannot be revoked by deny rules
  scopes: '[groups, email]'
```

## Secret Management

Argo CD does not decrypt secrets itself; integrate a tool:

- **Sealed Secrets** — commit `SealedSecret` CRs, the controller decrypts in-cluster. Zero Argo config.
- **External Secrets Operator** — commit `ExternalSecret` referencing a `SecretStore` (AWS SM/Vault/GCP); the operator materializes the `Secret`. Preferred for cloud secret managers.
- **Argo CD Vault Plugin / SOPS** — a repo-server CMP sidecar substitutes placeholders at manifest-generation time. `source.plugin.name` selects the sidecar; omit `name` for auto-discovery.

```yaml
# ExternalSecret (lives in Git, no plaintext):
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata: { name: myapp-secrets, namespace: myapp }
spec:
  refreshInterval: 1h
  secretStoreRef: { name: aws-secrets-manager, kind: SecretStore }
  target: { name: myapp-secret, creationPolicy: Owner }
  data:
    - { secretKey: db-password, remoteRef: { key: myapp/production/db, property: password } }
```

> **`source.plugin` resolves only if a matching CMP sidecar exists.** The `configManagementPlugins` key in `argocd-cm` was deprecated in v2.4 and **removed in v2.8**. Plugins now run as **sidecars on `argocd-repo-server`** (config `/home/argocd/cmp-server/config/plugin.yaml`, entrypoint `/var/run/argocd/argocd-cmp-server`). See Expert Practices for the sidecar spec.

## Multi-tenancy

Isolate tenants with an AppProject per tenant (scoped `sourceRepos`/`destinations`/resource whitelists) plus standard Kubernetes `ResourceQuota` and `NetworkPolicy` in the tenant namespace. Blacklist `ResourceQuota`, `LimitRange`, and `rbac.authorization.k8s.io/*` from tenant-writable kinds so a tenant cannot widen its own limits.

```yaml
kind: AppProject
metadata: { name: tenant-alpha, namespace: argocd }
spec:
  sourceRepos: ["https://github.com/tenant-alpha/*"]
  destinations: [{ namespace: "tenant-alpha-*", server: https://kubernetes.default.svc }]
  clusterResourceWhitelist: [{ group: "", kind: Namespace }]
  namespaceResourceBlacklist:
    - { group: "", kind: ResourceQuota }
    - { group: rbac.authorization.k8s.io, kind: "*" }
  roles:
    - { name: tenant-admin, policies: ["p, proj:tenant-alpha:tenant-admin, applications, *, tenant-alpha/*, allow"], groups: [tenant-alpha-admins] }
```

## Progressive Delivery (Argo Rollouts)

Argo Rollouts is a separate controller replacing `Deployment` with a `Rollout` CRD for canary/blue-green with analysis-driven promotion/abort. Manage the `Rollout` manifest via an ordinary Argo CD Application.

```yaml
kind: Rollout
spec:
  replicas: 5
  strategy:
    canary:
      steps:
        - setWeight: 20
        - pause: { duration: 10m }
        - setWeight: 60
        - pause: { duration: 10m }
      analysis: { templates: [{ templateName: success-rate }], startingStep: 1 }
      trafficRouting: { istio: { virtualService: { name: myapp-vsvc, routes: [primary] } } }
  selector: { matchLabels: { app: myapp } }
  template: { metadata: { labels: { app: myapp } }, spec: { containers: [{ name: myapp, image: myapp:stable }] } }
```

Blue-green uses `strategy.blueGreen` with `activeService`/`previewService`, `autoPromotionEnabled: false`, `scaleDownDelaySeconds`. Manual controls: `kubectl argo rollouts {promote,abort,undo,retry} myapp`.

## Rollback

Prefer **Git revert** (auto-synced, full audit trail) over imperative rollback:

```bash
git revert HEAD && git push origin main    # Argo CD reconciles the revert
```

Imperative rollback — note the automated-sync gotcha:

```bash
argocd app history myapp                       # find last-good revision
argocd app set myapp --sync-policy none        # ⚠ rollback FAILS while automated sync is on
argocd app rollback myapp [<revision>] [--prune]
# re-enable ONLY after the target is also reflected in Git:
argocd app set myapp --sync-policy automated --self-heal
```

Emergency runbook (last-resort escalation):

```bash
argocd app set myapp --sync-policy none        # 1. disable automation first
argocd app rollback myapp                       # 2. rollback to previous
argocd app sync myapp --force --replace --prune # 3. if stuck, force replace
# 4. if still failing, revert Git + force sync; 5. manual kubectl delete + resync as last resort
argocd app wait myapp --health --timeout 300
```

## Monitoring

Each component exposes metrics on a **different port** — mispairing the selector/port scrapes nothing:

| Component | Pod label `app.kubernetes.io/name` | Port | What lives there |
| --- | --- | --- | --- |
| application-controller | `argocd-application-controller` | `8082` | app sync/health metrics |
| server | `argocd-server` | `8083` | API/UI request metrics |
| repo-server | `argocd-repo-server` | `8084` | manifest-generation metrics |

```yaml
# ServiceMonitor for the controller metrics (the app metrics — NOT argocd-server):
kind: Service
metadata: { name: argocd-application-controller-metrics, namespace: argocd, labels: { app.kubernetes.io/name: argocd-application-controller-metrics } }
spec:
  ports: [{ name: metrics, port: 8082, targetPort: 8082 }]
  selector: { app.kubernetes.io/name: argocd-application-controller }   # NOT argocd-server
```

> **PromQL on 3.0:** per-app `argocd_app_sync_status`, `argocd_app_health_status`, `argocd_app_created_time` were **removed**. Use labels on `argocd_app_info` — e.g. `argocd_app_info{sync_status="OutOfSync"}`. Per-resource health is no longer persisted by default (`controller.resource.health.persist`).

Notifications via `argocd-notifications-cm` (triggers + templates) and per-app subscribe annotations:

```yaml
# argocd-notifications-cm:
data:
  trigger.on-health-degraded: |
    - { when: "app.status.health.status == 'Degraded'", send: [app-health-degraded] }
---
# on the Application:
metadata:
  annotations:
    notifications.argoproj.io/subscribe.on-health-degraded.slack: alerts-channel
```

## CLI Reference

```bash
argocd app create myapp --repo <url> --path apps/myapp \
  --dest-server https://kubernetes.default.svc --dest-namespace myapp \
  --sync-policy automated --auto-prune --self-heal
argocd app list | get myapp | diff myapp
argocd app sync myapp [--resource apps:Deployment:myapp] [--force --replace --prune]
argocd app wait myapp --health --timeout 300
argocd app set myapp --helm-set replicaCount=3
argocd app get myapp --hard-refresh          # force recompare when "OutOfSync, no changes"
argocd repo add <url> --ssh-private-key-path ~/.ssh/id_rsa   # or --username/--password
argocd cluster add <context> | list
argocd proj create myproject --src 'https://github.com/org/*' --dest <server>,myapp-*
```

## Troubleshooting

| Symptom | First moves |
| --- | --- |
| Stuck Progressing | `argocd app get myapp`; check child resource health; `argocd app sync --replace` |
| OutOfSync, no changes | `argocd app get --hard-refresh`; `argocd app diff`; usually a mutated field → `ignoreDifferences` |
| Permission denied | `argocd proj get <proj>`; inspect `argocd-rbac-cm`; remember `policy.default` cannot be denied |
| Validation error on sync | `argocd app sync --validate=false` or `syncOptions: [Validate=false]` |
| Deep debug | `kubectl logs -n argocd deploy/argocd-application-controller`; `kubectl get events -n argocd --field-selector involvedObject.name=myapp` |

## Best Practices (non-obvious only)

- **Immutable revisions for prod** — pin `targetRevision` to a SHA or immutable tag, never `HEAD`/floating branch.
- **`prune` + `PruneLast=true`** — never let prune run before replacements are Healthy.
- **`allowEmpty: false`** always — an accidentally-empty source with prune on deletes everything.
- **selfHeal only where you accept it fighting kubectl** — on debugging-heavy namespaces it reverts hotfixes mid-incident.
- **Webhooks over polling** — configure Git webhooks for sub-second sync instead of the 3m default poll.
- **Controller sharding** past ~1000 apps; tune Redis for repo-server cache.
- **All Argo config in Git** (Applications, AppProjects, `argocd-cm`) — it is your DR plan.

## Expert Practices: Idioms, Anti-Patterns & Gotchas

Mechanism-level guidance — each explains *why*. This is what separates a working manifest from one that silently misbehaves.

### Sync & Diffing

**`ignoreDifferences` alone is cosmetic during sync — pair it with `RespectIgnoreDifferences=true`.** By default `ignoreDifferences` only affects **drift detection** (what shows OutOfSync); the sync patch still resets those fields to Git values next sync. `RespectIgnoreDifferences=true` honors them during the sync stage too. Limitation: only works when the resource **already exists** — on initial creation the desired state is applied as-is. Essential for HPA-managed replicas, sidecar-injecting webhooks, cert-manager `caBundle`.

```yaml
spec:
  ignoreDifferences:
    - { group: apps, kind: Deployment, jsonPointers: [/spec/replicas] }        # HPA
    - group: admissionregistration.k8s.io
      kind: MutatingWebhookConfiguration
      jqPathExpressions: [".webhooks[].clientConfig.caBundle"]                  # cert-manager
  syncPolicy:
    syncOptions: [RespectIgnoreDifferences=true]
```

**Mutating webhooks/controllers cause perpetual OutOfSync.** Sidecar injection, HPA/VPA edits, cloud controllers writing `status.loadBalancer`, and quantity normalization (`1000m`→`1`, `3072Mi`→`3Gi`) all mutate resources after apply: each sync succeeds, live state diverges instantly, forever. Fix by targeting the mutated fields with `ignoreDifferences` (`jqPathExpressions`/`managedFieldsManagers`) **and** `RespectIgnoreDifferences=true`, ideally with `ServerSideApply=true`.

**Automated sync does NOT retry a failed commit-SHA — `selfHeal` re-triggers it.** Automated sync attempts exactly one sync per unique (SHA + parameters): a failed SHA "sticks" until a new commit arrives or `selfHeal` detects drift (retries after the self-heal timeout, 5s default). This silently breaks pipelines that push one commit and expect retries. Enable `selfHeal` for drift re-triggering; `syncPolicy.retry` only covers transient in-attempt errors.

**Self-managed Argo CD requires `ServerSideApply=true`; never combine with `Replace=true`.** Client-side apply stores prior state in the `last-applied-configuration` annotation (~262KB cap); large CRDs (the ApplicationSet CRD now exceeds it) overflow, and self-management hits field-ownership conflicts. SSA makes the Argo field manager own fields. But **"Replace=true takes precedence over ServerSideApply=true"** — setting both silently skips SSA.

### Hooks & Ordering

**Hooks are skipped during a selective sync; a failed `PreDelete` blocks Application deletion.** All hooks are skipped during `argocd app sync --resource ...`, so a `PreSync` migration silently doesn't run if someone selectively syncs only the Deployment. Separately, a failed `PreDelete` hook blocks the *whole* Application deletion until it succeeds or is removed — bound delete-time Jobs with `backoffLimit` and `activeDeadlineSeconds`.

**Sync waves order resources within ONE Application; for cross-Application ordering use Progressive Syncs.** Wave annotations on child Application objects only help during the parent's *initial* creation, because the `argoproj.io/Application` health check was removed in 1.8 — a wave advances as soon as the child Application *object* is applied. Restore it in `argocd-cm`:

```yaml
kind: ConfigMap
metadata: { name: argocd-cm, namespace: argocd }
data:
  resource.customizations.health.argoproj.io_Application: |
    hs = { status = "Progressing", message = "" }
    if obj.status ~= nil and obj.status.health ~= nil then
      hs.status = obj.status.health.status
      if obj.status.health.message ~= nil then hs.message = obj.status.health.message end
    end
    return hs
```

### Deletion & Finalizers

**Cascading delete needs `resources-finalizer.argocd.argoproj.io` — never add it reflexively to app-of-apps roots.** Without it, deleting an Application *orphans* its Deployments/Services. With it on an app-of-apps **root**, a single `kubectl delete` cascades through every child and wipes the environment. Add it deliberately (ephemeral PR previews), not on production roots.

### Helm

**Argo runs `helm template`, not `helm install` — and any Argo hook in a chart disables ALL Helm hooks.** `helm ls`/`history`/`rollback`/`test` are unavailable. "If you define any Argo CD hooks, all Helm hooks will be ignored" — never mix the two systems in one chart. Overriding `releaseName` breaks the `app.kubernetes.io/instance` label Argo injects, which can break label selectors.

**Helm `valueFiles` precedence is last-wins** (opposite of first-match-wins). Full order low→high: chart `values.yaml` < `valueFiles` (last wins) < inline `values` < `valuesObject` < `parameters`. Glob-matched files expand lexically — use numeric filename prefixes when order matters.

### ApplicationSet

**`applicationsSync` policies do NOT stop cascade deletion — add a finalizer.** `create-only`/`create-update` govern only the controller's *modify* ops; deleting the ApplicationSet still deletes child Applications via ownerReferences. Add `resources-finalizer.argocd.argoproj.io` (plus `preserveResourcesOnDeletion: true` to keep children's cluster resources). Also the controller-level `--policy` flag **overrides** per-set `applicationsSync` unless `--enable-policy-override` (`ARGOCD_APPLICATIONSET_CONTROLLER_ENABLE_POLICY_OVERRIDE`, default false).

**Progressive Syncs (RollingSync) force-disable autosync on every generated Application.** The controller sequences syncs itself; trigger at step level, not per app. Apps not selected by any step are **excluded** and must be synced manually. Beta in v3.3.0 but still **behind a feature flag** — enable explicitly. Ensure step `matchExpressions` match the labels your template actually sets.

```yaml
spec:
  strategy:
    type: RollingSync
    rollingSync:
      steps:
        - matchExpressions: [{ key: env, operator: In, values: [staging] }]
        - matchExpressions: [{ key: env, operator: In, values: [production] }]
          maxUpdate: 25%
  template: { metadata: { labels: { env: '{{env}}' } } }   # must match step selectors
```

**Use `ignoreApplicationDifferences` so the controller stops reverting per-app overrides.** By default the controller reverts any generated-Application field diverging from the template (including `syncPolicy`) within seconds — so you can't disable auto-sync on one app during an incident. List `jsonPointers`/`jqPathExpressions` to leave alone (commonly `/spec/syncPolicy`). Limitation: MergePatch replaces whole lists, so target list elements with a JQ path. (Distinct from per-resource `ignoreDifferences`.)

**Go-template ApplicationSets render missing keys as empty strings unless `goTemplateOptions: [missingkey=error]`.** A typo `{{.server}}`→`{{.srevr}}` silently produces Applications with blank destinations/namespaces, deploying to the wrong place. Applies only to `goTemplate: true`.

### Security

**`policy.default` grants ALL authenticated users a baseline that deny rules cannot revoke.** "This access cannot be blocked by a deny rule." So `policy.default: role:readonly` + per-user denies is silently ineffective. Leave `policy.default` **empty** and grant least-privilege roles per group. `deny` otherwise beats `allow` at equal scope — but never overrides the default grant.

**Never template the ApplicationSet `project` field; SCM/PR generators are admin-only.** If `template.spec.project` is templated from generator output, anyone who can write the generator's source can steer Applications into a privileged project and escalate. Hard-code `project`. SCM Provider and Pull Request generators are admin-only ("Only admins may create ApplicationSets to avoid leaking Secrets").

### Version-Breaking Changes

**Config Management Plugins in `argocd-cm` removed in v2.8 — use repo-server sidecars** (config `/home/argocd/cmp-server/config/plugin.yaml`, entrypoint `/var/run/argocd/argocd-cmp-server`; omit `plugin.name` for auto-discovery):

```yaml
# argocd-repo-server: add a CMP sidecar
containers:
  - name: avp
    image: my-avp-image:latest
    command: [/var/run/argocd/argocd-cmp-server]
    volumeMounts:
      - { name: var-files, mountPath: /var/run/argocd }
      - { name: plugins, mountPath: /home/argocd/cmp-server/plugins }
      - { name: plugin-config, mountPath: /home/argocd/cmp-server/config/plugin.yaml, subPath: plugin.yaml }
```

**Argo CD 3.0 is a high-blast-radius upgrade (2.14→3.0 notes):**

1. **Resource tracking** label→**annotation**; opt out with `application.resourceTrackingMethod: label`. Force a full sync (no `ApplyOutOfSyncOnly`) before any prune, or a mid-lifecycle switch can orphan a pruned resource.
2. **RBAC sub-resource inheritance removed** — `update/*`, `delete/*`, and `logs, get` now need explicit policies.
3. **`repositories`/`repository.credentials` in `argocd-cm` removed** — migrate to Secrets labeled `argocd.argoproj.io/secret-type: repository`.
4. **Metrics removed** — use `argocd_app_info` labels; per-resource health no longer persisted by default.

## Verification Checklist

Before declaring an Argo CD change done:

- [ ] Every automated app sets `prune`, `selfHeal`, `allowEmpty: false` deliberately (not copy-paste).
- [ ] `targetRevision` is an immutable SHA/tag for production apps.
- [ ] Any controller-mutated field (HPA replicas, webhook `caBundle`) has `ignoreDifferences` **and** `RespectIgnoreDifferences=true` — no perpetual OutOfSync.
- [ ] App-of-apps root does **not** carry `resources-finalizer`; ephemeral roots that should cascade **do**.
- [ ] Cross-Application ordering uses ApplicationSet Progressive Sync, not child sync-waves; the Application health check is restored in `argocd-cm`.
- [ ] ApplicationSet `template.spec.project` is hard-coded; `goTemplateOptions: [missingkey=error]` set.
- [ ] `policy.default` is empty; RBAC grants are least-privilege per group; 3.0 sub-resource/logs policies present if on 3.x.
- [ ] No `Replace=true` alongside `ServerSideApply=true`.
- [ ] Secrets are encrypted (Sealed/ESO/SOPS) — no plaintext in Git.
- [ ] Prometheus ServiceMonitors pair the correct selector with 8082/8083/8084.

## References

- [Argo CD Docs](https://argo-cd.readthedocs.io/) · [Best Practices](https://argo-cd.readthedocs.io/en/stable/user-guide/best_practices/)
- [Argo Rollouts](https://argoproj.github.io/argo-rollouts/) · [GitOps Principles](https://opengitops.dev/)
