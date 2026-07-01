---
name: loom-karpenter
description: Kubernetes node autoscaling and cost optimization with Karpenter. Use for node provisioning, spot instance management, cluster right-sizing, node consolidation, NodePool/EC2NodeClass configuration, disruption budgets, and multi-architecture support.
triggers:
  - karpenter
  - node autoscaling
  - nodepool
  - ec2nodeclass
  - provisioner
  - spot instances
  - on-demand instances
  - node consolidation
  - node termination
  - cluster autoscaling
  - right-sizing
  - capacity-type
  - node disruption
  - compute costs
  - instance selection
  - graviton
  - arm64
allowed-tools: Read, Edit, Write, Bash
---

# Karpenter

## Overview

Karpenter provisions right-sized nodes directly from cloud-provider APIs based on aggregate pending-pod resource requests — no node groups, sub-minute scale-up, active consolidation to cheaper nodes. vs Cluster Autoscaler: no predefined ASGs, picks from all instance types, bin-packs, first-class spot (on-demand fallback is opt-in, NOT automatic).

**Core dependency:** accurate pod `resources.requests` — Karpenter bin-packs against requests (limits are ignored for scheduling). **Instance-type flexibility is the engine of bin-packing and consolidation; over-constraining families defeats both** and narrows the spot pool.

> This skill targets the **v1 API** (Karpenter 1.0+): `NodePool` = `karpenter.sh/v1`, `EC2NodeClass` = `karpenter.k8s.aws/v1`. The pre-v1 `Provisioner`/`AWSNodeTemplate` and `v1beta1` are gone (see Currency).

## Examples

### 1. Basic NodePool (broad, flexible)

```yaml
apiVersion: karpenter.sh/v1
kind: NodePool
metadata: {name: default}
spec:
  template:
    spec:
      # v1 requires group AND kind alongside name (no default fallback → NotReady).
      nodeClassRef:
        group: karpenter.k8s.aws
        kind: EC2NodeClass
        name: default
      requirements:
        - {key: kubernetes.io/arch, operator: In, values: ["amd64", "arm64"]}
        # Prefer instance-category + generation over a fixed family list (broader
        # spot pool, auto-adopts new generations — the official default).
        - {key: karpenter.k8s.aws/instance-category, operator: In, values: ["c", "m", "r"]}
        - {key: karpenter.k8s.aws/instance-generation, operator: Gt, values: ["2"]}
        # Both types in ONE NodePool = on-demand fallback when spot is exhausted.
        - {key: karpenter.sh/capacity-type, operator: In, values: ["spot", "on-demand"]}
      # expireAfter is a v1 TEMPLATE field and is drift-able (changing it rolls nodes).
      expireAfter: 720h
      taints: [{key: workload-type, value: general, effect: NoSchedule}]
  limits: {cpu: 1000, memory: 1000Gi}   # soft, eventually-consistent, per-NodePool (see Gotchas)
  disruption:
    consolidationPolicy: WhenEmptyOrUnderutilized   # v1 name; WhenUnderutilized is rejected
    consolidateAfter: 30s
    budgets: [{nodes: "10%", duration: 5m}]
  weight: 10
```

### 2. EC2NodeClass (AWS specifics)

