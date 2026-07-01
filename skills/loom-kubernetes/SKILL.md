---
name: loom-kubernetes
description: Kubernetes deployment, cluster architecture, security, and operations. Use for manifests, Helm charts, RBAC, network policies, operators/CRDs, PodSecurityStandards, troubleshooting, and production best practices.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - kubernetes
  - k8s
  - pod
  - deployment
  - statefulset
  - daemonset
  - service
  - ingress
  - configmap
  - secret
  - pvc
  - namespace
  - helm
  - chart
  - kubectl
  - cluster
  - rbac
  - networkpolicy
  - podsecurity
  - operator
  - crd
  - job
  - cronjob
  - hpa
  - pdb
  - kustomize
---

# Kubernetes

## Overview

Production Kubernetes resource design, security hardening, Helm, and operations. The annotated examples below and the **Expert Practices** section carry the load-bearing knowledge — most production failures here are silent (no API error, no event), surfacing only under load, node maintenance, or a hardened cluster.

## Workload & Identity Cheatsheet

| Kind            | Identity guarantee                                                              | Use for                                     |
| --------------- | ------------------------------------------------------------------------------- | ------------------------------------------- |
| **Deployment**  | Fungible pods, random names, no ordering                                        | Stateless services                          |
| **StatefulSet** | Stable ordinal identity + DNS + per-pod PVC; ordered rollout (`OrderedReady`)   | Databases, quorum systems, sharded stores   |
| **DaemonSet**   | One pod per (matching) node                                                     | Node agents: logging, CNI, node-exporter    |
| **Job/CronJob** | Run-to-completion / scheduled                                                   | Batch, migrations, backups                  |

Rollout knobs: `strategy.rollingUpdate.maxSurge`/`maxUnavailable` (Deployment); `maxUnavailable` + `partition` (StatefulSet). `imagePullPolicy`: `IfNotPresent` for immutable tags/digests; `Always` only for mutable tags (adds a registry round-trip per start).

## Examples

### Production Deployment (annotated)

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: api-server
  namespace: production
  labels: {app: api-server, version: v1.2.0}
spec:
  replicas: 3
  strategy:
    type: RollingUpdate
    rollingUpdate: {maxSurge: 1, maxUnavailable: 0}
  selector:
    matchLabels: {app: api-server}
  template:
    metadata:
      labels: {app: api-server, version: v1.2.0}
      annotations: {prometheus.io/scrape: "true", prometheus.io/port: "8080"}
    spec:
      serviceAccountName: api-server
      securityContext:
        runAsNonRoot: true
        runAsUser: 1000
        fsGroup: 1000
        seccompProfile:
          type: RuntimeDefault          # REQUIRED by Restricted PSS; omitting it = pod rejected
      containers:
        - name: api
          image: myregistry.io/api-server:v1.2.0
          imagePullPolicy: IfNotPresent
          ports: [{name: http, containerPort: 8080}]
          env:
            - name: DATABASE_URL
              valueFrom: {secretKeyRef: {name: api-secrets, key: database-url}}
          resources:
            requests: {cpu: 100m, memory: 128Mi}
            limits: {memory: 512Mi}      # memory limit kept; CPU limit omitted (see CFS throttling)
          # startupProbe gates liveness/readiness — prefer over a large initialDelaySeconds
          startupProbe:
            httpGet: {path: /health/live, port: http}
            failureThreshold: 30         # 30 * 10s = 5 min startup budget
            periodSeconds: 10
          livenessProbe:
            httpGet: {path: /health/live, port: http}   # process health ONLY — no DB/cache/upstream
            periodSeconds: 20
            failureThreshold: 3
          readinessProbe:
            httpGet: {path: /health/ready, port: http}  # may check dependencies; drains, not restarts
            periodSeconds: 10
            failureThreshold: 3
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities: {drop: [ALL]}
          volumeMounts:
            - {name: tmp, mountPath: /tmp}
      volumes:
        - {name: tmp, emptyDir: {}}
      # soft node spread + hard zone spread with per-revision isolation (see Expert Practices)
      affinity:
        podAntiAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
            - weight: 100
              podAffinityTerm:
                labelSelector: {matchLabels: {app: api-server}}
                topologyKey: kubernetes.io/hostname
      topologySpreadConstraints:
        - maxSkew: 1
          topologyKey: topology.kubernetes.io/zone
          whenUnsatisfiable: DoNotSchedule
          labelSelector: {matchLabels: {app: api-server}}
          matchLabelKeys: [pod-template-hash]   # each rollout revision spreads independently (1.27+)
