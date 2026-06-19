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

Karpenter is a Kubernetes node autoscaler that provisions right-sized compute resources in response to changing application load. Unlike Cluster Autoscaler which scales predefined node groups, Karpenter provisions nodes based on aggregate pod resource requirements, enabling better bin-packing and cost optimization.

### Key Differences from Cluster Autoscaler

- **Direct provisioning**: Talks directly to cloud provider APIs (no node groups required)
- **Fast scaling**: Provisions nodes in seconds vs minutes
- **Flexible instance selection**: Chooses from all available instance types automatically
- **Consolidation**: Actively replaces nodes with cheaper alternatives
- **Spot instance optimization**: First-class support (on-demand fallback is opt-in, not automatic — see Spot Instance Management)

### When to Use Karpenter

- Running workloads with diverse resource requirements
- Need for fast scaling (sub-minute response)
- Cost optimization with spot instances and Graviton (ARM64)
- Consolidation to reduce cluster waste and over-provisioning
- Clusters with unpredictable or bursty workloads
- Right-sizing infrastructure to actual usage patterns
- Managing mixed capacity types (spot/on-demand) automatically

## Instructions

### 1. Installation and Setup

- Install Karpenter controller in cluster
- Configure cloud provider credentials (IAM roles)
- Set up instance profiles and security groups
- Create NodePools for different workload types
- Define EC2NodeClass (AWS) or equivalent for your provider

### 2. Design NodePool Strategy

- Separate NodePools for different workload classes
- Define instance type families and sizes
- Configure spot/on-demand mix
- Set resource limits per NodePool
- Plan for multi-AZ distribution

### 3. Configure Disruption Management

- Set disruption budgets to control churn
- Configure consolidation policies
- Define expiration windows for node lifecycle
- Handle workload-specific disruption constraints
- Test disruption scenarios

### 4. Optimize for Cost and Performance

- Enable consolidation for cost savings
- Use spot instances with fallback strategies
- Set appropriate resource requests on pods (Karpenter depends on accurate requests)
- Monitor node utilization and waste
- Adjust instance type restrictions based on usage
- Leverage Graviton (ARM64) instances for 20% cost reduction
- Configure capacity-type weighting to prefer spot over on-demand

### 5. Cost Optimization Strategies

- **Spot instances**: Configure 70-90% spot mix for fault-tolerant workloads
- **Graviton (ARM64)**: Use c7g, m7g, r7g families for lower costs
- **Consolidation**: Enable WhenEmptyOrUnderutilized policy to replace expensive nodes
- **Instance diversity**: Wide instance family selection improves spot availability
- **Right-sizing**: Let Karpenter bin-pack efficiently instead of over-provisioning

### 6. Spot Instance Management

- Use wide instance type selection (10+ families) for better spot availability
- Fallback to on-demand is NOT automatic: include both `spot` and `on-demand` in one NodePool's `capacity-type` (or run a lower-weight on-demand NodePool) — a spot-only NodePool leaves pods Pending when spot is exhausted
- Implement Pod Disruption Budgets to control blast radius
- Set graceful termination handlers in applications (preStop hooks)
- Monitor spot interruption rates and adjust instance selection
- Use diverse availability zones to reduce correlated failures

### 7. Node Consolidation

- **WhenEmptyOrUnderutilized**: Replaces nodes with cheaper/smaller alternatives actively (v1 name; the old `WhenUnderutilized` is rejected)
- **WhenEmpty**: Only consolidates completely empty nodes (conservative)
- Configure consolidateAfter delay to prevent churn (30s-600s typical)
- Use disruption budgets to limit consolidation rate (5-20% per window)
- Respect Pod Disruption Budgets during consolidation
- Set expiration windows to force periodic node refresh

## Best Practices

1. **Start Conservative**: Begin with restrictive instance types, expand based on observation
2. **Use Disruption Budgets**: Prevent too many nodes from being disrupted simultaneously
3. **Set Pod Resource Requests**: Karpenter relies on accurate requests for scheduling
4. **Enable Consolidation**: Let Karpenter optimize node utilization automatically
5. **Separate Workload Classes**: Use multiple NodePools for different requirements
6. **Monitor Provisioning**: Track provisioning latency and failures
7. **Test Spot Interruptions**: Ensure graceful handling of spot instance terminations
8. **Use Topology Spread**: Combine with pod topology constraints for availability

## Examples

### Example 1: Basic NodePool with Multiple Instance Types

