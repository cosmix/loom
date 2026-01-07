# PLAN: Invalid Cycle Test

This plan has a circular dependency and should fail validation.

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-a
      name: "Stage A"
      dependencies: [stage-c]
    - id: stage-b
      name: "Stage B"
      dependencies: [stage-a]
    - id: stage-c
      name: "Stage C"
      dependencies: [stage-b]
```

<!-- END loom METADATA -->
