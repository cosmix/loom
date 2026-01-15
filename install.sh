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

GITHUB_REPO="cosmix/loom"
GITHUB_RELEASES="https://github.com/${GITHUB_REPO}/releases/latest/download"

print_banner() {
	echo ""
	echo -e "${BOLD}Claude Code Setup${NC}"
	echo -e "${DIM}agents + skills + config${NC}"
	echo ""
}

step() { echo -e "${BLUE}::${NC} $1"; }
ok() { echo -e "   ${GREEN}ok${NC} $1"; }
warn() { echo -e "   ${YELLOW}--${NC} $1"; }
info() { echo -e "   ${DIM}$1${NC}"; }
backup() { echo -e "   ${CYAN}>>${NC} backed up ${DIM}→ $1${NC}"; }

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

is_curl_pipe() {
	# Check if running from curl pipe (SCRIPT_DIR won't have our files)
	[[ ! -d "$SCRIPT_DIR/agents" ]] && [[ ! -d "$SCRIPT_DIR/skills" ]]
}

check_dependencies() {
	if ! command -v unzip &>/dev/null; then
		echo -e "${RED}!!${NC} unzip is required but not installed"
		echo -e "   Install it with: apt install unzip / brew install unzip"
		exit 1
	fi
}

download_file() {
	local url="$1"
	local dest="$2"

	if command -v curl &>/dev/null; then
		curl -fsSL "$url" -o "$dest"
	elif command -v wget &>/dev/null; then
		wget -q "$url" -O "$dest"
	else
		echo -e "   ${RED}!!${NC} Neither curl nor wget available"
		return 1
	fi
}

download_and_extract_zip() {
	local url="$1"
	local dest_dir="$2"
	local temp_zip="/tmp/loom_temp_$$.zip"

	download_file "$url" "$temp_zip" || return 1

	mkdir -p "$dest_dir"
	unzip -q -o "$temp_zip" -d "$dest_dir"
	rm -f "$temp_zip"
}

build_skill_index() {
	local index_builder="$CLAUDE_DIR/hooks/loom/skill-index-builder.sh"
	if [[ -x "$index_builder" ]]; then
		if "$index_builder" >/dev/null 2>&1; then
			ok "skill keyword index built"
		else
			warn "failed to build skill keyword index"
		fi
	fi
}

install_agents_remote() {
	step "downloading agents"

	backup_if_exists "$CLAUDE_DIR/agents" || true
	download_and_extract_zip "${GITHUB_RELEASES}/agents.zip" "$CLAUDE_DIR/agents" || {
		warn "failed to download agents, trying raw files..."
		# Fallback: clone just agents directory
		return 1
	}

	local count
	count=$(find "$CLAUDE_DIR/agents" -name "*.md" 2>/dev/null | wc -l | tr -d ' ')
	ok "$count agents installed"
}

install_skills_remote() {
	step "downloading skills"

	backup_if_exists "$CLAUDE_DIR/skills" || true
	download_and_extract_zip "${GITHUB_RELEASES}/skills.zip" "$CLAUDE_DIR/skills" || {
		warn "failed to download skills"
		return 1
	}

	local count
	count=$(find "$CLAUDE_DIR/skills" -name "SKILL.md" 2>/dev/null | wc -l | tr -d ' ')
	ok "$count skills installed"
}

install_claude_md_remote() {
	step "downloading CLAUDE.md"

	local claude_md="$CLAUDE_DIR/CLAUDE.md"
	local temp_file="/tmp/CLAUDE.md.template.$$"

	backup_if_exists "$claude_md" || true

	download_file "${GITHUB_RELEASES}/CLAUDE.md.template" "$temp_file" || {
		warn "failed to download CLAUDE.md.template"
		return 1
	}

	{
		echo "# ───────────────────────────────────────────────────────────"
		echo "# claude-loom | installed $(date '+%Y-%m-%d %H:%M:%S')"
		echo "# ───────────────────────────────────────────────────────────"
		echo ""
		cat "$temp_file"
	} >"$claude_md"

	rm -f "$temp_file"
	ok "CLAUDE.md installed"
}

