#!/usr/bin/env bash
set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLAUDE_DIR="$HOME/.claude"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

print_banner() {
    echo ""
    echo -e "${BOLD}Claude Code Setup${NC}"
    echo -e "${DIM}agents + skills + config${NC}"
    echo ""
}

step()    { echo -e "${BLUE}::${NC} $1"; }
ok()      { echo -e "   ${GREEN}ok${NC} $1"; }
warn()    { echo -e "   ${YELLOW}--${NC} $1"; }
info()    { echo -e "   ${DIM}$1${NC}"; }
backup()  { echo -e "   ${CYAN}>>${NC} backed up ${DIM}→ $1${NC}"; }

backup_if_exists() {
    local path="$1"
    if [[ -e "$path" ]]; then
        local backup_path="${path}.bak.${TIMESTAMP}"
        mv "$path" "$backup_path"
        backup "$(basename "$backup_path")"
        return 0
    fi
    return 1
}

check_requirements() {
    step "checking source files"

    [[ -d "$SCRIPT_DIR/agents" ]] || { echo -e "   ${RED}!!${NC} agents/ not found"; exit 1; }
    [[ -d "$SCRIPT_DIR/skills" ]] || { echo -e "   ${RED}!!${NC} skills/ not found"; exit 1; }
    [[ -f "$SCRIPT_DIR/CLAUDE.template.md" ]] || { echo -e "   ${RED}!!${NC} CLAUDE.template.md not found"; exit 1; }

    ok "all files present"
}

confirm_overwrites() {
    local found=()

    [[ -d "$CLAUDE_DIR/agents" ]] && found+=("agents/")
    [[ -d "$CLAUDE_DIR/skills" ]] && found+=("skills/")
    [[ -f "$CLAUDE_DIR/CLAUDE.md" ]] && found+=("CLAUDE.md")

    if [[ ${#found[@]} -eq 0 ]]; then
        return 0
    fi

    echo ""
    warn "the following will be replaced in ~/.claude/:"
    for item in "${found[@]}"; do
        echo -e "      ${DIM}$item${NC}"
    done
    info "backups will be saved as *.bak.$TIMESTAMP"
    echo ""
    echo -en "   ${BOLD}proceed? [y/N]${NC} "
    read -r response
    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        echo ""
        info "cancelled"
        exit 0
    fi
}

ensure_claude_dir() {
    step "preparing $CLAUDE_DIR"

    if [[ ! -d "$CLAUDE_DIR" ]]; then
        mkdir -p "$CLAUDE_DIR"
        ok "created $CLAUDE_DIR"
    else
        ok "$CLAUDE_DIR exists"
    fi
}

install_agents() {
    step "installing agents"

    backup_if_exists "$CLAUDE_DIR/agents" || true
    cp -r "$SCRIPT_DIR/agents" "$CLAUDE_DIR/"

    local count
    count=$(find "$CLAUDE_DIR/agents" -name "*.md" | wc -l | tr -d ' ')
    ok "$count agents installed"
}

install_skills() {
    step "installing skills"

    backup_if_exists "$CLAUDE_DIR/skills" || true
    cp -r "$SCRIPT_DIR/skills" "$CLAUDE_DIR/"

    local count
    count=$(find "$CLAUDE_DIR/skills" -name "SKILL.md" | wc -l | tr -d ' ')
    ok "$count skills installed"
}

install_claude_md() {
    step "configuring CLAUDE.md"

    local claude_md="$CLAUDE_DIR/CLAUDE.md"

    backup_if_exists "$claude_md" || true

    {
        echo "# ───────────────────────────────────────────────────────────"
        echo "# claude-code-setup | installed $(date '+%Y-%m-%d %H:%M:%S')"
        echo "# ───────────────────────────────────────────────────────────"
        echo ""
        cat "$SCRIPT_DIR/CLAUDE.template.md"
    } > "$claude_md"

    ok "CLAUDE.md installed"
}

print_summary() {
    echo ""
    echo -e "${GREEN}done.${NC}"
    echo ""
    echo -e "${DIM}installed to ~/.claude/${NC}"
    echo -e "  agents/      ${DIM}specialized subagents${NC}"
    echo -e "  skills/      ${DIM}reusable capabilities${NC}"
    echo -e "  CLAUDE.md    ${DIM}orchestration rules${NC}"
    echo ""
    echo -e "${DIM}backups saved as *.bak.${TIMESTAMP}${NC}"
    echo ""
    echo -e "run ${CYAN}claude${NC} to start"
    echo ""
}

main() {
    print_banner
    check_requirements
    confirm_overwrites
    ensure_claude_dir
    install_agents
    install_skills
    install_claude_md
    print_summary
}

main "$@"