```yaml
apiVersion: karpenter.k8s.aws/v1
kind: EC2NodeClass
metadata: {name: default}
spec:
  # REQUIRED in v1 (unless amiFamily: Custom). Pin alias family@version so AMI
  # rollouts go through drift, not automatically on every AWS release. Use al2023
  # or bottlerocket: EKS stopped publishing AL2 AMIs on 2025-11-26 (k8s 1.32 was last).
  amiSelectorTerms:
    - alias: al2023@v20240807
  # kubelet lives on EC2NodeClass in v1 (moved from NodePool). NodePools needing
  # distinct kubelet config each need their own EC2NodeClass.
  kubelet:
    maxPods: 110
    systemReserved: {cpu: 100m, memory: 100Mi, ephemeral-storage: 1Gi}
    evictionHard: {memory.available: 5%, nodefs.available: 10%}
  role: KarpenterNodeRole-my-cluster
  subnetSelectorTerms: [{tags: {karpenter.sh/discovery: my-cluster}}]
  securityGroupSelectorTerms: [{tags: {karpenter.sh/discovery: my-cluster}}]
  # Do NOT call /etc/eks/bootstrap.sh — Karpenter injects it (AL2); on AL2023
  # Karpenter-owned fields (maxPods/labels/taints) override userData regardless.
  userData: |
    #!/bin/bash
    echo 'fs.inotify.max_user_watches=524288' >> /etc/sysctl.d/99-custom.conf
    sysctl -p /etc/sysctl.d/99-custom.conf
  blockDeviceMappings:
    - deviceName: /dev/xvda
      ebs: {volumeSize: 100Gi, volumeType: gp3, iops: 3000, throughput: 125, encrypted: true, deleteOnTermination: true}
  # v1 default hopLimit is 1, blocking non-hostNetwork pods from IMDS. Give such
  # pods IRSA/Pod Identity rather than raising this to 2 (see Security).
  metadataOptions: {httpEndpoint: enabled, httpPutResponseHopLimit: 1, httpTokens: required}
  tags: {Environment: production, ManagedBy: karpenter}
```

### 3. Specialized NodePools (distinguishing config only)

Same `nodeClassRef`/structure as Example 1; the workload class is expressed via requirements + disruption + taints:

```yaml
# GPU — on-demand, conservative consolidation, GPU taint
requirements:
  - {key: karpenter.k8s.aws/instance-family, operator: In, values: ["g5", "g6", "p4", "p5"]}
  - {key: karpenter.sh/capacity-type, operator: In, values: ["on-demand"]}
  - {key: karpenter.k8s.aws/instance-gpu-count, operator: Gt, values: ["0"]}
taints: [{key: nvidia.com/gpu, value: "true", effect: NoSchedule}]
disruption: {consolidationPolicy: WhenEmpty, consolidateAfter: 300s}
limits: {nvidia.com/gpu: 16}

# Batch — spot-only (NO fallback; pods Pending if spot exhausted), aggressive budget
requirements:
  - {key: karpenter.sh/capacity-type, operator: In, values: ["spot"]}
disruption:
  consolidationPolicy: WhenEmpty
  budgets: [{nodes: "20%"}]

# Stateful — on-demand only, memory-optimized, very conservative disruption
requirements:
  - {key: karpenter.sh/capacity-type, operator: In, values: ["on-demand"]}
  - {key: karpenter.k8s.aws/instance-family, operator: In, values: ["r6i", "r7i"]}
disruption:
  consolidationPolicy: WhenEmpty
  consolidateAfter: 600s
  budgets: [{nodes: "1", duration: 30m}]
```

### 4. Disruption budgets (scheduled + reason-scoped)

```yaml
disruption:
  consolidationPolicy: WhenEmptyOrUnderutilized
  consolidateAfter: 30s
  # SCHEDULES ARE UTC-ONLY; overlapping windows → Karpenter takes the MINIMUM.
  # NotReady + deleting nodes also consume budget (see Gotchas).
  budgets:
    - {nodes: "5%",  duration: 8h,  schedule: "0 8 * * MON-FRI"}   # business hours
    - {nodes: "20%", duration: 16h, schedule: "0 18 * * MON-FRI"}  # off-hours
    - {nodes: "10%"}                                               # default, always active
    - {nodes: "20%", reasons: ["Empty"]}                           # always allow idle-node removal
```

Pair `expireAfter` with `terminationGracePeriod` — v1 expiration is **forceful and NOT budget-limited**:

```yaml
spec:
  template:
    spec:
      expireAfter: 720h
      terminationGracePeriod: 1h
```

### 5. Pod scheduling hooks for Karpenter

