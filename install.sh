#!/usr/bin/env bash
set -euo pipefail

# Colors - minimal palette
R='\033[0;31m' # errors
G='\033[0;32m' # success
Y='\033[0;33m' # warnings
C='\033[0;36m' # accent
B='\033[1m'    # bold
D='\033[2m'    # dim
N='\033[0m'    # reset

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLAUDE_DIR="$HOME/.claude"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

GITHUB_REPO="cosmix/loom"
GITHUB_RELEASES="https://github.com/${GITHUB_REPO}/releases/latest/download"

# Component counts (updated during install)
COUNT_AGENTS=0
COUNT_SKILLS=0
COUNT_HOOKS=0

print_banner() {
	cat <<'EOF'

   ╷
   │  ┌─┐┌─┐┌┬┐
   │  │ ││ ││││
   ┴─┘└─┘└─┘┴ ┴

EOF
	echo -e "   ${D}Agent orchestration for Claude Code${N}"
	echo ""
}

print_components() {
	echo -e "   ${D}components${N}"
	echo -e "   ${C}cli${N}      parallel work orchestrator"
	echo -e "   ${C}agents${N}   specialized subagents"
	echo -e "   ${C}skills${N}   domain knowledge modules"
	echo -e "   ${C}hooks${N}    session lifecycle events"
	echo -e "   ${C}config${N}   orchestration rules"
	echo ""
}

# Progress indicators
step() {
	echo -e "   ${C}›${N} $1"
}

ok() {
	echo -e "   ${G}✓${N} $1"
}

warn() {
	echo -e "   ${Y}!${N} $1"
}

err() {
	echo -e "   ${R}✗${N} $1"
}

info() {
	echo -e "     ${D}$1${N}"
}

backup_msg() {
	echo -e "     ${D}backed up → $1${N}"
}

backup_if_exists() {
	local path="$1"
	if [[ -e "$path" ]]; then
		local backup_path="${path}.bak.${TIMESTAMP}"
		mv "$path" "$backup_path"
		backup_msg "$(basename "$backup_path")"
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
		err "unzip is required but not installed"
		info "install with: apt install unzip / brew install unzip"
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
		err "neither curl nor wget available"
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
	[[ -x "$index_builder" ]] && "$index_builder" >/dev/null 2>&1 || true
}

install_agents_remote() {
	step "agents"

	backup_if_exists "$CLAUDE_DIR/agents" || true
	download_and_extract_zip "${GITHUB_RELEASES}/agents.zip" "$CLAUDE_DIR/agents" || {
		warn "failed to download agents"
		return 1
	}

	COUNT_AGENTS=$(find "$CLAUDE_DIR/agents" -name "*.md" 2>/dev/null | wc -l | tr -d ' ')
	ok "$COUNT_AGENTS agents"
}

install_skills_remote() {
	step "skills"

	backup_if_exists "$CLAUDE_DIR/skills" || true
	download_and_extract_zip "${GITHUB_RELEASES}/skills.zip" "$CLAUDE_DIR/skills" || {
		warn "failed to download skills"
		return 1
	}

	COUNT_SKILLS=$(find "$CLAUDE_DIR/skills" -name "SKILL.md" 2>/dev/null | wc -l | tr -d ' ')
	ok "$COUNT_SKILLS skills"
}

install_claude_md_remote() {
	step "config"

	local claude_md="$CLAUDE_DIR/CLAUDE.md"
	local temp_file="/tmp/CLAUDE.md.template.$$"

	backup_if_exists "$claude_md" || true

	download_file "${GITHUB_RELEASES}/CLAUDE.md.template" "$temp_file" || {
		warn "failed to download config"
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
	ok "CLAUDE.md"
}

install_hooks_remote() {
	step "hooks"

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
		"commit-guard.sh"
		"commit-filter.sh"
		"ask-user-pre.sh"
		"ask-user-post.sh"
		"prefer-modern-tools.sh"
		"skill-index-builder.sh"
		"skill-trigger.sh"
	)

	for hook in "${all_hooks[@]}"; do
		if download_file "${GITHUB_RELEASES}/$hook" "$hooks_dir/$hook" 2>/dev/null; then
			chmod +x "$hooks_dir/$hook"
			((++COUNT_HOOKS))
		fi
	done

	if [[ $COUNT_HOOKS -eq 0 ]]; then
		warn "failed to download hooks"
		return 1
	fi

	ok "$COUNT_HOOKS hooks"

	# Build skill keyword index
	build_skill_index
}

check_requirements() {
	[[ -d "$SCRIPT_DIR/agents" ]] || { err "agents/ not found"; exit 1; }
	[[ -d "$SCRIPT_DIR/skills" ]] || { err "skills/ not found"; exit 1; }
	[[ -f "$SCRIPT_DIR/CLAUDE.md.template" ]] || { err "CLAUDE.md.template not found"; exit 1; }
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
	warn "existing files will be replaced:"
	for item in "${found[@]}"; do
		echo -e "     ${D}~/.claude/$item${N}"
	done
	echo ""
	info "backups saved as *.bak.$TIMESTAMP"
	echo ""
	echo -en "   ${B}proceed? [y/N]${N} "
	read -r response
	if [[ ! "$response" =~ ^[Yy]$ ]]; then
		echo ""
		info "cancelled"
		exit 0
	fi
}

ensure_claude_dir() {
	mkdir -p "$CLAUDE_DIR"
}