```yaml
apiVersion: karpenter.sh/v1
kind: NodePool
metadata:
  name: default
spec:
  # Template for nodes created by this NodePool
  template:
    spec:
      # Reference to EC2NodeClass (AWS-specific configuration).
      # In v1, group AND kind are required alongside name (see Gotchas).
      nodeClassRef:
        group: karpenter.k8s.aws
        kind: EC2NodeClass
        name: default

      # Requirements that constrain instance selection
      requirements:
        # Use amd64 or arm64 architectures
        - key: kubernetes.io/arch
          operator: In
          values: ["amd64", "arm64"]

        # Prefer instance-category + generation over a fixed family list
        # (broader spot pool, auto-adopts new generations — see Idioms).
        - key: karpenter.k8s.aws/instance-category
          operator: In
          values: ["c", "m", "r"]
        - key: karpenter.k8s.aws/instance-generation
          operator: Gt
          values: ["2"]

        # Allow a range of instance sizes
        - key: karpenter.k8s.aws/instance-size
          operator: In
          values: ["large", "xlarge", "2xlarge", "4xlarge"]

        # Use spot, falling back to on-demand (both types required for fallback)
        - key: karpenter.sh/capacity-type
          operator: In
          values: ["spot", "on-demand"]

        # Spread across availability zones
        - key: topology.kubernetes.io/zone
          operator: In
          values: ["us-west-2a", "us-west-2b", "us-west-2c"]

      # NOTE: kubelet config lives on EC2NodeClass.spec.kubelet in v1,
      # NOT here on the NodePool (see Corrections). expireAfter is also
      # a template field in v1 and is drift-able.
      expireAfter: 720h

      # Taints and labels
      taints:
        - key: workload-type
          value: general
          effect: NoSchedule

      # Metadata applied to nodes
      metadata:
        labels:
          workload-type: general
          managed-by: karpenter

  # Limits for this NodePool
  limits:
    cpu: 1000
    memory: 1000Gi

  # Disruption controls
  disruption:
    # Consolidation policy (v1: WhenEmpty or WhenEmptyOrUnderutilized)
    consolidationPolicy: WhenEmptyOrUnderutilized

    # Time window for when disruptions are allowed
    consolidateAfter: 30s

    # Budgets control the rate of (voluntary) disruptions
    budgets:
      - nodes: "10%"
        duration: 5m

  # Node weight for scheduling decisions (higher = preferred)
  weight: 10
```

### Example 2: EC2NodeClass for AWS-Specific Configuration

```yaml
apiVersion: karpenter.k8s.aws/v1
kind: EC2NodeClass
metadata:
  name: default
spec:
  # AMI selection is REQUIRED in v1 (unless amiFamily is Custom).
  # Pin alias family@version in production so AMI rollouts are controlled
  # via drift, not triggered automatically on every AWS release. Use al2023
  # (or bottlerocket): EKS stopped publishing AL2 AMIs on 2025-11-26 (k8s
  # 1.32 was the last version with them).
  amiSelectorTerms:
    - alias: al2023@v20240807

  # Alternative: select by id/tags instead of an alias (cannot combine
  # an alias term with other term types):
  # amiSelectorTerms:
  #   - id: ami-0123456789abcdef0
  #   - tags:
  #       karpenter.sh/discovery: my-cluster

  # Kubelet configuration lives on EC2NodeClass in v1 (moved from NodePool).
  # NodePools needing distinct kubelet config each need their own EC2NodeClass.
  kubelet:
    maxPods: 110
    systemReserved:
      cpu: 100m
      memory: 100Mi
      ephemeral-storage: 1Gi
    evictionHard:
      memory.available: 5%
      nodefs.available: 10%
    imageGCHighThresholdPercent: 85
    imageGCLowThresholdPercent: 80

  # IAM role for nodes (instance profile)
  role: KarpenterNodeRole-my-cluster

  # Subnet selection - use tags to identify subnets
  subnetSelectorTerms:
    - tags:
        karpenter.sh/discovery: my-cluster
        kubernetes.io/role/internal-elb: "1"

  # Security group selection
  securityGroupSelectorTerms:
    - tags:
        karpenter.sh/discovery: my-cluster
    - name: my-cluster-node-security-group

  # User data for node initialization.
  # Do NOT call /etc/eks/bootstrap.sh — Karpenter already injects it (AL2),
  # and on AL2023 Karpenter-owned fields (maxPods, labels, taints) override
  # userData regardless. Use this only for extra OS-level tuning.
  userData: |
    #!/bin/bash
    echo 'fs.inotify.max_user_watches=524288' >> /etc/sysctl.d/99-custom.conf
    sysctl -p /etc/sysctl.d/99-custom.conf

  # Block device mappings for EBS volumes
  blockDeviceMappings:
    - deviceName: /dev/xvda
      ebs:
        volumeSize: 100Gi
        volumeType: gp3
        iops: 3000
        throughput: 125
        encrypted: true
        deleteOnTermination: true

  # Metadata options for IMDS. v1 default hopLimit is 1, which blocks
  # non-hostNetwork pods from reaching IMDS — give such pods IRSA/Pod
  # Identity instead of raising this to 2 (see Security).
  metadataOptions:
    httpEndpoint: enabled
    httpProtocolIPv6: disabled
    httpPutResponseHopLimit: 1
    httpTokens: required

  # Detailed monitoring
  detailedMonitoring: true

  # Tags applied to EC2 instances
  tags:
    Name: karpenter-node
    Environment: production
    ManagedBy: karpenter
    ClusterName: my-cluster
```