```yaml
spec:
  # Route to a NodePool via matching toleration + nodeSelector
  tolerations: [{key: workload-type, operator: Equal, value: general, effect: NoSchedule}]
  nodeSelector: {workload-type: general}
  # ⚠ Karpenter treats PREFERRED affinity as required on the first pass (see Gotchas) —
  # use topologySpreadConstraints for correctness-critical spread.
  topologySpreadConstraints:
    - {maxSkew: 1, topologyKey: topology.kubernetes.io/zone, whenUnsatisfiable: ScheduleAnyway,
       labelSelector: {matchLabels: {app: my-application}}}
  containers:
    - name: app
      resources:                       # CRITICAL: accurate requests drive bin-packing
        requests: {cpu: 500m, memory: 1Gi}
        limits: {memory: 1Gi}          # memory req≈limit under consolidation (see Gotchas)
      lifecycle:
        preStop: {exec: {command: ["/bin/sh", "-c", "sleep 15"]}}   # drain on spot interruption
  terminationGracePeriodSeconds: 30
```

### 6. Spot with fallback + multi-arch (wide pool)

```yaml
requirements:
  # Both types in one NodePool = on-demand fallback (spot-only leaves pods Pending).
  - {key: karpenter.sh/capacity-type, operator: In, values: ["spot", "on-demand"]}
  - {key: kubernetes.io/arch, operator: In, values: ["amd64", "arm64"]}  # Graviton ~20% cheaper
  # Wide category+generation selection > fixed family list for spot depth.
  - {key: karpenter.k8s.aws/instance-category, operator: In, values: ["c", "m", "r"]}
  - {key: karpenter.k8s.aws/instance-generation, operator: Gt, values: ["2"]}
disruption:
  consolidationPolicy: WhenEmptyOrUnderutilized
  budgets: [{nodes: "25%"}]   # spot churns anyway
weight: 5                     # lower weight → tried after more-specific pools
# NOTE: spot-to-spot consolidation is a controller feature gate
# (settings.featureGates.spotToSpotConsolidation via Helm), NOT a NodePool annotation.
```

### 7. Protect a workload with a PDB (respected during consolidation)

```yaml
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata: {name: critical-service-pdb}
spec:
  minAvailable: 4
  selector: {matchLabels: {app: critical-service}}
# Karpenter will not disrupt a node if doing so violates the PDB (voluntary disruptions only).
```

## Expert Practices: Idioms, Anti-Patterns & Gotchas

### Currency (v1 API — Karpenter 1.0+)

- **Use the v1 APIs exclusively.** Karpenter 1.0 graduated `NodePool` to `karpenter.sh/v1` and `EC2NodeClass` to `karpenter.k8s.aws/v1`; **1.1 dropped `v1beta1` entirely** (the conversion webhooks are gone). A `v1beta1` manifest is **rejected** on Karpenter >= 1.1 — this is a hard break, not a deprecation warning. The v1 APIs carry a compatibility guarantee across the 1.x line.

- **`nodeClassRef` requires `group` + `kind` + `name`.** v1 renamed the old `apiVersion` key to `group`, and as of v1.1.0 `group` and `kind` are strictly required alongside `name`. A ref with only `name` leaves the NodePool **NotReady** — there is no default fallback.

  ```yaml
  nodeClassRef:
    group: karpenter.k8s.aws
    kind: EC2NodeClass
    name: default
  ```

- **`kubelet` config moved from NodePool to `EC2NodeClass.spec.kubelet`** (maxPods, podsPerCore, systemReserved, evictionHard, imageGC thresholds). A `kubelet` block left on a NodePool is invalid. Because many NodePools share one EC2NodeClass, **NodePools that need distinct kubelet config each need their own EC2NodeClass.** The `compatibility.karpenter.sh/v1beta1-kubelet-conversion` migration annotation was dropped in 1.1, so anything relying on it **silently loses kubelet config** after the upgrade.

