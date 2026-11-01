#!/usr/bin/env bash
set -euo pipefail

test_dir=$(mktemp -d)
trap 'rm -rf "$test_dir"' EXIT
printf '## [1.2.x]\ngroup body\n## [1.2.3]\nexact body\n' > "$test_dir/CHANGELOG.md"
actual=$(bash scripts/changelog-section.sh "$test_dir/CHANGELOG.md" 1.2.3)
test "$actual" = 'group body'