### Example 3: Specialized NodePools for Different Workloads

```yaml
---
# GPU workload NodePool
apiVersion: karpenter.sh/v1
kind: NodePool
metadata:
  name: gpu-workloads
spec:
  template:
    spec:
      nodeClassRef:
        group: karpenter.k8s.aws
        kind: EC2NodeClass
        name: gpu-nodes

      requirements:
        - key: karpenter.k8s.aws/instance-family
          operator: In
          values: ["g5", "g6", "p4", "p5"]

        - key: karpenter.sh/capacity-type
          operator: In
          values: ["on-demand"] # GPU instances typically on-demand

        - key: karpenter.k8s.aws/instance-gpu-count
          operator: Gt
          values: ["0"]

      taints:
        - key: nvidia.com/gpu
          value: "true"
          effect: NoSchedule

      metadata:
        labels:
          workload-type: gpu
          nvidia.com/gpu: "true"

  limits:
    cpu: 500
    memory: 2000Gi
    nvidia.com/gpu: 16

  disruption:
    consolidationPolicy: WhenEmpty
    consolidateAfter: 300s

---
# Batch/Spot-heavy NodePool
apiVersion: karpenter.sh/v1
kind: NodePool
metadata:
  name: batch-workloads
spec:
  template:
    spec:
      nodeClassRef:
        group: karpenter.k8s.aws
        kind: EC2NodeClass
        name: default

      requirements:
        # Spot-only: NO automatic on-demand fallback — pods stay Pending
        # if spot is exhausted. Add "on-demand" here for fallback.
        - key: karpenter.sh/capacity-type
          operator: In
          values: ["spot"]

        - key: karpenter.k8s.aws/instance-family
          operator: In
          values: ["c6a", "c6i", "c7i", "m6a", "m6i"] # Compute-optimized

        - key: karpenter.k8s.aws/instance-size
          operator: In
          values: ["2xlarge", "4xlarge", "8xlarge"]

      taints:
        - key: workload-type
          value: batch
          effect: NoSchedule

      metadata:
        labels:
          workload-type: batch
          spot-interruption-handler: enabled

  disruption:
    consolidationPolicy: WhenEmpty
    consolidateAfter: 60s
    budgets:
      - nodes: "20%" # Allow more aggressive disruption for batch

---
# Stateful workload NodePool (on-demand only)
apiVersion: karpenter.sh/v1
kind: NodePool
metadata:
  name: stateful-workloads
spec:
  template:
    spec:
      nodeClassRef:
        group: karpenter.k8s.aws
        kind: EC2NodeClass
        name: stateful-nodes

      requirements:
        - key: karpenter.sh/capacity-type
          operator: In
          values: ["on-demand"] # Only on-demand for stability

        - key: karpenter.k8s.aws/instance-family
          operator: In
          values: ["r6i", "r7i"] # Memory-optimized

        - key: karpenter.k8s.aws/instance-size
          operator: In
          values: ["xlarge", "2xlarge", "4xlarge"]

        - key: topology.kubernetes.io/zone
          operator: In
          values: ["us-west-2a", "us-west-2b"]

      # kubelet config (e.g. maxPods: 50 for lower density) belongs on the
      # referenced EC2NodeClass "stateful-nodes" in v1, not here.

      taints:
        - key: workload-type
          value: stateful
          effect: NoSchedule

      metadata:
        labels:
          workload-type: stateful
          storage-optimized: "true"

  limits:
    cpu: 200
    memory: 800Gi

  disruption:
    consolidationPolicy: WhenEmpty # Only consolidate when completely empty
    consolidateAfter: 600s # Wait 10 minutes
    budgets:
      - nodes: "1" # Very conservative disruption
        duration: 30m
```

