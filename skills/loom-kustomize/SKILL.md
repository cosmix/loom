---
name: loom-kustomize
description: Kubernetes-native configuration management with Kustomize. Use for environment-specific configs, resource patching (strategic merge, JSON6902), ConfigMap/Secret generation, overlays/bases, multi-environment deployments, and GitOps workflows.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - kustomize
  - kustomization
  - overlay
  - base
  - patch
  - strategic merge
  - json patch
  - json6902
  - configmap generator
  - secret generator
  - namespace
  - namePrefix
  - nameSuffix
  - commonLabels
  - commonAnnotations
  - component
  - transformer
  - replacement
  - multi-environment
  - dev/staging/prod configs
  - k8s manifest management
---

# Kustomize Skill

## Overview

Template-free, declarative Kubernetes config management via merging. `kubectl apply -k <dir>` and `kustomize build <dir>` render an overlay. Failure modes here are overwhelmingly **silent** (wrong output, exit 0) — always `diff` the `kustomize build` output before/after any structural change.

## Core Concepts

| Term            | Meaning                                                                                          |
| --------------- | ------------------------------------------------------------------------------------------------ |
| **Base**        | Dir with `kustomization.yaml` + shared resources; never modified in place.                       |
| **Overlay**     | Dir referencing a base under `resources:`, applying env-specific patches/generators.             |
| **Patch**       | Partial resource that modifies existing ones — strategic merge (default) or JSON 6902.           |
| **Component**   | Reusable opt-in feature bundle (`kind: Component`); runs against the parent's accumulated set.    |
| **Generator**   | Builds ConfigMaps/Secrets from literals/files/envs, appending a content-hash name suffix.        |
| **Transformer** | Cross-cutting mutation: `namespace`, `namePrefix`/`nameSuffix`, `labels`, `images`, `replicas`.  |
| **Replacement** | Copies a field value from a source resource into target field paths (successor to `vars`).       |

## Directory Layout

```text
k8s/
├── base/                    # shared, env-agnostic (kustomization.yaml + resources)
├── overlays/
│   ├── dev/                 # low resources, debug
│   ├── staging/             # + monitoring component
│   └── prod/                # HA, security, digest-pinned images
└── components/              # opt-in features: monitoring, ingress, debug-tools
```

Keep the hierarchy **shallow** (base → overlay, not base → overlay1 → overlay2). Paths in `resources:` are relative to the kustomization.yaml's own directory.

## Quick Reference

| Task             | Command                                                                    |
| ---------------- | -------------------------------------------------------------------------- |
| Build            | `kustomize build k8s/overlays/prod`                                        |
| Preview diff     | `kubectl diff -k k8s/overlays/prod`                                        |
| Apply            | `kubectl apply -k k8s/overlays/prod`                                       |
| Validate         | `kustomize build k8s/overlays/prod \| kubectl apply --dry-run=client -f -` |
| Migrate deprecated fields | `kustomize edit fix` (idempotent; run per dir; see Idioms)        |
| Set image tag    | `kustomize edit set image myapp=registry/myapp:v1.2.3`                     |
| Add ConfigMap    | `kustomize edit add configmap app-config --from-literal=KEY=value`         |

**Generators** (`configMapGenerator` / `secretGenerator` items): `literals:` (`K=v`), `files:` (`app.properties`, or `alias=path`), `envs:` (`config.env`). Secrets add `type:` (e.g. `kubernetes.io/tls`). `behavior:` = `create` (default) | `merge` | `replace`.

**Transformers** (top-level keys): `namespace`, `namePrefix`, `nameSuffix`, `labels` (use over deprecated `commonLabels`), `commonAnnotations`, `images` (`newName`/`newTag`/`digest`), `replicas` (`- name: x count: n`), `replacements`.

## Patch Strategies