install_agents() {
	step "agents"

	backup_if_exists "$CLAUDE_DIR/agents" || true
	cp -r "$SCRIPT_DIR/agents" "$CLAUDE_DIR/"

	COUNT_AGENTS=$(find "$CLAUDE_DIR/agents" -name "*.md" | wc -l | tr -d ' ')
	ok "$COUNT_AGENTS agents"
}

install_skills() {
	step "skills"

	backup_if_exists "$CLAUDE_DIR/skills" || true
	cp -r "$SCRIPT_DIR/skills" "$CLAUDE_DIR/"

	COUNT_SKILLS=$(find "$CLAUDE_DIR/skills" -name "SKILL.md" | wc -l | tr -d ' ')
	ok "$COUNT_SKILLS skills"
}

install_claude_md() {
	step "config"

	local claude_md="$CLAUDE_DIR/CLAUDE.md"

	backup_if_exists "$claude_md" || true

	{
		echo "# ───────────────────────────────────────────────────────────"
		echo "# claude-loom | installed $(date '+%Y-%m-%d %H:%M:%S')"
		echo "# ───────────────────────────────────────────────────────────"
		echo ""
		cat "$SCRIPT_DIR/CLAUDE.md.template"
	} >"$claude_md"

	ok "CLAUDE.md"
}

install_hooks() {
	step "hooks"

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
		"commit-guard.sh"
		"commit-filter.sh"
		"ask-user-pre.sh"
		"ask-user-post.sh"
		"prefer-modern-tools.sh"
		"skill-index-builder.sh"
		"skill-trigger.sh"
	)

	if [[ -d "$SCRIPT_DIR/hooks" ]]; then
		for hook_name in "${all_hooks[@]}"; do
			local hook="$SCRIPT_DIR/hooks/$hook_name"
			if [[ -f "$hook" ]]; then
				cp "$hook" "$hooks_dir/"
				chmod +x "$hooks_dir/$hook_name"
				((++COUNT_HOOKS))
			fi
		done
	fi

	ok "$COUNT_HOOKS hooks"

	# Build skill keyword index
	build_skill_index
}

install_loom_local() {
	step "cli"

	local install_dir="$HOME/.local/bin"
	local loom_bin="$install_dir/loom"
	local local_loom="$SCRIPT_DIR/loom/target/release/loom"

	# Check for local binary first
	if [[ -x "$local_loom" ]]; then
		mkdir -p "$install_dir"
		cp "$local_loom" "$loom_bin"
		chmod +x "$loom_bin"
		ok "loom"

		if [[ ":$PATH:" != *":$install_dir:"* ]]; then
			info "add ~/.local/bin to PATH"
		fi
		return 0
	fi

	# No local binary, fall back to download
	info "no local build, downloading..."
	install_loom_remote
}

install_loom_remote() {
	step "cli"

	local install_dir="$HOME/.local/bin"
	local loom_bin="$install_dir/loom"

	mkdir -p "$install_dir"

	# Detect platform and architecture
	local os arch target
	os="$(uname -s)"
	arch="$(uname -m)"

	case "$os" in
	Linux)
		case "$arch" in
		x86_64)
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
			warn "unsupported arch: $arch"
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
			warn "unsupported arch: $arch"
			return 0
			;;
		esac
		;;
	*)
		warn "unsupported platform: $os"
		return 0
		;;
	esac

	local download_url="${GITHUB_RELEASES}/$target"

	if command -v curl &>/dev/null; then
		if ! curl -fsSL "$download_url" -o "$loom_bin"; then
			warn "download failed"
			info "manual install: $download_url"
			return 0
		fi
	elif command -v wget &>/dev/null; then
		if ! wget -q "$download_url" -O "$loom_bin"; then
			warn "download failed"
			info "manual install: $download_url"
			return 0
		fi
	else
		warn "curl or wget required"
		return 0
	fi

	chmod +x "$loom_bin"
	ok "loom"

	if [[ ":$PATH:" != *":$install_dir:"* ]]; then
		info "add ~/.local/bin to PATH"
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
	echo -en "   ${B}delete backup files? [y/N]${N} "
	read -r response </dev/tty
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
	echo -e "   ${G}installed${N}"
	echo ""
	echo -e "   ${D}~/.claude/${N}"
	echo -e "     agents/     ${D}$COUNT_AGENTS specialized subagents${N}"
	echo -e "     skills/     ${D}$COUNT_SKILLS domain knowledge modules${N}"
	echo -e "     hooks/      ${D}$COUNT_HOOKS lifecycle event handlers${N}"
	echo -e "     CLAUDE.md   ${D}orchestration rules${N}"
	echo ""
	echo -e "   ${D}~/.local/bin/${N}"
	echo -e "     loom        ${D}parallel work orchestrator${N}"
	echo ""
	echo -e "   ${D}next steps${N}"
	echo -e "     ${C}loom init${N} <plan.md>   ${D}initialize a project${N}"
	echo -e "     ${C}loom run${N}              ${D}start orchestration${N}"
	echo ""
}

main() {
	print_banner
	print_components

	if is_curl_pipe; then
		info "downloading from github"
		echo ""
		check_dependencies
		ensure_claude_dir
		install_loom_remote
		install_agents_remote
		install_skills_remote
		install_hooks_remote
		install_claude_md_remote
	else
		check_requirements
		confirm_overwrites
		ensure_claude_dir
		install_loom_local
		install_agents
		install_skills
		install_hooks
		install_claude_md
		cleanup_backups
	fi

	print_summary
}

main "$@"