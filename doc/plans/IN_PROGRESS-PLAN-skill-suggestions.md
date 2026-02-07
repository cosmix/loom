# Plan: Language-Aware Skill Suggestions for Loom Stages

## Context

When loom stages execute, the signal generation system can recommend relevant Claude Code skills (like `/auth`, `/testing`, `/rust`) based on keyword matching against the stage description. However, there are two problems:

1. **Broken trigger parsing for most skills**: Only skills with explicit `triggers:` YAML lists (e.g., `/auth`) are matched. Skills using `trigger-keywords:` CSV (e.g., `/testing`) or description-embedded "Triggers:" (e.g., `/rust`, `/python`, `/typescript`, `/golang`) are silently ignored because `SkillMetadata` only deserializes the `triggers` YAML field.

2. **No project language awareness**: Even if trigger parsing worked, a Rust project's stage saying "Implement error handling for the API" would never match `/rust` because the description doesn't mention "rust". The system has no way to know the project's language and boost relevant skills.

**Goal**: Fix trigger parsing so all 62 skills are discoverable, auto-detect the project's language(s), and ensure language skills are always recommended for matching projects.

## Proposed Changes

### 1. Fix SkillMetadata trigger parsing (prerequisite)

**File**: `loom/src/skills/types.rs`

Add a `trigger_keywords` field with serde alias:

```rust
#[serde(default, alias = "trigger-keywords")]
pub trigger_keywords: Option<String>,  // CSV format
```

**File**: `loom/src/skills/index.rs`

In `add_skill()`, after processing `metadata.triggers`, also:

- Parse `trigger_keywords` CSV if present (split on `,`, trim)
- If both `triggers` and `trigger_keywords` are empty, extract from description's "Triggers:" suffix

Priority: `triggers` YAML list > `trigger-keywords` CSV > description-embedded "Triggers:"

### 2. Extract project language detection into shared module

**New file**: `loom/src/language.rs` (small module, ~80 lines)

Extract and extend the detection logic from `commands/sandbox/suggest.rs`:

```rust
pub struct DetectedLanguage {
    pub name: String,       // "rust", "python", "typescript", "go"
    pub skill_name: String, // "rust", "python", "typescript", "golang"
    pub detected_by: String, // "Cargo.toml", "package.json", etc.
}

pub fn detect_project_languages(root: &Path) -> Vec<DetectedLanguage>
```

Detection rules (extending sandbox suggest):

| File | Language | Skill |
|------|----------|-------|
| `Cargo.toml` | Rust | `rust` |
| `package.json` | TypeScript/JavaScript | `typescript` |
| `tsconfig.json` | TypeScript | `typescript` |
| `pyproject.toml` / `requirements.txt` | Python | `python` |
| `go.mod` | Go | `golang` |

Reuse this in `sandbox/suggest.rs` to avoid duplication (sandbox suggest calls `detect_project_languages` then maps to domains).

### 3. Add `get_by_name()` to SkillIndex

**File**: `loom/src/skills/index.rs`

```rust
pub fn get_by_name(&self, name: &str) -> Option<&SkillMetadata> {
    self.skills.iter().find(|s| s.name == name)
}
```

This enables looking up a specific skill by name for direct injection.

### 4. Inject detected language skills into signal generation

**File**: `loom/src/orchestrator/signals/generate.rs`

Change `generate_signal_with_skills()` to accept detected languages:

```rust
pub fn generate_signal_with_skills(
    ...,
    skill_index: Option<&SkillIndex>,
    detected_languages: &[DetectedLanguage],  // NEW
) -> Result<PathBuf>
```

In the skill recommendation logic:

1. Start with keyword-matched skills (existing behavior, now works for all skills)
2. For each detected language, look up the corresponding skill by name via `get_by_name()`
3. If the language skill wasn't already matched by keywords, inject it with `matched_triggers: ["project-language"]` and a high score
4. Deduplicate by name, cap at `DEFAULT_MAX_SKILL_RECOMMENDATIONS`

### 5. Detect languages in orchestrator at startup

**File**: `loom/src/orchestrator/core/orchestrator.rs`

In `Orchestrator::new()`, after loading skill index:

```rust
let detected_languages = detect_project_languages(&config.project_root());
```

Store as `pub(super) detected_languages: Vec<DetectedLanguage>` on Orchestrator.

**File**: `loom/src/orchestrator/core/stage_executor.rs`

Pass `&self.detected_languages` to `generate_signal_with_skills()`.

### 6. Enhance signal display for language skills

**File**: `loom/src/orchestrator/signals/format/sections.rs`

In `format_skill_recommendations()`, add a note when a skill was recommended due to project language detection:

```
| rust | Rust language expertise... | `/rust` | project language |
```

Add a "project-language" column or annotation to the matched triggers display so agents understand WHY a skill was recommended.

## Files Modified

