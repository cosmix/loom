# PLAN: Invalid Cycle Test

This plan has a circular dependency and should fail validation.

---

<!-- FLUX METADATA - Do not edit manually -->

```yaml
flux:
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

<!-- END FLUX METADATA -->