```

### Service + Ingress

```yaml
apiVersion: v1
kind: Service
metadata: {name: api-server, namespace: production}
spec:
  type: ClusterIP
  selector: {app: api-server}
  ports: [{port: 80, targetPort: http, name: http}]
---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: api-server
  namespace: production
  annotations: {cert-manager.io/cluster-issuer: letsencrypt-prod}
spec:
  ingressClassName: nginx     # canonical since 1.18; kubernetes.io/ingress.class is deprecated
  tls: [{hosts: [api.example.com], secretName: api-tls-cert}]
  rules:
    - host: api.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend: {service: {name: api-server, port: {number: 80}}}
```

### ConfigMap + Secret

```yaml
apiVersion: v1
kind: ConfigMap
metadata: {name: api-config, namespace: production}
data:
  log-level: "info"
  feature-flags: |
    {"new-checkout": true}
---
apiVersion: v1
kind: Secret
metadata: {name: api-secrets, namespace: production}
type: Opaque
# WARNING: Secrets are only base64-encoded, NOT encrypted at rest in etcd by default.
# Enable encryption at rest or use External Secrets Operator. Placeholders only — never commit real creds.
stringData:
  database-url: "postgresql://user:CHANGE_ME@db-host:5432/myapp"
```

### HorizontalPodAutoscaler

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata: {name: api-server, namespace: production}
spec:
  scaleTargetRef: {apiVersion: apps/v1, kind: Deployment, name: api-server}
  minReplicas: 3
  maxReplicas: 10
  metrics:
    - type: Resource
      resource:
        name: cpu
        target: {type: Utilization, averageUtilization: 70}
    # WARNING: memory-based HPA scales out but never back in (memory is non-compressible, runtimes
    # rarely return heap to the OS) — prefer CPU or external/custom metrics (RPS, queue depth via KEDA).
    # Every container MUST set resources.requests.cpu or the HPA silently ignores the pod.
  behavior:
    scaleDown:
      stabilizationWindowSeconds: 300
      policies: [{type: Percent, value: 10, periodSeconds: 60}]
    scaleUp:
      stabilizationWindowSeconds: 0
      policies: [{type: Percent, value: 100, periodSeconds: 15}]
```

### NetworkPolicy (zero-trust) + default-deny

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata: {name: api-server-netpol, namespace: production}
spec:
  podSelector: {matchLabels: {app: api-server}}
  policyTypes: [Ingress, Egress]
  ingress:
    - from:
        # Same list element = namespaceSelector AND podSelector (zero-trust). Separate dashes = OR.
        - namespaceSelector: {matchLabels: {name: ingress-nginx}}
      ports: [{protocol: TCP, port: 8080}]
  egress:
    - to: [{podSelector: {matchLabels: {app: postgresql}}}]
      ports: [{protocol: TCP, port: 5432}]
    - to:
        - namespaceSelector: {matchLabels: {name: kube-system}}
          podSelector: {matchLabels: {k8s-app: kube-dns}}
      ports:
        - {protocol: UDP, port: 53}
        - {protocol: TCP, port: 53}   # required: DNS falls back to TCP for large responses / DNSSEC
---
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata: {name: default-deny-all, namespace: production}
spec:
  podSelector: {}
  policyTypes: [Ingress, Egress]