install_hooks_remote() {
	step "downloading hooks"

	# All hooks go to loom/ subdirectory to keep them separate from user hooks
	local hooks_dir="$CLAUDE_DIR/hooks/loom"
	mkdir -p "$hooks_dir"

	# All loom hooks
	local all_hooks=(
		"session-start.sh"
		"post-tool-use.sh"
		"pre-compact.sh"
		"session-end.sh"
		"learning-validator.sh"
		"subagent-stop.sh"
		"commit-guard.sh"
		"ask-user-pre.sh"
		"ask-user-post.sh"
		"prefer-modern-tools.sh"
		"skill-index-builder.sh"
		"skill-trigger.sh"
	)
	local downloaded=0

	for hook in "${all_hooks[@]}"; do
		if download_file "${GITHUB_RELEASES}/$hook" "$hooks_dir/$hook" 2>/dev/null; then
			chmod +x "$hooks_dir/$hook"
			((++downloaded))
		fi
	done

	if [[ $downloaded -eq 0 ]]; then
		warn "failed to download hooks"
		return 1
	fi

	ok "$downloaded hooks installed"

	# Build skill keyword index
	build_skill_index
}

check_requirements() {
	step "checking source files"

	[[ -d "$SCRIPT_DIR/agents" ]] || {
		echo -e "   ${RED}!!${NC} agents/ not found"
		exit 1
	}
	[[ -d "$SCRIPT_DIR/skills" ]] || {
		echo -e "   ${RED}!!${NC} skills/ not found"
		exit 1
	}
	[[ -f "$SCRIPT_DIR/CLAUDE.md.template" ]] || {
		echo -e "   ${RED}!!${NC} CLAUDE.md.template not found"
		exit 1
	}

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
		echo "# claude-loom | installed $(date '+%Y-%m-%d %H:%M:%S')"
		echo "# ───────────────────────────────────────────────────────────"
		echo ""
		cat "$SCRIPT_DIR/CLAUDE.md.template"
	} >"$claude_md"

	ok "CLAUDE.md installed"
}

install_hooks() {
	step "installing hooks"

	# All hooks go to loom/ subdirectory to keep them separate from user hooks
	local hooks_dir="$CLAUDE_DIR/hooks/loom"
	mkdir -p "$hooks_dir"

	# All loom hooks
	local all_hooks=(
		"session-start.sh"
		"post-tool-use.sh"
		"pre-compact.sh"
		"session-end.sh"
		"learning-validator.sh"
		"subagent-stop.sh"
		"commit-guard.sh"
		"ask-user-pre.sh"
		"ask-user-post.sh"
		"prefer-modern-tools.sh"
		"skill-index-builder.sh"
		"skill-trigger.sh"
	)

	local count=0

	if [[ -d "$SCRIPT_DIR/hooks" ]]; then
		for hook_name in "${all_hooks[@]}"; do
			local hook="$SCRIPT_DIR/hooks/$hook_name"
			if [[ -f "$hook" ]]; then
				cp "$hook" "$hooks_dir/"
				chmod +x "$hooks_dir/$hook_name"
				((++count))
			fi
		done
	fi

	ok "$count hooks installed"

	# Build skill keyword index
	build_skill_index
}

install_loom_local() {
	step "installing loom CLI"

	local install_dir="$HOME/.local/bin"
	local loom_bin="$install_dir/loom"
	local local_loom="$SCRIPT_DIR/loom/target/release/loom"

	# Check for local binary first
	if [[ -x "$local_loom" ]]; then
		# Create install directory if needed
		if [[ ! -d "$install_dir" ]]; then
			mkdir -p "$install_dir"
			info "created $install_dir"
		fi

		cp "$local_loom" "$loom_bin"
		chmod +x "$loom_bin"
		ok "loom CLI installed from local build"

		# Check if ~/.local/bin is in PATH
		if [[ ":$PATH:" != *":$install_dir:"* ]]; then
			info "add $install_dir to your PATH to use loom"
		fi
		return 0
	fi

	# No local binary, fall back to download
	info "no local build found, downloading from GitHub..."
	install_loom_remote
}

