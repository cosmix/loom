#!/usr/bin/env bash
set -euo pipefail

# Build and install loom locally for development
# This compiles an optimized development binary and runs install.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

cleanup_backups() {
    local backups=()
    local root file name response

    for root in "$HOME/.claude" "$HOME/.codex"; do
        [[ -d "$root" ]] || continue
        while IFS= read -r -d '' file; do
            name="${file##*/}"
            if [[ "$name" =~ \.bak\.[0-9]{8}-[0-9]{6}$ ]]; then
                backups+=("$file")
            fi
        done < <(find "$root" -type f -name '*.bak.*' -print0 2>/dev/null)
    done

    [[ ${#backups[@]} -gt 0 ]] || return 0

    echo ""
    echo "Loom backup files:"
    for file in ${backups[@]+"${backups[@]}"}; do
        echo "  $file"
    done

    if ! exec 3<>/dev/tty 2>/dev/null; then
        echo "Backups kept (no interactive terminal)."
        return 0
    fi

    echo -n "Delete these backup files? [y/N] "
    if ! IFS= read -r response <&3; then
        response=""
    fi
    exec 3>&-

    if [[ "$response" =~ ^[Yy]$ ]]; then
        for file in ${backups[@]+"${backups[@]}"}; do
            rm -f "$file"
        done
        echo "Backups deleted."
    else
        echo "Backups kept."
    fi
}

# Kill any running loom daemon
if pgrep -x loom &>/dev/null; then
    echo "Killing loom daemon..."
    pkill -x loom 2>/dev/null || true
    sleep 1
fi

# Override the repository's four-job cap for local installs, while allowing
# callers to limit concurrency on machines with less memory.
build_jobs="${CARGO_BUILD_JOBS:-$(nproc 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)}"
echo "Building development binary (jobs: $build_jobs, LTO off)..."
cd "$SCRIPT_DIR/loom"
# Keep release optimization and the installer's target/release path, but avoid
# whole-program LTO and single-unit codegen on every development rebuild.
CARGO_BUILD_JOBS="$build_jobs" \
CARGO_PROFILE_RELEASE_LTO=off \
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
CARGO_PROFILE_RELEASE_INCREMENTAL=true \
cargo build --release

echo ""
echo "Running install.sh..."
cd "$SCRIPT_DIR"
bash ./install.sh "$@"

cleanup_backups
