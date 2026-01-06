# PLAN: Acceptance Criteria Test

A test plan with comprehensive acceptance criteria.

---

<!-- FLUX METADATA - Do not edit manually -->

```yaml
flux:
  version: 1
  stages:
    - id: stage-with-criteria
      name: "Stage With Acceptance"
      description: "This stage has multiple acceptance criteria"
      dependencies: []
      acceptance:
        - "cargo test"
        - "cargo clippy -- -D warnings"
        - "cargo fmt --check"
      files:
        - "src/*.rs"
        - "tests/*.rs"
```

<!-- END FLUX METADATA -->