- **`consolidationPolicy: WhenUnderutilized` was renamed to `WhenEmptyOrUnderutilized`** (old value rejected). And **`expireAfter` moved** from `spec.disruption` to `spec.template.spec.expireAfter` and is now **drift-able**: changing it triggers Drift and rolling replacement of running nodes (in v1beta1 it was a no-op on existing nodes). Pair it with `spec.template.spec.terminationGracePeriod` — v1 expiration is **forceful and NOT rate-limited by disruption budgets.**

### Anti-Patterns

- **`amiSelectorTerms` is required in v1** (unless `amiFamily: Custom`); omitting it leaves the EC2NodeClass and every referencing NodePool **NotReady**. An `alias` term cannot be combined with other term types and must match the `amiFamily`. **Pin `alias: family@version` in production** — `family@latest` rolls every node whenever AWS publishes a new EKS-optimized AMI, so an untested AMI can break workloads with no operator action. Use **al2023** or **bottlerocket** for new clusters: k8s 1.32 was the last version with EKS AL2 AMIs, and EKS stopped publishing them on 2025-11-26. (The AL2 base OS itself is supported until 2026-06-30 — it has not reached EOL.)

- **Never run the Karpenter controller on a Karpenter-managed node.** A spot interruption, consolidation, or expiry can terminate the controller before it provisions its replacement — a circular dependency where no controller is up to launch a node and no node exists to host the controller. Run it on **EKS Fargate** (a Fargate profile for the `karpenter` namespace) or a **static managed node group Karpenter does not manage**, pinned via `nodeSelector`/tolerations.

- **Make NodePools mutually exclusive or weighted.** AWS: "if multiple NodePools are matched, Karpenter will randomly choose which to use, causing unexpected results." Enforce routing with **taints on the NodePool + matching tolerations** (hard isolation, e.g. GPU pools) or distinct **`weight`** values (preference ordering with fallback).

- **Do not call `/etc/eks/bootstrap.sh` in custom `userData` (AL2)** — Karpenter already injects it, so a second call reconfigures an already-running kubelet, init fails, and **the node never joins** despite appearing to start. On **AL2023**, userData is merged as NodeConfig and Karpenter-owned fields (maxPods, labels, taints) override userData — set those via native spec fields, not userData.

### Gotchas

- **`httpPutResponseHopLimit` defaults to 1 in v1** (was 2). This deliberately prevents non-`hostNetwork` pods from reaching IMDS (169.254.169.254) — the response TTL expires crossing the container netns. Any pod calling IMDS directly (SDK credential chaining, region/AZ detection) then **silently fails**. **Fix with IRSA or EKS Pod Identity**, not by raising the hop limit to 2 (that re-exposes IMDS to all containers — a credential-theft surface). Raise to 2 only as a deliberate, scoped exception.

- **Set memory `requests` = `limits` when consolidation is enabled.** Karpenter bin-packs against requests; limits are ignored. After `WhenEmptyOrUnderutilized` packs pods tightly, pods whose memory limit exceeds their request can all burst at once and **OOM-kill neighbors**. Incompressible resources (memory, ephemeral-storage, GPU/hugepages) want requests ≈ working set / equal to limits; CPU is compressible (throttled, not killed), so `requests != limits` is fine there.

- **`karpenter.sh/do-not-disrupt` only blocks *voluntary* disruption** (consolidation, voluntary drift). It does **NOT** block Expiration, Interruption, Node Repair, or manual deletion. Since v1 made expiration forceful, a long-running pod relying solely on this annotation is still terminated when the node's TTL fires — use `terminationGracePeriod` + SIGTERM handling for lifetime guarantees. The value must be empty/`"true"` or a valid Go duration; an invalid value (e.g. `"30 minutes"`) is **silently ignored** with only a Kubernetes event.

- **Disruption budget math subtracts deleting AND NotReady nodes:** `allowed = roundup(total * pct) - deleting - notready`. A cluster under resource pressure can resolve to **0 allowed disruptions** and block all consolidation with nothing intentional in flight. With multiple active budget windows Karpenter takes the **minimum**. **Schedules are UTC-only** (no timezone) — `0 8 * * MON-FRI` fires at 08:00 UTC. Forceful methods (expiration, interruption) are never budget-limited.