### Example 4: Disruption Budgets and Consolidation Policies

```yaml
apiVersion: karpenter.sh/v1
kind: NodePool
metadata:
  name: production-apps
spec:
  template:
    spec:
      nodeClassRef:
        group: karpenter.k8s.aws
        kind: EC2NodeClass
        name: default

      requirements:
        - key: karpenter.sh/capacity-type
          operator: In
          values: ["spot", "on-demand"]

        - key: karpenter.k8s.aws/instance-family
          operator: In
          values: ["c6i", "m6i", "r6i"]

      # Expiration is a template field in v1 and is drift-able: changing it
      # rolls existing nodes. Expiration is forceful and NOT budget-limited,
      # so cap drain time with terminationGracePeriod.
      expireAfter: 720h # 30 days
      terminationGracePeriod: 1h

  # Advanced disruption configuration
  disruption:
    # Consolidation policy options (v1):
    # - WhenEmptyOrUnderutilized: replace under-used nodes with cheaper/smaller
    # - WhenEmpty: only replace completely empty nodes
    consolidationPolicy: WhenEmptyOrUnderutilized

    # How soon after a node becomes eligible for consolidation
    consolidateAfter: 30s

    # Multiple budget windows. SCHEDULES ARE UTC-ONLY (no timezone support),
    # and when windows overlap Karpenter takes the MINIMUM. NotReady nodes
    # also consume budget — see Gotchas.
    budgets:
      # During business hours: conservative disruption (08:00 UTC)
      - nodes: "5%"
        duration: 8h
        schedule: "0 8 * * MON-FRI"

      # During off-hours: more aggressive consolidation
      - nodes: "20%"
        duration: 16h
        schedule: "0 18 * * MON-FRI"

      # Weekends: most aggressive
      - nodes: "30%"
        duration: 48h
        schedule: "0 0 * * SAT"

      # Default budget (always active)
      - nodes: "10%"
```

### Example 5: Pod Scheduling with Karpenter

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-application
spec:
  replicas: 5
  selector:
    matchLabels:
      app: my-application
  template:
    metadata:
      labels:
        app: my-application
    spec:
      # Tolerations to allow scheduling on Karpenter nodes
      tolerations:
        - key: workload-type
          operator: Equal
          value: general
          effect: NoSchedule

      # Node selector to target specific NodePool
      nodeSelector:
        workload-type: general
        karpenter.sh/capacity-type: spot # Prefer spot

      # Affinity rules for better placement
      affinity:
        # Spread across zones for availability
        podAntiAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
            - weight: 100
              podAffinityTerm:
                labelSelector:
                  matchLabels:
                    app: my-application
                topologyKey: topology.kubernetes.io/zone

        # Node affinity for instance type preferences
        nodeAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
            # Prefer ARM instances (cheaper)
            - weight: 50
              preference:
                matchExpressions:
                  - key: kubernetes.io/arch
                    operator: In
                    values: ["arm64"]

            # Prefer larger instances (better bin-packing)
            - weight: 30
              preference:
                matchExpressions:
                  - key: karpenter.k8s.aws/instance-size
                    operator: In
                    values: ["2xlarge", "4xlarge"]

      # Topology spread constraints
      topologySpreadConstraints:
        # Spread across zones
        - maxSkew: 1
          topologyKey: topology.kubernetes.io/zone
          whenUnsatisfiable: ScheduleAnyway
          labelSelector:
            matchLabels:
              app: my-application

        # Spread across nodes
        - maxSkew: 1
          topologyKey: kubernetes.io/hostname
          whenUnsatisfiable: ScheduleAnyway
          labelSelector:
            matchLabels:
              app: my-application

      containers:
        - name: app
          image: my-app:latest

          # CRITICAL: Accurate resource requests for Karpenter
          resources:
            requests:
              cpu: 500m
              memory: 1Gi
            limits:
              cpu: 1000m
              memory: 2Gi

          # Graceful shutdown for spot interruptions
          lifecycle:
            preStop:
              exec:
                command:
                  - /bin/sh
                  - -c
                  - sleep 15 # Allow time for deregistration

      # Termination grace period for spot interruptions
      terminationGracePeriodSeconds: 30
```

### Example 6: Spot Instance Handling and Fallback

```yaml
apiVersion: karpenter.sh/v1
kind: NodePool
metadata:
  name: spot-with-fallback
