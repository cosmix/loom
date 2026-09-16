#!/usr/bin/env bash
# Extract the body of a single release's section from a Keep a Changelog style
# CHANGELOG.md. Sections are grouped by minor version, e.g. `## [0.8.x]`, with
# an optional ` - <date>` suffix; `## [Unreleased]` is never matched.
#
# Usage: changelog-section.sh <changelog-path> <version>
#   <version> must look like X.Y.Z (e.g. 0.8.1).
#
# Matches, in order: an exact `## [X.Y.Z]` heading, else the minor-group
# heading `## [X.Y.x]`. Prints the section body (everything between the
# heading and the next `## ` heading or EOF) with leading/trailing blank
# lines stripped. Exits 1 with no output if nothing matches.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <changelog-path> <version>" >&2
  exit 1
fi

changelog_path=$1
version=$2

if [ ! -f "$changelog_path" ]; then
  echo "changelog not found: $changelog_path" >&2
  exit 1
fi

if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  exit 1
fi

minor_group="${version%.*}.x"

awk -v exact="$version" -v group="$minor_group" '
  function trim(s) {
    sub(/^[ \t]+/, "", s)
    sub(/[ \t]+$/, "", s)
    return s
  }

  # A heading line looks like: `## [<label>]` optionally followed by
  # ` - <date>` (or any other trailing text after the closing bracket).
  # Any other `## ` heading (no brackets) also ends a section, so a future
  # non-version heading cannot bleed the body past its boundary.
  /^##[^#]/ {
    if (in_section) {
      exit
    }
    if ($0 !~ /^##[ \t]*\[/) {
      next
    }
    line = $0
    sub(/^##[ \t]*\[/, "", line)
    close_pos = index(line, "]")
    if (close_pos > 0) {
      label = trim(substr(line, 1, close_pos - 1))
      if (label == exact || label == group) {
        in_section = 1
        next
      }
    }
    next
  }

  in_section { buf[++n] = $0 }

  END {
    if (!in_section && n == 0) {
      exit 1
    }
    start = 1
    end = n
    while (start <= end && trim(buf[start]) == "") start++
    while (end >= start && trim(buf[end]) == "") end--
    if (start > end) {
      exit 1
    }
    for (i = start; i <= end; i++) print buf[i]
  }
' "$changelog_path"