- **Spot-to-spot consolidation needs the feature gate AND >= 15 instance types.** Enable via Helm `settings.featureGates.spotToSpotConsolidation=true` (controller-level — **there is no `karpenter.sh/spot-to-spot-consolidation` NodePool annotation; it is fabricated and does nothing**). Even enabled, single-node spot-to-spot consolidation requires >= 15 cheaper qualifying instance types or Karpenter logs `requires 15 cheaper instance type options ... got N` and skips. Over-constraining instance families silently disables the optimization.

- **`spec.limits` is a soft, eventually-consistent cap** — during a burst, parallel provisioning decisions can each see room and all launch, transiently overshooting it. Limits are **per NodePool only** (no cluster-wide limit). When hit, Karpenter writes `resource usage of X exceeds limit of Y` to **controller logs only** (no Kubernetes event) — detect overrun with a CloudWatch Logs metric filter + a billing alarm. Treat limits + billing alarms as the cost guardrail, not a hard spend cap.

- **Karpenter treats *preferred* affinity as *required* on the first scheduling pass**, relaxing preferences one at a time only if requirements can't be met (unlike kube-scheduler, which treats them as soft against existing nodes). A pod with `preferredDuringScheduling` pod-anti-affinity can therefore make Karpenter **provision a NEW node** instead of using an underutilized one — costly for overprovisioning/headroom placeholders. (This does NOT apply to topology spread.) If spreading is required for correctness, use `requiredDuringScheduling` affinity or `topologySpreadConstraints` with `DoNotSchedule`.

### Idioms

- **Prefer `instance-category` + `instance-generation` over fixed `instance-family` lists.** The official default NodePool selects `instance-category In [c, m, r]` and `instance-generation Gt 2`. This keeps the spot pool broad (Price-Capacity-Optimized draws from the deepest pools → lower interruption risk) and auto-adopts new generations without editing the manifest. A short family list is rigid and narrows the spot pool.

  ```yaml
  requirements:
    - key: karpenter.k8s.aws/instance-category
      operator: In
      values: ["c", "m", "r"]
    - key: karpenter.k8s.aws/instance-generation
      operator: Gt
      values: ["2"]
  ```

- **Enable native interruption handling via SQS; do not also run Node Termination Handler.** Point the controller at an SQS queue fed by EventBridge rules (`--interruption-queue` / Helm `settings.interruptionQueue`). It proactively taints/drains/replaces nodes on spot notices, scheduled maintenance, and stop/terminate events, launching a replacement in parallel with the drain on the 2-minute spot notice. Running **aws-node-termination-handler alongside it drains the same node twice** (conflicting taints, excessive churn) — use one or the other.

- **Scope disruption budgets by `reasons`** (`Drifted`, `Underutilized`, `Empty`; omitted = all voluntary reasons). Rate-limit causes independently — e.g. freeze drift-driven AMI rollouts during business hours while still allowing empty-node cleanup:

  ```yaml
  disruption:
    consolidationPolicy: WhenEmptyOrUnderutilized
    consolidateAfter: 30s
    budgets:
      - nodes: "0"
        schedule: "0 9 * * mon-fri" # UTC
        duration: 8h
      - nodes: "20%"
        reasons: ["Empty"] # always allow idle-node removal
  ```

### Private / Air-Gapped Clusters

- A private cluster needs a regional **STS VPC endpoint** (Karpenter uses IRSA; missing → `WebIdentityErr: failed to retrieve credentials`) and an **SSM VPC endpoint** (queries SSM for EKS-optimized AMI IDs and to hydrate the launch-template cache; missing → `Unable to hydrate the AWS launch template cache`). There is **no VPC endpoint for the Price List API** — Karpenter ships on-demand pricing in its binary and only refreshes it on upgrade (logs `retreiving on-demand pricing data ... i/o timeout`), so plan upgrade cadence to refresh pricing in air-gapped environments. Only **two** endpoints are required; pricing degrades gracefully to stale data.