install_loom_remote() {
	step "installing loom CLI"

	local install_dir="$HOME/.local/bin"
	local loom_bin="$install_dir/loom"

	# Create install directory if needed
	if [[ ! -d "$install_dir" ]]; then
		mkdir -p "$install_dir"
		info "created $install_dir"
	fi

	# Detect platform and architecture
	local os arch target
	os="$(uname -s)"
	arch="$(uname -m)"

	case "$os" in
	Linux)
		case "$arch" in
		x86_64)
			# Detect libc type
			if ldd --version 2>&1 | grep -q musl; then
				target="loom-x86_64-unknown-linux-musl"
			else
				target="loom-x86_64-unknown-linux-gnu"
			fi
			;;
		aarch64 | arm64)
			target="loom-aarch64-unknown-linux-gnu"
			;;
		*)
			warn "unsupported architecture: $arch"
			info "loom CLI not installed"
			return 0
			;;
		esac
		;;
	Darwin)
		case "$arch" in
		x86_64)
			target="loom-x86_64-apple-darwin"
			;;
		arm64 | aarch64)
			target="loom-aarch64-apple-darwin"
			;;
		*)
			warn "unsupported architecture: $arch"
			info "loom CLI not installed"
			return 0
			;;
		esac
		;;
	*)
		warn "unsupported platform: $os (only Linux and macOS are supported)"
		info "loom CLI not installed"
		return 0
		;;
	esac

	# Download loom binary
	local download_url="${GITHUB_RELEASES}/$target"
	info "downloading $target"

	if command -v curl &>/dev/null; then
		if ! curl -fsSL "$download_url" -o "$loom_bin"; then
			warn "failed to download loom CLI"
			info "you can manually install from: $download_url"
			return 0
		fi
	elif command -v wget &>/dev/null; then
		if ! wget -q "$download_url" -O "$loom_bin"; then
			warn "failed to download loom CLI"
			info "you can manually install from: $download_url"
			return 0
		fi
	else
		warn "curl or wget required to download loom"
		info "install curl or wget, then download from: $download_url"
		return 0
	fi

	# Make executable
	chmod +x "$loom_bin"

	ok "loom CLI installed to $install_dir"

	# Check if ~/.local/bin is in PATH
	if [[ ":$PATH:" != *":$install_dir:"* ]]; then
		info "add $install_dir to your PATH to use loom"
	fi
}

cleanup_backups() {
	local backups=()

	# Find all backups created during this installation
	while IFS= read -r -d '' file; do
		backups+=("$file")
	done < <(find "$CLAUDE_DIR" -maxdepth 2 -name "*.bak.${TIMESTAMP}" -print0 2>/dev/null)

	if [[ ${#backups[@]} -eq 0 ]]; then
		return 0
	fi

	echo ""
	echo -en "   ${BOLD}delete backup files? [y/N]${NC} "
	read -r response
	if [[ "$response" =~ ^[Yy]$ ]]; then
		for backup in "${backups[@]}"; do
			rm -rf "$backup"
		done
		ok "backups deleted"
	else
		info "backups kept at ~/.claude/*.bak.${TIMESTAMP}"
	fi
}

print_summary() {
	echo ""
	echo -e "${GREEN}done.${NC}"
	echo ""
	echo -e "${DIM}installed to ~/.claude/${NC}"
	echo -e "  agents/      ${DIM}specialized subagents${NC}"
	echo -e "  skills/      ${DIM}reusable capabilities${NC}"
	echo -e "  hooks/       ${DIM}Claude Code event hooks${NC}"
	echo -e "  CLAUDE.md    ${DIM}orchestration rules${NC}"
	echo ""
	echo -e "${DIM}installed to ~/.local/bin/${NC}"
	echo -e "  loom         ${DIM}parallel work orchestrator${NC}"
	echo ""
	echo -e "${DIM}hooks are auto-configured when you run:${NC}"
	echo -e "  ${CYAN}loom init${NC} <plan.md>    ${DIM}in your project${NC}"
	echo ""
	echo -e "run ${CYAN}loom run${NC} to start"
	echo ""
}

main() {
	print_banner

	if is_curl_pipe; then
		info "running from curl pipe, downloading from GitHub..."
		check_dependencies
		ensure_claude_dir
		install_agents_remote
		install_skills_remote
		install_claude_md_remote
		install_hooks_remote
		install_loom_remote
	else
		check_requirements
		confirm_overwrites
		ensure_claude_dir
		install_agents
		install_skills
		install_claude_md
		install_hooks
		install_loom_local
		cleanup_backups
	fi

	print_summary
}

main "$@"