spec:
  template:
    spec:
      nodeClassRef:
        group: karpenter.k8s.aws
        kind: EC2NodeClass
        name: default

      requirements:
        # Both capacity types in ONE NodePool gives on-demand fallback when
        # spot is exhausted (spot-only would leave pods Pending).
        - key: karpenter.sh/capacity-type
          operator: In
          values: ["spot", "on-demand"]

        # Wide instance type selection for better spot availability
        - key: karpenter.k8s.aws/instance-family
          operator: In
          values:
            - "c5a"
            - "c6a"
            - "c6i"
            - "c7i"
            - "m5a"
            - "m6a"
            - "m6i"
            - "m7i"
            - "r5a"
            - "r6a"
            - "r6i"
            - "r7i"

        - key: karpenter.k8s.aws/instance-size
          operator: In
          values: ["large", "xlarge", "2xlarge", "4xlarge"]

        # Support both architectures for more spot options
        - key: kubernetes.io/arch
          operator: In
          values: ["amd64", "arm64"]

      # Metadata to track spot usage.
      # NOTE: spot-to-spot consolidation is a CONTROLLER feature gate
      # (settings.featureGates.spotToSpotConsolidation=true via Helm), NOT a
      # NodePool annotation — there is no such annotation (see Gotchas).
      metadata:
        labels:
          spot-enabled: "true"

  disruption:
    consolidationPolicy: WhenEmptyOrUnderutilized
    consolidateAfter: 30s

    # More aggressive for spot since they can be interrupted anyway
    budgets:
      - nodes: "25%"

  # Weight influences Karpenter's NodePool selection
  # Higher weight = more preferred
  # Use lower weight so other NodePools are tried first
  weight: 5
```

### Example 7: Karpenter with Pod Disruption Budget

```yaml
# Application Deployment
apiVersion: apps/v1
kind: Deployment
metadata:
  name: critical-service
spec:
  replicas: 6
  selector:
    matchLabels:
      app: critical-service
  template:
    metadata:
      labels:
        app: critical-service
    spec:
      tolerations:
        - key: workload-type
          operator: Equal
          value: general
          effect: NoSchedule

      containers:
        - name: app
          image: critical-service:latest
          resources:
            requests:
              cpu: 1000m
              memory: 2Gi
            limits:
              cpu: 2000m
              memory: 4Gi

---
# Pod Disruption Budget to protect during consolidation
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: critical-service-pdb
spec:
  minAvailable: 4 # Always keep at least 4 replicas running
  selector:
    matchLabels:
      app: critical-service
# Karpenter respects PDBs during consolidation
# It will not disrupt nodes if doing so would violate the PDB
```

### Example 8: Multi-Architecture NodePool

```yaml
apiVersion: karpenter.sh/v1
kind: NodePool
metadata:
  name: multi-arch
spec:
  template:
    spec:
      nodeClassRef:
        group: karpenter.k8s.aws
        kind: EC2NodeClass
        name: default

      requirements:
        # Support both AMD64 and ARM64
        - key: kubernetes.io/arch
          operator: In
          values: ["amd64", "arm64"]

        # ARM instances (Graviton) - typically 20% cheaper
        - key: karpenter.k8s.aws/instance-family
          operator: In
          values:
            # ARM (Graviton2)
            - "c6g"
            - "m6g"
            - "r6g"
            # ARM (Graviton3)
            - "c7g"
            - "m7g"
            - "r7g"
            # AMD64 alternatives
            - "c6i"
            - "m6i"
            - "r6i"

        - key: karpenter.sh/capacity-type
          operator: In
          values: ["spot", "on-demand"]

      metadata:
        labels:
          multi-arch: "true"

  disruption:
    consolidationPolicy: WhenEmptyOrUnderutilized
    consolidateAfter: 60s

---
# EC2NodeClass with multi-architecture AMI support
apiVersion: karpenter.k8s.aws/v1
kind: EC2NodeClass
metadata:
  name: default
spec:
  # amiSelectorTerms is required in v1. The al2023 alias resolves the right
  # AMI per architecture automatically; pin @version in production.
  amiSelectorTerms:
    - alias: al2023@v20240807

  role: KarpenterNodeRole-my-cluster

  subnetSelectorTerms:
    - tags:
        karpenter.sh/discovery: my-cluster

  securityGroupSelectorTerms:
    - tags:
        karpenter.sh/discovery: my-cluster
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

## Monitoring and Troubleshooting

