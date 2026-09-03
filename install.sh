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
CODEX_DIR="$HOME/.codex"

GITHUB_REPO="cosmix/loom"
GITHUB_RELEASES="https://github.com/${GITHUB_REPO}/releases/latest/download"

SKILLS_MODE=""
SKILLS_MODE_EXPLICIT=0

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

print_usage() {
	cat <<'EOF'
Usage: install.sh [--skills core|all]

Options:
  --skills core  Install core skills to ~/.claude/skills and catalog the rest (default)
  --skills all   Install every loom skill to ~/.claude/skills
  -h, --help     Show this help message
EOF
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

is_curl_pipe() {
	# Check if running from curl pipe (SCRIPT_DIR won't have our files)
	[[ ! -d "$SCRIPT_DIR/agents" ]] && [[ ! -d "$SCRIPT_DIR/skills" ]]
}

check_runtime_tools() {
	if ! command -v jq &>/dev/null; then
		err "jq is required by loom's Claude Code hooks but is not installed"
		info "install with: apt install jq / brew install jq"
		exit 1
	fi
	local missing_tools=()
	command -v rg &>/dev/null || missing_tools+=("ripgrep (rg)")
	command -v fd &>/dev/null || missing_tools+=("fd")
	if ((${#missing_tools[@]} > 0)); then
		warn "search tools missing: ${missing_tools[*]} - the installed CLAUDE.md steers agents to rg/fd over grep/find"
		info "install with: apt install ripgrep fd-find (then symlink fdfind to fd) / brew install ripgrep fd"
	fi
}

read_recorded_skills_mode() {
	local config_file="$CLAUDE_DIR/loom-install.toml"
	local line

	[[ -f "$config_file" ]] || return 0

	while IFS= read -r line || [[ -n "$line" ]]; do
		line="${line#"${line%%[![:space:]]*}"}"
		line="${line%"${line##*[![:space:]]}"}"
		[[ "$line" =~ ^skills([[:space:]]|=) ]] || continue

		if [[ "$line" =~ ^skills[[:space:]]*=[[:space:]]*\"(core|all)\"[[:space:]]*$ ]]; then
			SKILLS_MODE="${BASH_REMATCH[1]}"
			return 0
		fi

		err "invalid skills setting in $config_file"
		return 1
	done < "$config_file"
}

parse_args() {
	while [[ $# -gt 0 ]]; do
		case "$1" in
			--skills)
				if [[ $# -lt 2 || "$2" == -* ]]; then
					err "--skills requires core or all"
					return 1
				fi
				SKILLS_MODE="$2"
				SKILLS_MODE_EXPLICIT=1
				shift 2
				;;
			--skills=*)
				SKILLS_MODE="${1#--skills=}"
				SKILLS_MODE_EXPLICIT=1
				shift
				;;
			-h|--help)
				print_usage
				exit 0
				;;
			*)
				err "unknown option: $1"
				return 1
				;;
		esac
	done

	if [[ "$SKILLS_MODE_EXPLICIT" -eq 0 ]]; then
		read_recorded_skills_mode
	fi

	if [[ -z "$SKILLS_MODE" ]]; then
		SKILLS_MODE="core"
	fi

	case "$SKILLS_MODE" in
		core|all) ;;
		*)
			err "--skills must be core or all"
			return 1
			;;
	esac
}

check_requirements() {
	[[ -d "$SCRIPT_DIR/loom" ]] || { err "loom/ not found"; exit 1; }
}

confirm_overwrites() {
	local found=()

	[[ -d "$CLAUDE_DIR/agents" ]] && found+=("~/.claude/agents/ (loom-* only)")
	[[ -d "$CLAUDE_DIR/skills" ]] && found+=("~/.claude/skills/ (loom-* only)")
	[[ -d "$CLAUDE_DIR/loom-skill-catalog" ]] && found+=("~/.claude/loom-skill-catalog/")
	[[ -f "$CLAUDE_DIR/CLAUDE.md" ]] && found+=("~/.claude/CLAUDE.md")
	[[ -f "$CLAUDE_DIR/loom-install.toml" ]] && found+=("~/.claude/loom-install.toml")
	[[ -d "$CLAUDE_DIR/commands" ]] && found+=("~/.claude/commands/ (address.md, distill.md, pressure.md)")
	[[ -d "$CLAUDE_DIR/hooks/loom" ]] && found+=("~/.claude/hooks/loom")

	local found_other=()
	[[ -f "$CODEX_DIR/AGENTS.md" ]] && found_other+=("~/.codex/AGENTS.md")
	[[ -d "$CODEX_DIR/skills" ]] && found_other+=("~/.codex/skills")
	[[ -d "$CODEX_DIR/loom-skill-catalog" ]] && found_other+=("~/.codex/loom-skill-catalog")

	if [[ ${#found[@]} -eq 0 ]] && [[ ${#found_other[@]} -eq 0 ]]; then
		return 0
	fi

	echo ""
	warn "existing loom files may be updated:"
	# `${a[@]+"${a[@]}"}` guard: bash 3.2 (macOS) treats an empty array as unset
	# under `set -u`, so a bare "${a[@]}" aborts. Never write `${#a[@]-0}` here —
	# the length form takes no default and bash 5 rejects it as bad substitution.
	for item in ${found[@]+"${found[@]}"}; do
		echo -e "     ${D}$item${N}"
	done
	for item in ${found_other[@]+"${found_other[@]}"}; do
		echo -e "     ${D}$item${N}"
	done
	echo ""
	echo -en "   ${B}proceed? [y/N]${N} "
	read -r response
	if [[ ! "$response" =~ ^[Yy]$ ]]; then
		echo ""
		info "cancelled"
		exit 0
	fi
}

install_loom_local() {
	local install_dir="$HOME/.local/bin"
	local loom_bin="$install_dir/loom"
	local local_loom="$SCRIPT_DIR/loom/target/release/loom"

	# Check for local binary first
	if [[ -x "$local_loom" ]]; then
		mkdir -p "$install_dir"
		# Remove old binary first to avoid "Text file busy" when loom is running
		rm -f "$loom_bin"
		cp "$local_loom" "$loom_bin"
		chmod +x "$loom_bin"
		ok "loom"

		if [[ ":$PATH:" != *":$install_dir:"* ]]; then
			info "add ~/.local/bin to PATH"
		fi
		return 0
	fi

	# No local binary, fall back to the released binary and its embedded assets.
	info "no local build; downloading the release binary and using its embedded assets"
	install_loom_remote
}

install_loom_remote() {
	local install_dir="$HOME/.local/bin"
	local loom_bin="$install_dir/loom"

	mkdir -p "$install_dir"

	# Detect platform and architecture. The release workflow
	# (.github/workflows/release.yml) publishes exactly three binaries; the
	# updater's RELEASE_ASSETS (loom/src/commands/self_update/mod.rs)
	# is the other place naming them. Change all three together.
	local os arch target
	os="$(uname -s)"
	arch="$(uname -m)"

	case "$os" in
	Linux)
		case "$arch" in
		x86_64) target="loom-linux-x86_64" ;;
		esac
		;;
	Darwin)
		case "$arch" in
		x86_64) target="loom-darwin-x86_64" ;;
		arm64 | aarch64) target="loom-darwin-arm64" ;;
		esac
		;;
	esac

	if [[ -z "${target:-}" ]]; then
		err "no published loom binary for $os/$arch"
		info "build from source: git clone https://github.com/${GITHUB_REPO} && cd loom/loom && cargo build --release && cd .., then rerun install.sh"
		exit 1
	fi

	local download_url="${GITHUB_RELEASES}/$target"
	local temp_bin="${loom_bin}.tmp.$$"

	if command -v curl &>/dev/null; then
		if ! curl -fsSL "$download_url" -o "$temp_bin"; then
			rm -f "$temp_bin"
			err "download failed"
			info "manual install: $download_url"
			exit 1
		fi
	elif command -v wget &>/dev/null; then
		if ! wget -q "$download_url" -O "$temp_bin"; then
			rm -f "$temp_bin"
			err "download failed"
			info "manual install: $download_url"
			exit 1
		fi
	else
		err "curl or wget required"
		exit 1
	fi

	# Remove old binary first to avoid "Text file busy" when loom is running
	rm -f "$loom_bin"
	mv "$temp_bin" "$loom_bin"

	chmod +x "$loom_bin"
	ok "loom"

	if [[ ":$PATH:" != *":$install_dir:"* ]]; then
		info "add ~/.local/bin to PATH"
	fi
}

print_summary() {
	echo ""
	echo -e "   ${G}installed${N}"
	echo ""
	echo -e "   ${D}~/.claude/${N}"
	echo -e "     agents/     ${D}managed agents${N}"
	echo -e "     skills/     ${D}managed skills${N}"
	echo -e "     loom-skill-catalog/ ${D}catalogued skills${N}"
	echo -e "     hooks/      ${D}managed lifecycle event handlers${N}"
	echo -e "     commands/   ${D}managed slash commands${N}"
	echo -e "     CLAUDE.md   ${D}orchestration rules${N}"
	echo ""
	echo -e "   ${D}~/.codex/${N}"
	echo -e "     skills/     ${D}managed skills${N}"
	echo -e "     AGENTS.md   ${D}orchestration rules${N}"
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
	parse_args "$@"
	print_banner
	print_components
	check_runtime_tools

	step "cli"
	if is_curl_pipe; then
		info "downloading from github"
		echo ""
		install_loom_remote
	else
		check_requirements
		confirm_overwrites
		install_loom_local
	fi

	local loom_bin="$HOME/.local/bin/loom"
	[[ -x "$loom_bin" ]] || {
		err "loom binary was not installed at $loom_bin"
		exit 1
	}

	"$loom_bin" install-assets --help >/dev/null 2>&1 || {
		err "installed loom binary does not support install-assets (version skew; update the binary and rerun install.sh)"
		exit 1
	}

	"$loom_bin" install-assets --skills "$SKILLS_MODE"
	print_summary
}

if [[ "${LOOM_INSTALL_LIB_ONLY:-0}" != "1" ]]; then
	main "$@"
fi
