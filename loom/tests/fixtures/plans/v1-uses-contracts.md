# PLAN: Version 1 Plan Using Contracts

A `version: 1` plan whose standard stage declares a `contracts:` entry. Contracts
exist only in plan version 2, so `loom plan verify` must reject it.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: add-greeting
      name: "Add a greeting"
      stage_type: standard
      working_dir: "."
      dependencies: []
      contracts:
        - id: greets-by-name
          file: "loom/tests/greeting.rs"
          test: "greets_by_name"
          runner: cargo-test
          scenario: "a greeting is built for the user named Ada"
          rejects: "a greeting that ignores the name and always says hello world"
      acceptance:
        - "cargo test --manifest-path loom/Cargo.toml --test greeting"
```

<!-- END loom METADATA -->
