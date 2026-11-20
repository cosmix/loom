# Brief: CI installs every toolchain and runs the confinement probes (codex unit, gpt-6.1-terra)

Stage `stage-cache-sessions`, worker `ci`. Repository root is the working directory. Do not touch git or `.loom/`.

## File you own

`.github/workflows/ci.yml` only.

## Context

- The `test` job ("Test (linux)", ubuntu-latest, around line 107) installs `tmux ripgrep bubblewrap` and runs `sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0` (lines 127-130). It then runs `cargo test --all-targets` in `loom` (lines 132-136) and a bubblewrap probe step with `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` (lines 138-142).
- The `test-macos` job ("Test (macos)", macos-latest, around line 161) runs `brew install tmux ripgrep jq` and `cargo test --all-targets --no-fail-fast`.
- Read both jobs in full before editing. Keep every existing step and its order.

## Changes (both jobs)

1. **Toolchain setup**, before the test steps, with pinned major versions:
   - `actions/setup-go@v5` (go-version `stable`);
   - `actions/setup-java@v4` (distribution `temurin`, java-version `21`);
   - `gradle/actions/setup-gradle@v4`;
   - Maven 3.9: the runner images ship it; add a step that runs `mvn -v` and fails unless the version starts with `3.9`;
   - `actions/setup-node@v4` (node-version `22`);
   - `oven-sh/setup-bun@v2`;
   - `astral-sh/setup-uv@v6`;
   - `mozilla-actions/sccache-action@v0.0.9`;
   - ccache: `sudo apt-get install -y ccache` on Linux, `brew install ccache` on macOS;
   - cmake: present on both images; add a `cmake --version` check.
2. **Seed step**, network allowed, run before the probes. Seed the host caches the probes read, then export the variables through `$GITHUB_ENV`:
   - Go: `go mod download github.com/pkg/errors@v0.9.1`, run in a temp module (`go mod init seed`).
   - Maven: `mvn -q -B dependency:get -Dartifact=commons-lang:commons-lang:2.6`.
   - Gradle: a throwaway project (`settings.gradle`, a `build.gradle` with the java plugin, mavenCentral and `implementation 'commons-lang:commons-lang:2.6'`, and one Java source file), then `gradle -q --no-daemon compileJava`.
   - Exports: `LOOM_TEST_GO_SEED_MODULE=github.com/pkg/errors@v0.9.1`, `LOOM_TEST_MAVEN_SEED=commons-lang:commons-lang:2.6`, `LOOM_TEST_GRADLE_SEED=commons-lang:commons-lang:2.6`.
3. **Probe step**, named "Run the confined toolchain probes", after the existing test step:
   - working-directory `loom`;
   - env `LOOM_TEST_REQUIRE_SANDBOX_FREE: "1"`;
   - run `cargo test --lib stage_cache::`, which includes `stage_cache::toolchain_tests::`; name both in a comment above the step so the literal `stage_cache::toolchain_tests::` appears.
   This is the step a skip fails, so a missing toolchain or launcher on either runner is a red build.
4. Raise nothing else: no new jobs, no changes to the `changes` filter beyond keeping these steps gated on the existing `rust` flag like their job.

## Verify

YAML parses: `ruby -ryaml -e 'YAML.load_file(".github/workflows/ci.yml")'` or `python3 -c 'import yaml,sys; yaml.safe_load(open(".github/workflows/ci.yml"))'`, whichever exists. Run `actionlint` if it is installed. Make sure the file contains the literal `stage_cache::toolchain_tests::`; the stage acceptance greps for it. Report the steps added with their line numbers.