| Aspect        | Strategic Merge (default)                                    | JSON 6902 (RFC 6902)                                    |
| ------------- | ----------------------------------------------------------- | ------------------------------------------------------ |
| Merge model   | K8s-schema-aware; maps by key, lists merge on known key     | `op` (`add`/`remove`/`replace`/`move`/`copy`/`test`) on JSON Pointer paths |
| Best for      | Field/env/resource-limit tweaks, adding containers          | Precise array-index ops, conditional `test`, CRDs      |
| Whole-list replace | **Unreliable** — `$patch: replace` regressed (issue #2980, ignored/merged instead) | Reliable: `op: replace` on the list path          |

Both go under the unified `patches:` field, which auto-detects type from content and whose `target` supports `group/version/kind/name/namespace` + `labelSelector`/`annotationSelector` (one patch → many resources). `patchesStrategicMerge`/`patchesJson6902` are deprecated (v5.0.0) and gone from the v1 API.

## Components

Reusable opt-in bundles for features only some environments need (monitoring, ingress, debug tools, security hardening). Use a **patch** instead for required, always-applied, or env-specific value tweaks.

```yaml
# k8s/components/monitoring/kustomization.yaml
apiVersion: kustomize.config.k8s.io/v1alpha1   # alpha — API may change without deprecation window
kind: Component
resources:
  - servicemonitor.yaml
patches:
  - path: patch-metrics.yaml
    target: {kind: Deployment}
labels:
  - pairs: {prometheus.io/scrape: "true"}
    includeSelectors: false
```

> **Reference Components under `components:`, never `resources:`.** A Component placed under `resources:` is silently misapplied as a plain manifest; kustomize enforces the split (Components rejected from `resources:`, Kustomizations from `components:`). Components run against the parent's accumulated resource set and **can be nested**.

## Examples

### Base kustomization + Deployment

```yaml
# k8s/base/kustomization.yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
namespace: myapp

# Use the labels transformer, NOT deprecated commonLabels (which mutates the
# IMMUTABLE spec.selector.matchLabels → delete/recreate on any later change).
# includeSelectors defaults false (safe on live resources); includeTemplates
# reaches pod templates. Define real selector labels directly in deployment.yaml.
labels:
  - pairs: {app: myapp, managed-by: kustomize}
    includeSelectors: false
    includeTemplates: true

commonAnnotations: {contact: team@example.com}

resources:
  - deployment.yaml
  - service.yaml
  - serviceaccount.yaml

configMapGenerator:
  - name: app-config
    literals: [LOG_LEVEL=info, MAX_CONNECTIONS=100]

images:
  - name: myapp
    newName: registry.example.com/myapp
    newTag: latest
```

```yaml
# k8s/base/deployment.yaml (selector label defined explicitly — not injected)
apiVersion: apps/v1
kind: Deployment
metadata: {name: myapp}
spec:
  replicas: 1
  selector:
    matchLabels: {app: myapp}
  template:
    metadata:
      labels: {app: myapp}
    spec:
      serviceAccountName: myapp
      containers:
        - name: app
          image: myapp
          ports: [{containerPort: 8080, name: http}]
          envFrom: [{configMapRef: {name: app-config}}]
          resources:
            requests: {memory: 128Mi, cpu: 100m}
            limits: {memory: 256Mi, cpu: 200m}
          readinessProbe:
            httpGet: {path: /ready, port: http}
            initialDelaySeconds: 5
```

### Production overlay (annotated)

```yaml
# k8s/overlays/prod/kustomization.yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
namespace: myapp-prod

labels:
  - pairs: {environment: prod, criticality: high}
    includeSelectors: false
    includeTemplates: true

commonAnnotations:
  oncall: sre-team@example.com

# SINGLE resources: block. A second resources: key later in the same document
# silently wins (duplicate YAML map keys), dropping ../../base entirely.
resources:
  - ../../base
  - poddisruptionbudget.yaml
  - horizontalpodautoscaler.yaml
  - networkpolicy.yaml

patches:
  - path: patch-resources.yaml     # bump requests/limits
  - path: patch-affinity.yaml      # required podAntiAffinity + nodeAffinity
  - path: patch-security.yaml      # runAsNonRoot, drop ALL, RO rootfs, seccomp

configMapGenerator:
  - name: app-config
    behavior: merge
    envs: [config-values.env]

secretGenerator:
  - name: app-secrets
    envs: [secrets.env]            # gitignored / external-managed

images:
  - name: myapp
    newTag: v1.2.3
    digest: sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  # pin by digest in prod

replicas:
  - {name: myapp, count: 5}

components:
  - ../../components/monitoring
  - ../../components/ingress
```

Overlay patch files are minimal partial docs matched by `kind`/`name`, e.g. a security patch:

```yaml
# patch-security.yaml
apiVersion: apps/v1
kind: Deployment
metadata: {name: myapp}
spec:
  template:
    spec:
      securityContext:
        runAsNonRoot: true
        runAsUser: 1000
        seccompProfile: {type: RuntimeDefault}
      containers:
        - name: app
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities: {drop: [ALL]}
```

### JSON 6902 patch (precise array ops)

```yaml
# kustomization.yaml
patches:
  - path: json-patch.yaml
    target: {group: apps, version: v1, kind: Deployment, name: myapp}
```

```yaml
# json-patch.yaml — list of {op, path, value}; auto-detected as JSON 6902
- op: add                                                 # append sidecar
  path: /spec/template/spec/containers/-
  value: {name: log-shipper, image: fluent/fluent-bit:2.0}
- op: replace                                             # container 0 image
  path: /spec/template/spec/containers/0/image
  value: registry.example.com/myapp:v1.2.3
- op: remove                                              # env var by index
  path: /spec/template/spec/containers/0/env/3
- op: test                                                # conditional guard
  path: /spec/replicas
  value: 1
- op: replace
  path: /spec/replicas
  value: 5
```

### Generators

```yaml
configMapGenerator:
  - name: app-config
    behavior: merge                 # create (default) | replace | merge
    literals: [LOG_LEVEL=debug]     # overrides base key
    files: [app.properties, tls.crt=certs/server.crt]
    envs: [config.env]
    options:
      disableNameSuffixHash: true   # ⚠ defeats config-driven rollouts (see Gotchas)

secretGenerator:
  - name: tls-secrets
    files: [tls.crt, tls.key]
    type: kubernetes.io/tls
  - name: app-secrets
    envs: [secrets.env]             # gitignored — never commit secrets
```

### Replacements (propagate a field value)

```yaml
replacements:
  # Propagate hashed ConfigMap name into the Deployment volume ref
  - source: {kind: ConfigMap, name: app-config, fieldPath: metadata.name}
    targets:
      - select: {kind: Deployment}
        fieldPaths:
          - spec.template.spec.volumes.[name=config].configMap.name
```

### Remote bases

```yaml
resources:
  - https://github.com/org/repo//k8s/base?ref=v1.0.0   # pin to tag or full SHA (see Security)
```

## Common Tasks

```bash
# Validate / preview
kustomize build k8s/overlays/prod | kubectl apply --dry-run=server -f -
kubectl diff -k k8s/overlays/prod

# Inspect name/hash transformations
kustomize build k8s/overlays/prod | rg '^  name:'
kustomize build k8s/overlays/prod | rg -A2 'kind: ConfigMap'

# Migrate deprecated fields in place (idempotent; wrap in a build-diff)
kustomize edit fix

# Bootstrap kustomization.yaml from existing manifests
kustomize create --autodetect        # or --resources deployment.yaml,service.yaml
```

## GitOps & CI/CD Integration

**ArgoCD**: `Application.spec.source.path: k8s/overlays/prod`, `syncPolicy.automated {prune, selfHeal}`.
**Flux**: `kustomize.toolkit.fluxcd.io/v1 Kustomization` with `spec.path`, `prune: true`, `sourceRef` (GitRepository), `healthChecks`.

Both bundle their **own** kustomize version — pin/match it (see Currency gotcha). Prefer the controller's authenticated Git source over remote bases in `resources:`.

```bash
# CI: render, then validate. kubeval is unmaintained — use kubeconform.
kustomize build "k8s/overlays/${OVERLAY}" > all.yaml
kubeconform -strict -summary -kubernetes-version 1.29.0 all.yaml
conftest test all.yaml    # OPA policy gate
```

### Helm inflation gotcha

> **`helmCharts` requires `kustomize build --enable-helm`.** Without the flag the whole `helmCharts` section is **silently skipped** (no error), so CI can emit manifests missing entire components yet still "pass". `kubectl apply -k` CANNOT pass `--enable-helm` (`unknown flag`), making `helmCharts` effectively dead there — use `kustomize build --enable-helm | kubectl apply -f -`. It's a deliberately limited subset (no private-registry auth, no post-renderers). For anything nontrivial, `helm template` then kustomize-patch on top.

## Troubleshooting

| Error (excerpt)                              | Cause / Fix                                                                 |
| -------------------------------------------- | -------------------------------------------------------------------------- |
| `evalsymlink failure ... no such file`       | Wrong base path; paths are relative to the kustomization.yaml's own dir.   |
| `no matches for OriginalId ... ConfigMap`    | Patched resource missing in base, or name/kind mismatch.                   |
| `conflict: multiple matches`                 | Patch too broad — add metadata or use JSON 6902 for precise targeting.     |
| `base ... refers back to overlay` (cyclic)   | Bases must never reference overlays.                                        |
| `unknown flag: --enable-helm`                | Using `kubectl apply -k`; switch to `kustomize build --enable-helm`.       |

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal guidance for production Kustomize. Most of these failure modes are **silent** — wrong output with exit code 0 — so always diff `kustomize build` output before and after any structural change.

### Anti-Patterns (deprecated fields removed from the v1 API)

**`commonLabels` → `labels` transformer.** `commonLabels` (deprecated v5.0.0) injects every label into `spec.selector.matchLabels` in addition to `metadata.labels` and pod templates. Because a Deployment/StatefulSet/Service selector is **immutable after first apply**, adding or changing any `commonLabel` later forces a delete-and-recreate. The `labels` transformer defaults `includeSelectors: false` (safe on live resources); add `includeTemplates: true` so pod labels still propagate (Prometheus scraping, log aggregation). Reserve `includeSelectors: true` for stable identity labels set only at creation time — and prefer defining those directly in base manifests so the selector contract is explicit. The docs warn that changing selectors on live resources "could result in failures."

```yaml
labels:
  - pairs:
      app.kubernetes.io/part-of: myplatform
      environment: prod
    includeSelectors: false   # safe on live resources — does NOT touch matchLabels
    includeTemplates: true    # still reaches pod template labels
```

**`patchesStrategicMerge` / `patchesJson6902` → unified `patches`.** Both deprecated in v5.0.0. The `patches` field auto-detects the type from content (resource-shaped doc = strategic merge; list of `{op, path, value}` = JSON 6902), and its `target` supports `group/version/kind/name/namespace` plus `labelSelector` and `annotationSelector`, so one patch can hit many resources. **Migration is not behavior-neutral:** `patches` runs *after* the label transformers while `patchesJson6902` ran *before* them, so a patch that touched `/metadata/labels` can produce different output post-migration. Always diff.

**`vars` → `replacements`.** `vars` (`$(VAR)` substitution, deprecated v5.0.0) is replaced by `replacements`, a structured source/target model that propagates any field value (including hashed ConfigMap/Secret names). **Migration is NOT 1:1:** `replacements` works on field pointers and cannot do free-form mid-string concatenation like `path: $(ROOT)/cluster/$(REVISION)`. It *can* do single-segment replacement within a string via the `delimiter` + `index` options, but for true composite strings, restructure (compose in an init container/env var) before migrating.

```yaml
replacements:
  - source: {kind: ConfigMap, name: app-config, fieldPath: metadata.name}
    targets:
      - select: {kind: Deployment}
        fieldPaths:
          - spec.template.spec.volumes.[name=config].configMap.name
```

### Idioms

**`kustomize edit fix` is the canonical, idempotent migration tool.** It migrates deprecated fields in place — `bases`→`resources`, `commonLabels`→`labels`, `patchesStrategicMerge`/`patchesJson6902`→`patches`, and (experimentally, `--vars`) `vars`→`replacements`. Run it per kustomization directory; use it as a CI lint to surface remaining deprecated fields. Two things it can't guarantee semantically: the patch apply-order shift and `vars` mid-string interpolation — so wrap it in a build diff:

```bash
kustomize build . > before.yaml
kustomize edit fix
kustomize build . > after.yaml
diff before.yaml after.yaml   # confirm semantic equivalence
```

**Components apply against the accumulated resource set.** Components use `apiVersion: kustomize.config.k8s.io/v1alpha1`, `kind: Component` (alpha — the API may change without a deprecation window). Unlike a base, a Component runs its patches/generators against the resource set the *parent* has accumulated so far, enabling additive opt-in feature bundles. They must be referenced under `components:`, never `resources:`, and **can be nested** (a Component may itself reference other Components via `components:`).

**Design with Kustomize's deliberate constraints.** The maintainers commit to NOT supporting: (1) removal directives (you cannot delete a label/patch/resource from a base — restructure/fork the base, or use a JSON 6902 `op: remove` for a specific path); (2) `${VAR}` templating; (3) build-time side effects from CLI args or shell env vars (output must be fully determined by version-controlled files); (4) globs. The sanctioned pattern for env-driven output is to mutate the kustomization, commit, then build:

```bash
kustomize edit set image myapp=registry.example.com/myapp:${GIT_SHA}
kustomize build overlays/prod
```

**Namespace transformer only re-homes `default`-named RBAC subjects.** Setting top-level `namespace:` runs the namespace transformer in `DefaultSubjectsOnly` mode: it only namespaces `RoleBinding`/`ClusterRoleBinding` subjects whose `name` is literally `default`. A ServiceAccount subject with any other name gets **no namespace** — silently breaking RBAC — unless you configure a custom `NamespaceTransformer` with `setRoleBindingSubjects: allServiceAccounts`. Verify RBAC subjects explicitly after setting a namespace.

### Gotchas (silent failures)

**Strategic merge patches silently REPLACE entire list fields on CRDs.** For built-in types, strategic merge knows array merge keys from the embedded OpenAPI schema (e.g. `containers` merges on `name`). Custom Resources have no such schema, so kustomize falls back to JSON merge semantics: a patched list field **replaces the whole array** with no error — dropping every element not in the patch. Fix with either an `openapi` field carrying `x-kubernetes-patch-merge-key`/`x-kubernetes-patch-strategy`, or a JSON 6902 patch (which behaves identically for built-in and custom resources):

```yaml
patches:
  - target: {group: argoproj.io, version: v1alpha1, kind: Application, name: my-app}
    patch: |-
      - op: add
        path: /spec/sources/-
        value:
          repoURL: https://github.com/org/repo
```

**`disableNameSuffixHash: true` defeats config-driven rolling updates — and a global `true` cannot be overridden.** The content-hash suffix (`app-config-k8bgk8h4t6`) is what triggers a rollout on config change: the new name changes the pod spec, so pods are replaced. Disabling it gives a stable name but silently breaks this — config changes apply to the ConfigMap, but Deployments do not roll. Worse, the boolean follows "global true wins": setting it in top-level `generatorOptions` strips the hash from **every** generator, and a per-generator `disableNameSuffixHash: false` is **ignored**. Only ever disable per-generator, and only for resources referenced by a fixed external name; if you need a stable name *and* rolling updates, keep the hash and propagate the hashed name via `replacements`.

**`replacements` list filters like `[name=nginx]` are unanchored regex.** Since v4.5.3, `spec.template.spec.containers.[name=nginx].image` matches *any* element whose name *contains* `nginx` — `my-nginx`, `nginx-sidecar` — and all matches are targeted, silently overwriting several containers. Anchor explicitly with `[name=^nginx$]`, or use index-based paths. (This differs from a `patches` `target`'s `name`/`namespace`, which ARE auto-anchored.)

**`configMapGenerator` `behavior: merge` matches by logical name AND namespace.** An overlay generator with `behavior: merge` matches the base by pre-hash name — but if one side specifies `namespace` and the other omits it, kustomize treats them as different generators and creates a **second** ConfigMap instead of merging (no error). `merge` also only merges discrete keys; if both reference the same filename, the overlay file replaces the base file entirely. Keep `namespace` consistent (or absent) on both sides, and verify the build emits one ConfigMap, not two.

**`sortOptions` only applies to the top-level (build-target) kustomization.** `sortOptions` (`order: legacy | fifo`; v5.0.0+, replaces `--reorder`) controls output ordering — `legacy` (default) places Namespaces/ServiceAccounts/RBAC first, which is why `kubectl apply -k` usually works without explicit ordering. Instances in bases/components reached via `resources` are **ignored**, so set it in the overlay that is the actual build target (what ArgoCD/Flux points to), never in a shared base.

**Pin a standalone kustomize version.** kubectl embeds an older kustomize that does not track standalone releases (the lag has spanned major versions). Features added after the embedded version — unified `patches`, `sortOptions`, Components, newer `replacements` behaviors — may error or be silently ignored under `kubectl apply -k` while a dev's standalone v5 produces correct output. ArgoCD/Flux also bundle their own versions. Standardize on a pinned `kustomize build | kubectl apply -f -` in CI and match the GitOps controller's version. Check with `kustomize version` and `kubectl version --client -o json` (`kustomizeVersion`).

### Security

**Never use `--load-restrictor=LoadRestrictionsNone` with untrusted input.** The default `LoadRestrictionsRootOnly` (v2.0) prevents file references (`resources`, `configMapGenerator` files) from reading outside the kustomization root — a real security boundary. With it off, a kustomization you don't fully control (e.g. a compromised remote base) can use a `configMapGenerator` file reference to read arbitrary host files (credentials, TLS keys, `/etc/passwd`). It also breaks relocatability. Only use it in fully trusted, audited build environments; the legitimate alternative for cross-directory file needs is a proper base with its own `kustomization.yaml`.

**Pin remote bases to a full commit SHA or tag.** Kustomize fetches remote bases over the network at build time. `?ref=main` (or no ref, which uses default-branch HEAD) means a force-push or compromised upstream flows straight into your cluster on the next reconcile with no change to your repo. Short SHAs are unsupported — use a full fetchable SHA (strongest; tags can be force-pushed) or a versioned tag. Remote fetches also add latency and a rate-limit/availability dependency, so mirror critical bases locally. In GitOps, prefer the controller's own authenticated `GitRepository`/`OCIRepository` source.

```yaml
resources:
  - https://github.com/org/platform-base//k8s/base?ref=a3f8c2d1e4b5f6a7b8c9d0e1f2a3b4c5d6e7f8a9   # full SHA
  - https://github.com/org/platform-base//k8s/base?ref=v2.3.1                                       # or versioned tag
```

**Never commit secrets.** Use sealed-secrets / external-secrets-operator / SOPS, or `secretGenerator` with gitignored env files. Pin production images by `digest`, not mutable tags. Enforce policy at admission (OPA Gatekeeper, Kyverno).

### Currency

**Validate with `kubeconform`, not `kubeval`.** `kubeval` is unmaintained and its schema registry is stale, so it cannot validate recent Kubernetes API versions and silently passes manifests using newer/renamed fields. `kubeconform` is the maintained successor on the same JSON-schema approach, with a current registry and CRD support via `-schema-location`:

```bash
kustomize build k8s/overlays/prod | kubeconform -strict -summary -kubernetes-version 1.29.0
```

## Verification Checklist

- [ ] `kustomize build <overlay>` succeeds and output diffed against the prior render for **unintended** changes.
- [ ] No deprecated fields remain (`commonLabels`/`patchesStrategicMerge`/`patchesJson6902`/`vars`/`bases`) — `kustomize edit fix` clean.
- [ ] Labels use the `labels` transformer with `includeSelectors: false`; selectors defined explicitly in base manifests.
- [ ] Exactly one `resources:` key per file; Components under `components:` (never `resources:`).
- [ ] ConfigMap/Secret hash suffix intact where rolling updates are wanted; `merge` generators emit ONE object (consistent `namespace`).
- [ ] CRD list patches use JSON 6902 (or `openapi` merge keys), not bare strategic merge.
- [ ] Prod images pinned by digest; secrets gitignored/external; remote bases pinned to full SHA/tag.
- [ ] CI validates with `kubeconform` (+ policy gate); GitOps controller kustomize version matches CI.