| File                                               | Change                                                        |
| -------------------------------------------------- | ------------------------------------------------------------- |
| `loom/src/skills/types.rs`                         | Add `trigger_keywords` field to SkillMetadata                 |
| `loom/src/skills/index.rs`                         | Parse all trigger formats; add `get_by_name()`                |
| `loom/src/skills/matcher.rs`                       | No changes needed (already works with any trigger source)     |
| `loom/src/language.rs`                             | **NEW** - Project language detection                          |
| `loom/src/lib.rs`                                  | Export `language` module                                      |
| `loom/src/commands/sandbox/suggest.rs`             | Refactor to use shared `language::detect_project_languages()` |
| `loom/src/orchestrator/signals/generate.rs`        | Accept detected languages, inject language skills             |
| `loom/src/orchestrator/core/orchestrator.rs`       | Detect languages at startup, store on struct                  |
| `loom/src/orchestrator/core/stage_executor.rs`     | Pass detected languages to signal generation                  |
| `loom/src/orchestrator/signals/format/sections.rs` | Annotate language-detected skills in display                  |

## Verification

```bash
cd loom
cargo test                     # All existing tests pass
cargo clippy -- -D warnings    # No new warnings
cargo test skills              # Skill matching tests specifically
```

### Manual verification

1. Create a temp project with `Cargo.toml` → verify `/rust` appears in signal recommendations
2. Create a temp project with `package.json` + `tsconfig.json` → verify `/typescript` appears
3. Verify skills with `trigger-keywords:` CSV (like `/testing`) now match correctly
4. Verify skills with description-embedded triggers (like `/rust`) now match correctly
5. Verify deduplication: if stage description mentions "rust" AND project has `Cargo.toml`, `/rust` appears only once

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  sandbox:
    enabled: true
    auto_allow: true
    excluded_commands: ["loom"]
    filesystem:
      deny_read:
        - "~/.ssh/**"
        - "~/.aws/**"
        - "../../**"
      deny_write:
        - "../../**"
        - "doc/loom/knowledge/**"
    network:
      allowed_domains:
        - "github.com"
        - "api.github.com"
        - "crates.io"
        - "static.crates.io"
        - "doc.rust-lang.org"
  stages:
    - id: knowledge-bootstrap
      name: "Knowledge Bootstrap"
      description: |
        Verify existing knowledge coverage and fill gaps related to skill matching,
        trigger parsing, and language detection integration points.

        Run loom knowledge check. If coverage < 50%, run loom map --deep.
        Focus on: skills/ module internals, signal generation data flow,
        sandbox suggest detection logic.
      stage_type: knowledge
      working_dir: "."
      dependencies: []
      acceptance:
        - "test -d doc/loom/knowledge"

    - id: fix-trigger-parsing
      name: "Fix Skill Trigger Parsing"
      description: |
        Fix SkillMetadata to parse all three trigger formats so all 62 skills
        are discoverable by the keyword matcher.

        EXECUTION PLAN (2 parallel subagents):

        Subagent 1 - Types & Index:
          Files owned: src/skills/types.rs, src/skills/index.rs
          Task: Add trigger_keywords field to SkillMetadata. In add_skill(),
                parse trigger_keywords CSV. If both triggers and trigger_keywords
                are empty, extract from description "Triggers:" suffix.
                Add get_by_name() method to SkillIndex.
          Acceptance: cargo test skills passes, new tests for CSV and description parsing

        Subagent 2 - Tests:
          Files owned: src/skills/tests.rs (if exists, else inline in index.rs tests)
          Files read-only: src/skills/types.rs, src/skills/index.rs
          Task: Write tests for: trigger_keywords CSV parsing, description-embedded
                trigger extraction, get_by_name lookup, priority ordering
                (YAML > CSV > description).
          Acceptance: All new tests pass
      stage_type: standard
      working_dir: "loom"
      dependencies:
        - knowledge-bootstrap
      acceptance:
        - "cargo test skills"
        - "cargo clippy -- -D warnings"
      truths:
        - "cargo test skills -- --nocapture 2>&1 | grep -q 'test result: ok'"
      artifacts:
        - "src/skills/types.rs"
        - "src/skills/index.rs"
      wiring:
        - source: "src/skills/index.rs"
          pattern: "trigger_keywords"
          description: "Index parses trigger_keywords CSV from skill metadata"
        - source: "src/skills/index.rs"
          pattern: "get_by_name"
          description: "SkillIndex has get_by_name lookup method"

    - id: language-detection
      name: "Project Language Detection Module"
      description: |
        Create shared language detection module and refactor sandbox suggest to use it.

        EXECUTION PLAN (2 parallel subagents):

        Subagent 1 - Detection module:
          Files owned: src/language.rs, src/lib.rs
          Task: Create language.rs with DetectedLanguage struct and
                detect_project_languages(root) function. Detect Rust (Cargo.toml),
                TypeScript (tsconfig.json/package.json), Python (pyproject.toml/
                requirements.txt), Go (go.mod). Export module from lib.rs.
          Acceptance: cargo build succeeds, unit tests for each language detection

        Subagent 2 - Sandbox refactor:
          Files owned: src/commands/sandbox/suggest.rs
          Files read-only: src/language.rs
          Task: Refactor suggest.rs to call detect_project_languages() and map
                DetectedLanguage to domain lists. Remove duplicated detection logic.
          Acceptance: cargo test sandbox passes, existing suggest tests still pass
      stage_type: standard
      working_dir: "loom"
      dependencies:
        - knowledge-bootstrap
      acceptance:
        - "cargo test language"
        - "cargo test sandbox"
        - "cargo clippy -- -D warnings"
      truths:
        - "cargo test language -- --nocapture 2>&1 | grep -q 'test result: ok'"
      artifacts:
        - "src/language.rs"
      wiring:
        - source: "src/lib.rs"
          pattern: "pub mod language"
          description: "Language module exported from lib.rs"
        - source: "src/commands/sandbox/suggest.rs"
          pattern: "detect_project_languages"
          description: "Sandbox suggest reuses shared detection"

    - id: signal-integration
      name: "Integrate Language Skills into Signal Generation"
      description: |
        Wire language detection into orchestrator and signal generation so detected
        language skills are always recommended in signals.

        EXECUTION PLAN (2 parallel subagents):

        Subagent 1 - Orchestrator & signal generation:
          Files owned: src/orchestrator/core/orchestrator.rs,
                       src/orchestrator/core/stage_executor.rs,
                       src/orchestrator/signals/generate.rs
          Task: In Orchestrator::new(), detect project languages and store on struct.
                In stage_executor, pass detected_languages to generate_signal_with_skills().
                In generate.rs, after keyword matching, inject detected language skills
                via get_by_name() with high score and "project-language" trigger.
                Deduplicate by skill name.
          Acceptance: cargo build, cargo test orchestrator

        Subagent 2 - Signal display:
          Files owned: src/orchestrator/signals/format/sections.rs
          Files read-only: src/skills/types.rs
          Task: In format_skill_recommendations(), annotate skills that were
                recommended due to project language detection (show "project-language"
                in matched triggers). Keep table format clean.
          Acceptance: cargo build, cargo clippy
      stage_type: standard
      working_dir: "loom"
      dependencies:
        - fix-trigger-parsing
        - language-detection
      acceptance:
        - "cargo test"
        - "cargo clippy -- -D warnings"
      truths:
        - "cargo test -- --nocapture 2>&1 | grep -q 'test result: ok'"
      artifacts:
        - "src/orchestrator/signals/generate.rs"
        - "src/orchestrator/core/orchestrator.rs"
      wiring:
        - source: "src/orchestrator/core/orchestrator.rs"
          pattern: "detected_languages"
          description: "Orchestrator stores detected project languages"
        - source: "src/orchestrator/signals/generate.rs"
          pattern: "detected_languages"
          description: "Signal generation uses detected languages for skill injection"

    - id: code-review
      name: "Code Review"
      description: |
        Security and quality review of all changes. Check for:
        - No hardcoded skill names that could drift from actual skill files
        - Trigger parsing handles edge cases (empty descriptions, malformed CSV)
        - Language detection handles missing/unreadable files gracefully
        - No unwrap() in new code, proper error handling with anyhow
      stage_type: code-review
      working_dir: "loom"
      dependencies:
        - fix-trigger-parsing
        - language-detection
        - signal-integration
      acceptance:
        - "cargo test"
        - "cargo clippy -- -D warnings"

    - id: integration-verify
      name: "Integration Verification"
      description: |
        Verify the complete feature end-to-end:
        1. All tests pass (cargo test)
        2. No clippy warnings (cargo clippy -- -D warnings)
        3. Build succeeds (cargo build)
        4. Functional: skill trigger parsing works for all 3 formats
        5. Functional: language detection detects Rust from Cargo.toml
        6. Functional: detected language skills appear in generated signals
        7. Verify sandbox suggest still works after refactor

        FUNCTIONAL VERIFICATION (MANDATORY):
        - Verify trigger_keywords field is parsed from skill YAML
        - Verify description-embedded triggers are extracted
        - Verify detect_project_languages finds Rust from Cargo.toml
        - Verify signal generation includes language-detected skills

        KNOWLEDGE (MANDATORY):
        - loom memory list
        - loom memory promote all mistakes
        - loom memory promote decision patterns
      stage_type: integration-verify
      working_dir: "loom"
      dependencies:
        - code-review
      acceptance:
        - "cargo test"
        - "cargo clippy -- -D warnings"
        - "cargo build"
        - "cargo fmt --check"
      truths:
        - "cargo test skills -- --nocapture 2>&1 | grep -q 'test result: ok'"
        - "cargo test language -- --nocapture 2>&1 | grep -q 'test result: ok'"
      artifacts:
        - "src/language.rs"
        - "src/skills/types.rs"
        - "src/skills/index.rs"
      wiring:
        - source: "src/lib.rs"
          pattern: "pub mod language"
          description: "Language detection module is exported"
        - source: "src/orchestrator/signals/generate.rs"
          pattern: "detected_languages"
          description: "Signal generation uses detected languages for skill injection"
        - source: "src/skills/index.rs"
          pattern: "trigger_keywords"
          description: "Index parses all trigger formats"
```

<!-- END loom METADATA -->