## Monitoring & Troubleshooting

```text
# v1 metrics ("provisioner" metrics were removed). Cross-check names against the
# live /metrics endpoint + https://karpenter.sh/docs/reference/metrics/ (change per release).
karpenter_nodes_created_total / karpenter_nodes_terminated_total
karpenter_scheduler_scheduling_duration_seconds
karpenter_nodepools_allowed_disruptions
karpenter_voluntary_disruption_decisions_total
karpenter_pods_state
```

| Symptom                       | First checks                                                                            |
| ----------------------------- | -------------------------------------------------------------------------------------- |
| Pods stuck Pending            | NodePool requirements vs pod selectors/tolerations; cloud limits; subnet/AZ capacity   |
| Excessive node churn          | Raise `consolidateAfter`; tighten disruption budgets; verify request accuracy; `WhenEmpty` |
| High cost despite Karpenter   | Consolidation enabled? spot actually used? oversized requests? widen instance variety  |
| Spot interruptions hurt SLA   | Add PDBs, wider instance diversity, more replicas, `preStop` drain                      |
| NodePool `NotReady`           | Missing `amiSelectorTerms`, incomplete `nodeClassRef` (group+kind+name), or stray `kubelet` on NodePool |

## Terraform Install (Helm)

```hcl
resource "helm_release" "karpenter" {
  namespace        = "karpenter"
  create_namespace = true
  name             = "karpenter"
  repository       = "oci://public.ecr.aws/karpenter"
  chart            = "karpenter"
  version          = "1.1.1" # pin a current 1.x release (v1 APIs)
  values = [<<-EOT
    settings:
      clusterName: ${var.cluster_name}
      clusterEndpoint: ${var.cluster_endpoint}
      # Native interruption handling — feed this SQS queue from EventBridge.
      # Do NOT also run aws-node-termination-handler (double-drain churn).
      interruptionQueue: ${var.interruption_queue_name}
      featureGates:
        spotToSpotConsolidation: true   # controller-level, not per-NodePool
    serviceAccount:
      annotations:
        eks.amazonaws.com/role-arn: ${var.karpenter_irsa_arn}
    controller:
      resources:
        requests: {cpu: 1, memory: 1Gi}
        limits: {cpu: 2, memory: 2Gi}
    EOT
  ]
  depends_on = [aws_iam_role_policy_attachment.karpenter_controller]
}
# Apply NodePool/EC2NodeClass via kubectl_manifest resources depending on this release.
```

**Migration from Cluster Autoscaler:** deploy Karpenter alongside CA with distinctly-labeled NodePools → migrate workloads incrementally (add tolerations/nodeSelectors, watch provisioning + cost) → scale down and remove CA node groups once fully migrated.

## Verification Checklist

- [ ] All manifests use v1 APIs; `nodeClassRef` has group+kind+name; `kubelet` on EC2NodeClass not NodePool.
- [ ] `amiSelectorTerms` present and pinned to `family@version` (al2023/bottlerocket); not `@latest`.
- [ ] `consolidationPolicy` uses `WhenEmptyOrUnderutilized`/`WhenEmpty`; `expireAfter` paired with `terminationGracePeriod`.
- [ ] Instance selection is broad (category+generation, both arches) — not over-constrained; spot NodePools include `on-demand` for fallback (or a weighted on-demand pool exists).
- [ ] Pod `resources.requests` accurate; memory requests == limits where consolidation is on.
- [ ] Disruption budgets set; UTC schedules understood; NotReady/deleting budget math accounted for.
- [ ] Native SQS interruption handling on; aws-node-termination-handler NOT also running.
- [ ] Controller runs on Fargate or an unmanaged node group (not a Karpenter node).
- [ ] IMDS hop-limit left at 1 + IRSA/Pod Identity for pods needing AWS creds; PDBs protect critical workloads.