```

### RBAC (least privilege)

```yaml
apiVersion: v1
kind: ServiceAccount
metadata: {name: api-server, namespace: production}
---
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata: {name: api-server, namespace: production}
rules:
  - apiGroups: [""]
    resources: ["configmaps"]
    verbs: ["get", "list", "watch"]
  - apiGroups: [""]
    resources: ["secrets"]
    resourceNames: ["api-secrets"]   # explicit allowlist; grant only `get` (list/watch leak .data)
    verbs: ["get"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata: {name: api-server, namespace: production}
roleRef: {apiGroup: rbac.authorization.k8s.io, kind: Role, name: api-server}
subjects: [{kind: ServiceAccount, name: api-server, namespace: production}]
```

### StatefulSet with per-pod storage

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata: {name: postgresql, namespace: production}
spec:
  serviceName: postgresql-headless   # required headless Service for stable pod DNS
  replicas: 3
  selector: {matchLabels: {app: postgresql}}
  persistentVolumeClaimRetentionPolicy:
    whenDeleted: Delete              # reclaim PVCs on teardown (default is Retain)
    whenScaled: Retain               # keep data on scale-in
  template:
    metadata:
      labels: {app: postgresql}
    spec:
      containers:
        - name: postgres
          image: postgres:15-alpine
          ports: [{containerPort: 5432, name: postgres}]
          env:
            - {name: PGDATA, value: /var/lib/postgresql/data/pgdata}
          volumeMounts: [{name: data, mountPath: /var/lib/postgresql/data}]
          resources:
            requests: {cpu: 250m, memory: 512Mi}
            limits: {memory: 2Gi}
  volumeClaimTemplates:
    - metadata: {name: data}
      spec:
        accessModes: ["ReadWriteOnce"]
        storageClassName: fast-ssd
        resources: {requests: {storage: 50Gi}}
```

### Helm chart essentials

Structure: `Chart.yaml` (metadata + `dependencies`), `values.yaml` (defaults), `templates/` (`_helpers.tpl` for shared labels/names, resource templates, `NOTES.txt`), `charts/` (deps), `.helmignore`.

```yaml
# templates/_helpers.tpl — standard label block reused via {{ include "app.labels" . }}
{{- define "app.labels" -}}
helm.sh/chart: {{ .Chart.Name }}-{{ .Chart.Version }}
app.kubernetes.io/name: {{ .Chart.Name }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}
```

Test before install: `helm lint`, `helm template`, `helm install --dry-run`. Roll back with `helm rollback <release> <rev>`. Name templates: `... | trunc 63 | trimSuffix "-"` (K8s name limit).

## Troubleshooting Commands

```bash
# Pods
kubectl describe pod <pod> -n <ns>
kubectl logs <pod> -n <ns> [--previous] [-c <container>]
kubectl exec -it <pod> -n <ns> -- /bin/sh
kubectl get events -n <ns> --sort-by='.lastTimestamp'

# Resource usage / scheduling
kubectl top nodes; kubectl top pods -n <ns>
kubectl describe node <node>

# Network debug (ephemeral toolbox)
kubectl run -it --rm debug --image=nicolaka/netshoot --restart=Never -- /bin/bash

# Rollouts
kubectl rollout status|history|undo|restart deployment/<name> -n <ns>

# RBAC
kubectl auth can-i get pods --as=system:serviceaccount:<ns>:<sa> -n <ns>

# Cluster health (componentstatuses deprecated since 1.19)
kubectl get --raw '/livez?verbose'
kubectl get --raw '/readyz?verbose'
```

## Common Issues

| Issue                   | Cause                                            | Fix                                                |
| ----------------------- | ------------------------------------------------ | -------------------------------------------------- |
| ImagePullBackOff        | Registry auth missing or image not found         | Check imagePullSecrets, verify image exists        |
| CrashLoopBackOff        | App crashes on startup                           | Check logs, verify config, add startup probe       |
| Pending pod             | Insufficient resources / scheduling constraints  | Check node capacity, taints, tolerations, affinity |
| OOMKilled (exit 137)    | Memory limit exceeded                            | Raise memory limit or fix leak (memory can't throttle) |
| Service unreachable     | Wrong selector or port                           | Verify selector matches pod labels, check ports    |
| DNS resolution fails    | CoreDNS or NetworkPolicy blocking                | Check CoreDNS pods, verify egress UDP+TCP/53       |
| PVC pending             | StorageClass missing or no volumes               | Verify StorageClass and provisioner                |
| HPA not scaling         | metrics-server missing or no resource requests   | Install metrics-server, set requests.cpu           |

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal practices that separate working-on-my-laptop manifests from production-grade ones. Most of these fail silently — no API error, no event — so they bite only under load, during node maintenance, or in a hardened cluster.

### Probes

**Liveness checks only process health; readiness checks dependencies.** A liveness probe that returns non-200 because a database, cache, or upstream is slow makes the kubelet _restart_ the pod — which fixes nothing, drops in-flight requests, dumps warm cache, and adds load to the already-degraded dependency and the remaining pods. Under load this is a self-reinforcing restart cascade (e.g. `timeoutSeconds: 1` + `failureThreshold: 1` restarts a pod that merely took 2s). Liveness must hit an in-memory handler that only proves the request thread is responsive; dependency-awareness belongs in **readiness**, which drains the pod from Service endpoints _without_ restarting it. Make liveness more tolerant than readiness (higher `failureThreshold` / longer `periodSeconds`) so a pod is drained before it is ever killed.

```yaml
livenessProbe:
  httpGet:
    path: /healthz # in-memory only; no DB/cache/upstream calls
    port: 8080
  periodSeconds: 15
  timeoutSeconds: 5
  failureThreshold: 3
readinessProbe:
  httpGet:
    path: /ready # may check DB connectivity, cache warmth, dependencies
    port: 8080
  periodSeconds: 10
  failureThreshold: 2
```

**Use a startupProbe, not a bloated `initialDelaySeconds`.** Inflating liveness `initialDelaySeconds` to tolerate slow startup makes liveness equally slow to catch a _real_ deadlock at runtime. While a startup probe is configured, Kubernetes does not run liveness or readiness probes until it succeeds. Size it as `failureThreshold * periodSeconds` to cover worst-case startup (e.g. `30 * 10s = 5 min`), then give liveness tight values that only take effect after startup passes.

**Prefer `httpGet`/`tcpSocket`/`grpc` over `exec` probes at high pod density.** `exec` probes fork a new process every execution; at scale with short `periodSeconds` this adds measurable node CPU overhead and probe-latency spikes under pressure. `tcpSocket` is cheapest, `httpGet` moderate. For gRPC use the built-in `grpc` probe type (GA since 1.24) rather than shipping `grpc_health_probe` invoked via `exec`. Reserve `exec` for checks that genuinely need a command, with generous timeouts.

### Resources & Scheduling

**CPU limits cause CFS throttling even when the node has idle CPU.** Limits are enforced by Linux CFS bandwidth control over a 100ms period: a container that exhausts its quota early is throttled to zero for the rest of that period regardless of free node CPU, injecting multi-millisecond latency spikes (severe for GC'd / latency-sensitive runtimes) that are invisible in average-utilization dashboards — watch `container_cpu_cfs_throttled_periods_total`, not `kubectl top`. Tradeoff: `requests == limits` gives Guaranteed QoS (evicted only after BestEffort and Burstable under node pressure) but limits bin-packing; omitting the CPU limit (requests only) maximizes burst headroom. For latency-sensitive services, prefer requests-only or generous limits, but always keep a memory limit (a clean OOM beats node-wide starvation).

```yaml
# Latency-sensitive: request for scheduling, no CPU limit; keep a memory limit
resources:
  requests: { cpu: 200m, memory: 256Mi }
  limits: { memory: 256Mi }
```

**QoS is computed per Pod — one container without `requests == limits` demotes the whole Pod.** Guaranteed (evicted last) requires _every_ container — app, init, sidecar — to set CPU and memory requests equal to limits. A single sidecar (monitoring agent, Envoy/Istio proxy, log shipper) with no resource spec silently drops the entire Pod to Burstable, which is evicted first. Memory is non-compressible: a container exceeding its memory limit is OOMKilled (exit 137) immediately, never throttled like CPU.

**Use `topologySpreadConstraints` for HA spread; isolate rollout revisions with `matchLabelKeys`.** Hard `podAntiAffinity` (`requiredDuringScheduling`, `topologyKey: hostname`) is binary and blocks scheduling once replicas exceed nodes — bad for HPA. The idiomatic pattern is hard zone spread via `topologySpreadConstraints` (`maxSkew: 1`, zone, `DoNotSchedule`) plus soft node spread via `preferred` podAntiAffinity. Without `matchLabelKeys`, a spread constraint counts _all_ Deployment revisions together during a rollout, so new pods can cluster in one zone while the old+new mix still satisfies `maxSkew`. Add `matchLabelKeys: [pod-template-hash]` (beta, on by default since 1.27) so each revision spreads independently.

```yaml
topologySpreadConstraints:
  - maxSkew: 1
    topologyKey: topology.kubernetes.io/zone
    whenUnsatisfiable: DoNotSchedule
    labelSelector:
      matchLabels:
        app: api-server
    matchLabelKeys:
      - pod-template-hash # each rollout revision spreads independently
```

### Autoscaling

**Memory-based HPA is an anti-pattern for load-responsive scaling.** CPU is compressible — an overloaded pod's utilization rises and adding replicas sheds load, a valid signal. Memory is non-compressible and runtimes (JVM, Go, Node.js) rarely return heap to the OS, so utilization creeps up and stays high at rest: the HPA scales out but never scales back in, parking at `maxReplicas`. Two further traps: undersized memory requests make the percentage math run away; and if _any_ container lacks a CPU request the HPA controller silently ignores that pod for CPU metrics (no event). Drive HPA from CPU or external/custom metrics (RPS, queue depth via KEDA); use VPA to right-size memory requests; never run HPA and VPA on the same metric. Reserve memory HPA for the rare case where memory truly tracks load (a cache filling with traffic), and target `AverageValue` (absolute bytes), not `Utilization`.

### Networking

**`from`/`to` list semantics: separate list items OR, selectors within one item AND.** This is the most common NetworkPolicy security bug, and a one-dash indentation change silently flips the security posture with no API error. `namespaceSelector` + `podSelector` as sibling fields in the _same_ list element are ANDed (pods matching the label _and_ in the matching namespace — the zero-trust intent). As _separate_ list elements (each with its own leading dash) they are ORed (any pod in the namespace, _or_ any pod with the label cluster-wide — dangerously over-permissive). Both shapes are accepted silently.

```yaml
ingress:
  - from:
      # AND: only role=client pods that are ALSO in team=frontend namespaces
      - namespaceSelector:
          matchLabels:
            team: frontend
        podSelector:
          matchLabels:
            role: client
```

**NetworkPolicy is a silent no-op without an enforcing CNI.** The API server stores policies regardless of whether the network plugin enforces them — the docs state plainly that a NetworkPolicy without a controller that implements it has no effect, with no error or event. Flannel alone does not enforce NetworkPolicy, so a Flannel-only cluster that looks locked down is wide open. Calico, Cilium, Antrea, and Weave Net do enforce it. Verify empirically after applying:

```bash
kubectl run test --image=busybox --restart=Never -it --rm -- wget -T2 -O- http://target-service
```

**DNS egress must allow BOTH UDP/53 and TCP/53.** A port entry allows exactly one protocol. DNS uses UDP/53 but falls back to TCP/53 for responses over the UDP size limit (many records, DNSSEC) and some resolver configs. UDP-only policies break DNS intermittently and unattributably — always list both.

### Security

**Restricted PSS requires `seccompProfile: RuntimeDefault` (or `Localhost`) — an unset profile is rejected.** Under the Restricted Pod Security Standard `seccompProfile.type` is a restricted field whose only allowed values are `RuntimeDefault` or `Localhost`; Undefined/nil is _not_ allowed (stricter than Baseline). A pod omitting it is rejected by the PodSecurity admission controller in any namespace labeled `pod-security.kubernetes.io/enforce: restricted`. Note: `readOnlyRootFilesystem` is good hardening but is _not_ a PSS-checked control — don't conflate the two. (PodSecurity admission replaced PodSecurityPolicy, removed in 1.25.)

**PodSecurity `enforce` is checked at pod creation, not on the Deployment.** The admission controller evaluates the _pod template_ when the ReplicaSet controller creates pods, not when you `kubectl apply` the Deployment. With only `enforce: restricted`, the Deployment applies cleanly but its pods are silently rejected — a broken rollout that is far harder to debug than a pre-apply warning. Add `warn` and `audit` labels and pin `enforce-version` so violations surface on the workload apply; roll Restricted out in `warn` mode first.

```yaml
metadata:
  labels:
    pod-security.kubernetes.io/enforce: restricted
    pod-security.kubernetes.io/enforce-version: v1.31
    pod-security.kubernetes.io/warn: restricted # surfaces violations on Deployment apply
    pod-security.kubernetes.io/audit: restricted
```

**RBAC `escalate`, `bind`, `impersonate` are cluster-admin-equivalent; `list`/`watch` on secrets leak contents.** These three verbs bypass the protection that stops a user granting rights they don't have: `escalate` lets them author a Role with more rights than they hold, `bind` lets them bind a role whose rights they lack, `impersonate` lets them act as a privileged identity. Treat any holder as having full cluster control. Separately, `list`/`watch` on secrets return the actual `.data` of every secret, not just metadata — grant application service accounts only `get` with an explicit `resourceNames` allowlist.

**`ClusterRoleBinding` grants its role in EVERY namespace.** A benign `pod-reader` ClusterRole bound via a ClusterRoleBinding lets the subject read pods (and any other granted resource, including secrets) in `kube-system` and everywhere else. To grant a ClusterRole's permissions within one namespace, reference it from a _namespaced_ `RoleBinding`. Reserve ClusterRoleBindings for genuinely cluster-scoped needs (nodes, PVs, cluster-wide operators).

**Disable `automountServiceAccountToken` on workloads that never call the API.** Every pod gets an SA token mounted by default, even if the app never talks to the API server. Since 1.22 these are short-lived auto-rotating bound tokens, but they are still a usable scoped API credential handed to anyone who gets code execution in the container. Set `automountServiceAccountToken: false` on the ServiceAccount (or per-Pod, which takes precedence) and re-enable only on pods that genuinely need API access. High-value, low-effort CIS hardening.

### Lifecycle & Availability Gotchas

**A `preStop` sleep absorbs the endpoint-deregistration race — and eats the grace period.** On deletion the kubelet sends SIGTERM (and runs `preStop`) _in parallel_ with the endpoints controller removing the pod from EndpointSlices and kube-proxy asynchronously deleting iptables/IPVS rules on every node. A pod that exits immediately still receives — and RSTs — new connections via stale rules, causing intermittent 502/connection-reset on every rolling update (invisible on single-node clusters). Add a `preStop` sleep (commonly 5–15s) so the pod keeps serving while it deregisters. Timing trap: `terminationGracePeriodSeconds` starts at deletion and the `preStop` sleep is charged against it (SIGTERM is sent only after `preStop` returns), so set grace ≥ preStop sleep + app drain + margin or SIGKILL truncates the drain. Hooks are at-least-once — keep them idempotent.

```yaml
terminationGracePeriodSeconds: 60 # 5s preStop + ~45s drain + margin
containers:
  - name: app
    lifecycle:
      preStop:
        exec:
          command: ["/bin/sh", "-c", "sleep 5"] # serve through endpoint deregistration
```

**StatefulSet PVCs are retained by default — opt into deletion explicitly.** Deleting or scaling down a StatefulSet does _not_ delete PVCs from `volumeClaimTemplates` (by design, for data safety), so they accumulate cost indefinitely. Kubernetes 1.27 graduated `persistentVolumeClaimRetentionPolicy` to beta with `whenDeleted`/`whenScaled` knobs, but both still default to `Retain`. Use `whenScaled: Retain` / `whenDeleted: Delete` for production. Separately, the default `OrderedReady` podManagementPolicy serializes rollouts: if pod N fails readiness, pods 0..N-1 are not updated — a stuck pod deadlocks the rollout until deleted deliberately.

**Set `unhealthyPodEvictionPolicy: AlwaysAllow` on PDBs to prevent node-drain deadlocks.** The default `IfHealthyBudget` only evicts unhealthy (e.g. crash-looping) pods if the budget is already satisfied, so a node drain can block indefinitely waiting for a broken app to become healthy — a silent incident during maintenance and upgrades. `AlwaysAllow` evicts already-unhealthy pods regardless of budget while still protecting healthy ones; the docs recommend it. The GA field requires Kubernetes **1.31+** (alpha 1.26, beta 1.27).

**PDB v1 empty selector matches ALL pods — a silent inversion from v1beta1.** `policy/v1beta1` PDB was removed in 1.25; use `policy/v1`. Critical behavior change: in v1beta1 an empty selector (`{}`) matched _zero_ pods, but in v1 it matches _every_ pod in the namespace. A migrated manifest still carrying `selector: {}` silently becomes a namespace-wide constraint blocking all voluntary disruptions. Always set an explicit selector.

### Currency (deprecated patterns to avoid)

- `kubernetes.io/ingress.class` annotation → use `spec.ingressClassName` (deprecated since 1.18).
- `kubectl get componentstatuses` (`cs`) → use `kubectl get --raw '/livez?verbose'` / `'/readyz?verbose'` (deprecated since 1.19; reports false "Unhealthy" for scheduler/controller-manager).
- `policy/v1beta1` PodDisruptionBudget and `PodSecurityPolicy` → both removed in 1.25; use `policy/v1` PDB and PodSecurity admission labels.

## Verification Checklist

- [ ] Liveness hits an in-memory endpoint (no deps); readiness carries dependency checks; liveness more tolerant than readiness; startupProbe covers slow starts.
- [ ] Every container (incl. sidecars/init) sets `requests.cpu`; memory limit present; CPU limit omitted or generous for latency-sensitive services.
- [ ] HA spread via `topologySpreadConstraints` + `matchLabelKeys: [pod-template-hash]`; not hard hostname anti-affinity.
- [ ] Hardened pods: `runAsNonRoot`, `seccompProfile: RuntimeDefault`, `drop: [ALL]`, `readOnlyRootFilesystem`, `allowPrivilegeEscalation: false`.
- [ ] PSS labels include `warn`+`audit` (not just `enforce`); rolled out in `warn` first.
- [ ] NetworkPolicy default-deny present, enforcing CNI verified, DNS egress allows UDP+TCP/53, AND-vs-OR selector shape checked.
- [ ] RBAC least-privilege: secrets `get` + `resourceNames` allowlist; no stray `escalate`/`bind`/`impersonate`; ClusterRoleBindings justified.
- [ ] `preStop` drain + adequate `terminationGracePeriodSeconds`; PDB with explicit selector and `unhealthyPodEvictionPolicy: AlwaysAllow` (1.31+).
- [ ] No deprecated APIs (`policy/v1beta1`, PSP, `ingress.class` annotation).
