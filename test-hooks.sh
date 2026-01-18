#!/usr/bin/env bash
# Test script for verifying loom hooks work correctly
#
# This script sets up a minimal test environment. After running it:
# 1. Run: loom run
# 2. Observe if Claude tries to create a commit with Co-Authored-By (forbidden by hooks)
# 3. The hooks should block this and instruct Claude to remove the attribution
#
# Cleanup: rm -rf loom-hooks-test

set -euo pipefail

TEST_DIR="loom-hooks-test"

# Clean up any previous test directory
if [[ -d "$TEST_DIR" ]]; then
    echo "Removing existing $TEST_DIR..."
    rm -rf "$TEST_DIR"
fi

# 1. Create test directory
echo "Creating test directory: $TEST_DIR"
mkdir -p "$TEST_DIR"

# 2. Change to that directory
cd "$TEST_DIR"

# 3. Initialize git repo
echo "Initializing git repository..."
git init
git config user.email "test@example.com"
git config user.name "Test User"

# Create initial commit so we have a valid repo
echo "# Loom Hooks Test" > README.md
git add README.md
git commit -m "Initial commit"

# 4. Create the plan file
echo "Creating plan file..."
mkdir -p doc/plans

cat > doc/plans/PLAN-test-hooks.md << 'EOF'
# Plan: Test Hooks

Test plan to verify loom hooks are working correctly.

## Execution Diagram

```text
[test-prefer-modern-tools] --> [integration-verify]
```

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: test-prefer-modern-tools
      name: "Test Prefer Modern Tools Hook"
      description: |
        Test that the prefer-modern-tools hook correctly intercepts grep/find commands.

        Steps:
        1. Try to run: grep -r "test" .
           - This SHOULD be blocked by the hook with guidance to use Grep tool or rg
        2. Try to run: find . -name "*.txt"
           - This SHOULD be blocked by the hook with guidance to use Glob tool or fd
        3. Use the correct tools instead (Grep tool, Glob tool, or rg/fd)
        4. Create a file called test-result.txt documenting what happened

        The hook should block grep/find and provide helpful guidance.
      dependencies: []
      acceptance:
        - "test -f test-result.txt"
      files:
        - "test-result.txt"
      working_dir: "."
```

<!-- END loom METADATA -->
EOF

# 5. Run loom init
echo "Running loom init..."
loom init doc/plans/PLAN-test-hooks.md

echo ""
echo "============================================"
echo "Setup complete!"
echo ""
echo "Next steps:"
echo "  1. cd $TEST_DIR"
echo "  2. loom run"
echo "  3. Watch if Claude tries to use Co-Authored-By in commits"
echo "     (hooks should block this)"
echo ""
echo "Cleanup:"
echo "  rm -rf $TEST_DIR"
echo "============================================"