### Key Metrics to Monitor

```text
# Scheduling / provisioning metrics (v1 — "provisioner" metrics were removed)
karpenter_nodes_created_total
karpenter_nodes_terminated_total
karpenter_scheduler_scheduling_duration_seconds

# Disruption metrics
karpenter_nodepools_allowed_disruptions
karpenter_voluntary_disruption_eligible_nodes
karpenter_voluntary_disruption_decisions_total

# Cost metrics
karpenter_cloudprovider_instance_type_offering_price_estimate

# Pod metrics
karpenter_pods_state (pending, running, etc.)

# Cross-check names against the live /metrics endpoint and
# https://karpenter.sh/docs/reference/metrics/ — they change between releases.
```

### Common Issues and Solutions

#### Issue: Pods stuck in Pending

- Check NodePool requirements match pod node selectors/tolerations
- Verify cloud provider limits not exceeded
- Check instance type availability in selected zones
- Ensure subnet capacity available

#### Issue: Excessive node churn

- Adjust consolidation delay (consolidateAfter)
- Review disruption budgets
- Check if pod resource requests are accurate
- Consider using WhenEmpty instead of WhenEmptyOrUnderutilized

#### Issue: High costs despite using Karpenter

- Enable consolidation if not already active
- Verify spot instances are being used
- Check if pods have unnecessarily large resource requests
- Review instance type selection (allow more variety)

#### Issue: Spot interruptions causing service disruption

- Implement Pod Disruption Budgets
- Use diverse instance types for better spot availability
- Configure appropriate replica counts
- Implement graceful shutdown in applications

## Integration with Terraform

```hcl
# Install Karpenter via Terraform
resource "helm_release" "karpenter" {
  namespace        = "karpenter"
  create_namespace = true
  name             = "karpenter"
  repository       = "oci://public.ecr.aws/karpenter"
  chart            = "karpenter"
  version          = "1.1.1" # pin a current 1.x release (v1 APIs)

  values = [
    <<-EOT
    settings:
      clusterName: ${var.cluster_name}
      clusterEndpoint: ${var.cluster_endpoint}
      # Native interruption handling — feed this SQS queue from EventBridge.
      # Do NOT also run aws-node-termination-handler (double-drain churn).
      interruptionQueue: ${var.interruption_queue_name}
      # Spot-to-spot consolidation is controller-level (not per-NodePool):
      featureGates:
        spotToSpotConsolidation: true

    serviceAccount:
      annotations:
        eks.amazonaws.com/role-arn: ${var.karpenter_irsa_arn}

    controller:
      resources:
        requests:
          cpu: 1
          memory: 1Gi
        limits:
          cpu: 2
          memory: 2Gi
    EOT
  ]

  depends_on = [
    aws_iam_role_policy_attachment.karpenter_controller
  ]
}

# Deploy default NodePool
resource "kubectl_manifest" "karpenter_nodepool_default" {
  yaml_body = <<-YAML
    apiVersion: karpenter.sh/v1
    kind: NodePool
    metadata:
      name: default
    spec:
      template:
        spec:
          nodeClassRef:
            group: karpenter.k8s.aws
            kind: EC2NodeClass
            name: default
          requirements:
            - key: karpenter.sh/capacity-type
              operator: In
              values: ["spot", "on-demand"]
            - key: karpenter.k8s.aws/instance-category
              operator: In
              values: ["c", "m", "r"]
            - key: karpenter.k8s.aws/instance-generation
              operator: Gt
              values: ["2"]
          expireAfter: 720h
      limits:
        cpu: 1000
        memory: 1000Gi
      disruption:
        consolidationPolicy: WhenEmptyOrUnderutilized
        consolidateAfter: 30s
  YAML

  depends_on = [helm_release.karpenter]
}
```

## Migration from Cluster Autoscaler

1. **Plan the migration**
   - Identify current node groups and their characteristics
   - Map workloads to new NodePool configurations
   - Plan for coexistence period

2. **Deploy Karpenter alongside Cluster Autoscaler**
   - Install Karpenter in the cluster
   - Create NodePools with distinct labels
   - Test with non-critical workloads first

3. **Migrate workloads incrementally**
   - Update pod specs with Karpenter tolerations/node selectors
   - Monitor provisioning and consolidation behavior
   - Validate cost and performance metrics

4. **Remove Cluster Autoscaler**
   - Once all workloads migrated, scale down CA node groups
   - Remove Cluster Autoscaler deployment
   - Clean up CA-specific resources
