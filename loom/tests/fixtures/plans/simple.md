# PLAN: Simple Sequential Test

A test plan with two sequential stages.

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "First Stage"
      description: "Initial setup work"
      dependencies: []
      acceptance:
        - "echo 'stage 1 complete'"
      files:
        - "src/*.rs"
    - id: stage-2
      name: "Second Stage"
      description: "Follow-up work"
      dependencies: [stage-1]
      acceptance:
        - "echo 'stage 2 complete'"
      files:
        - "tests/*.rs"
```

<!-- END loom METADATA -->
