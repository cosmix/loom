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
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

GITHUB_REPO="cosmix/loom"
GITHUB_RELEASES="https://github.com/${GITHUB_REPO}/releases/latest/download"

# Component counts (updated during install)
COUNT_AGENTS=0
COUNT_SKILLS=0
COUNT_CATALOG_SKILLS=0
COUNT_HOOKS=0
COUNT_COMMANDS=0
SKILLS_MODE=""
SKILLS_MODE_EXPLICIT=0
CORE_SKILLS=()

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
	# Use the loom binary to build the index (faster, more robust than bash)
	local loom_bin="$HOME/.local/bin/loom"
	[[ -x "$loom_bin" ]] && "$loom_bin" skill-index >/dev/null 2>&1 || true
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

write_install_config() {
	{
		printf '%s\n' '# Managed by loom install.sh. Delete this file to reset the skills layout.'
		printf 'skills = "%s"\n' "$SKILLS_MODE"
	} > "$CLAUDE_DIR/loom-install.toml"
}

load_core_skills() {
	local manifest="$1"
	local line

	[[ -f "$manifest" ]] || return 1
	CORE_SKILLS=()

	while IFS= read -r line || [[ -n "$line" ]]; do
		line="${line#"${line%%[![:space:]]*}"}"
		line="${line%"${line##*[![:space:]]}"}"
		[[ -z "$line" || "$line" == \#* ]] && continue
		CORE_SKILLS+=("$line")
	done < "$manifest"
}

is_core_skill() {
	local name="$1"
	local core_skill

	for core_skill in ${CORE_SKILLS[@]+"${CORE_SKILLS[@]}"}; do
		[[ "$core_skill" == "$name" ]] && return 0
	done
	return 1
}

install_skills_from_source() {
	local source_dir="$1"
	local manifest="$2"
	local allow_missing_manifest="$3"
	local placement_mode="$SKILLS_MODE"
	local catalog_dir="$CLAUDE_DIR/loom-skill-catalog"
	local skill_dir name destination_dir
	local installed_skills=0
	local catalogued_skills=0

	step "skills"

	if ! load_core_skills "$manifest"; then
		if [[ "$allow_missing_manifest" == "true" ]]; then
			warn "core-skills.txt missing from release; installing all skills"
			placement_mode="all"
			# Propagate the actual layout to the global so write_install_config
			# (which runs after this function) records what was really placed.
			SKILLS_MODE="all"
		else
			err "core-skills.txt not found: $manifest"
			return 1
		fi
	fi

	mkdir -p "$CLAUDE_DIR/skills"
	if [[ "$placement_mode" == "core" ]]; then
		mkdir -p "$catalog_dir"
	else
		rm -rf "$catalog_dir"
	fi

	for skill_dir in "$source_dir"/loom-*/; do
		[[ -d "$skill_dir" ]] || continue
		name=$(basename "$skill_dir")
		[[ "$name" == loom-* ]] || continue

		if [[ "$placement_mode" == "core" ]] && ! is_core_skill "$name"; then
			destination_dir="$catalog_dir"
			((++catalogued_skills))
		else
			destination_dir="$CLAUDE_DIR/skills"
			((++installed_skills))
		fi

		# Every generated destination is a verified loom-* directory.
		# The trailing slash must be stripped: BSD cp copies a directory's
		# CONTENTS when the source ends in `/`, which on macOS would splatter
		# every SKILL.md straight into the destination root.
		rm -rf "$destination_dir/$name"
		cp -R "${skill_dir%/}" "$destination_dir/"
	done

	if [[ "$placement_mode" == "core" ]]; then
		# Only loom-* directories are considered for resident/catalog cleanup.
		for skill_dir in "$CLAUDE_DIR/skills"/loom-*/; do
			[[ -d "$skill_dir" ]] || continue
			name=$(basename "$skill_dir")
			if ! is_core_skill "$name"; then
				rm -rf "${skill_dir%/}"
			fi
		done

		for skill_dir in "$catalog_dir"/loom-*/; do
			[[ -d "$skill_dir" ]] || continue
			name=$(basename "$skill_dir")
			if is_core_skill "$name"; then
				rm -rf "${skill_dir%/}"
			fi
		done
	fi

	COUNT_SKILLS="$installed_skills"
	COUNT_CATALOG_SKILLS="$catalogued_skills"
	if [[ "$placement_mode" == "core" ]]; then
		ok "$COUNT_SKILLS skills, $COUNT_CATALOG_SKILLS catalogued"
	else
		ok "$COUNT_SKILLS skills"
	fi
}

install_agents_remote() {
	step "agents"

	mkdir -p "$CLAUDE_DIR/agents"
	download_and_extract_zip "${GITHUB_RELEASES}/agents.zip" "/tmp/loom_agents_$$" || {
		warn "failed to download agents"
		return 1
	}

	for agent_file in /tmp/loom_agents_$$/loom-*.md; do
		[ -f "$agent_file" ] || continue
		cp "$agent_file" "$CLAUDE_DIR/agents/"
	done
	rm -rf "/tmp/loom_agents_$$"

	COUNT_AGENTS=$(find "$CLAUDE_DIR/agents" -name "loom-*.md" 2>/dev/null | wc -l | tr -d ' ')
	ok "$COUNT_AGENTS agents"
}

install_skills_remote() {
	download_and_extract_zip "${GITHUB_RELEASES}/skills.zip" "/tmp/loom_skills_$$" || {
		warn "failed to download skills"
		return 1
	}

	install_skills_from_source "/tmp/loom_skills_$$" "/tmp/loom_skills_$$/core-skills.txt" "true"
	rm -rf "/tmp/loom_skills_$$"
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
		"_common.sh"
		"_read_discipline.sh"
		"_read_ledger.sh"
		"session-start.sh"
		"post-tool-use.sh"
		"pre-compact.sh"
		"session-end.sh"
		"subagent-stop.sh"
		"subagent-start.sh"
		"learning-validator.sh"
		"commit-guard.sh"
		"commit-filter.sh"
		"subagent-verify-guard.sh"
		"ask-user-pre.sh"
		"ask-user-post.sh"
		"prefer-modern-tools.sh"
		"skill-trigger.sh"
		"user-prompt-context.sh"
		"git-add-guard.sh"
		"worktree-isolation.sh"
		"worktree-file-guard.sh"
		"plans-path-guard.sh"
		"no-preexisting-failures.sh"
		"codex-forward-guard.sh"
		"codex-forward.sh"
		"loom-control-complete.sh"
		"stage-terminal-guard.sh"
		"spawn-guard.sh"
		"read-guard.sh"
		"poll-guard.sh"
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

	# Remove replaced bash script (now handled by loom skill-index)
	rm -f "$hooks_dir/skill-index-builder.sh"

	# Build skill keyword index
	build_skill_index
}

check_requirements() {
	[[ -d "$SCRIPT_DIR/agents" ]] || { err "agents/ not found"; exit 1; }
	[[ -d "$SCRIPT_DIR/skills" ]] || { err "skills/ not found"; exit 1; }
	[[ -f "$SCRIPT_DIR/skills/core-skills.txt" ]] || { err "skills/core-skills.txt not found"; exit 1; }
	[[ -f "$SCRIPT_DIR/CLAUDE.md.template" ]] || { err "CLAUDE.md.template not found"; exit 1; }
	[[ -f "$SCRIPT_DIR/commands/pressure.md" ]] || { err "commands/pressure.md not found"; exit 1; }
	[[ -f "$SCRIPT_DIR/commands/address.md" ]] || { err "commands/address.md not found"; exit 1; }
	[[ -f "$SCRIPT_DIR/commands/distill.md" ]] || { err "commands/distill.md not found"; exit 1; }
	[[ -f "$SCRIPT_DIR/codex/skills/pressure/SKILL.md" ]] || { err "codex/skills/pressure/SKILL.md not found"; exit 1; }
}

confirm_overwrites() {
	local found=()

	[[ -d "$CLAUDE_DIR/agents" ]] && found+=("agents/ (loom-* only)")
	[[ -d "$CLAUDE_DIR/skills" ]] && found+=("skills/ (loom-* only)")
	[[ -f "$CLAUDE_DIR/CLAUDE.md" ]] && found+=("CLAUDE.md")
	[[ -d "$CLAUDE_DIR/commands" ]] && found+=("commands/ (loom-owned commands)")

	local found_other=()
	[[ -d "$CODEX_DIR/skills/pressure" ]] && found_other+=("~/.codex/skills/pressure")

	if [[ ${#found[@]} -eq 0 ]] && [[ ${#found_other[@]} -eq 0 ]]; then
		return 0
	fi

	echo ""
	warn "existing loom files may be updated:"
	# `${a[@]+"${a[@]}"}` guard: bash 3.2 (macOS) treats an empty array as unset
	# under `set -u`, so a bare "${a[@]}" aborts. Never write `${#a[@]-0}` here —
	# the length form takes no default and bash 5 rejects it as bad substitution.
	for item in ${found[@]+"${found[@]}"}; do
		echo -e "     ${D}~/.claude/$item${N}"
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

ensure_claude_dir() {
	mkdir -p "$CLAUDE_DIR"
}

install_agents() {
	step "agents"

	mkdir -p "$CLAUDE_DIR/agents"
	for agent_file in "$SCRIPT_DIR/agents"/loom-*.md; do
		[ -f "$agent_file" ] || continue
		cp "$agent_file" "$CLAUDE_DIR/agents/"
	done

	COUNT_AGENTS=$(find "$CLAUDE_DIR/agents" -name "loom-*.md" 2>/dev/null | wc -l | tr -d ' ')
	ok "$COUNT_AGENTS agents"
}

install_skills() {
	install_skills_from_source "$SCRIPT_DIR/skills" "$SCRIPT_DIR/skills/core-skills.txt" "false"
}

install_commands() {
	step "commands"

	mkdir -p "$CLAUDE_DIR/commands"
	for cmd_file in "$SCRIPT_DIR/commands"/*.md; do
		[ -f "$cmd_file" ] || continue
		cp "$cmd_file" "$CLAUDE_DIR/commands/"
		((++COUNT_COMMANDS))
	done

	# Assert required command files were present in source and thus copied
	local missing=0
	for required in commands/pressure.md commands/address.md commands/distill.md; do
		[[ -f "$SCRIPT_DIR/$required" ]] || { err "$required not found in source"; missing=1; }
	done
	[[ $missing -eq 0 ]] || exit 1

	ok "$COUNT_COMMANDS commands"
}

install_codex_skill() {
	step "codex skill"

	# Assert source exists before touching the destination
	[[ -f "$SCRIPT_DIR/codex/skills/pressure/SKILL.md" ]] || {
		err "codex/skills/pressure/SKILL.md not found"
		exit 1
	}

	mkdir -p "$CODEX_DIR/skills/pressure"
	cp "$SCRIPT_DIR/codex/skills/pressure/SKILL.md" "$CODEX_DIR/skills/pressure/"

	ok "pressure skill"
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
		"_common.sh"
		"_read_discipline.sh"
		"_read_ledger.sh"
		"session-start.sh"
		"post-tool-use.sh"
		"pre-compact.sh"
		"session-end.sh"
		"subagent-stop.sh"
		"subagent-start.sh"
		"learning-validator.sh"
		"commit-guard.sh"
		"commit-filter.sh"
		"subagent-verify-guard.sh"
		"ask-user-pre.sh"
		"ask-user-post.sh"
		"prefer-modern-tools.sh"
		"skill-trigger.sh"
		"user-prompt-context.sh"
		"git-add-guard.sh"
		"worktree-isolation.sh"
		"worktree-file-guard.sh"
		"plans-path-guard.sh"
		"no-preexisting-failures.sh"
		"codex-forward-guard.sh"
		"codex-forward.sh"
		"loom-control-complete.sh"
		"stage-terminal-guard.sh"
		"spawn-guard.sh"
		"read-guard.sh"
		"poll-guard.sh"
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

	# Remove replaced bash script (now handled by loom skill-index)
	rm -f "$hooks_dir/skill-index-builder.sh"

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
	local temp_bin="${loom_bin}.tmp.$$"

	if command -v curl &>/dev/null; then
		if ! curl -fsSL "$download_url" -o "$temp_bin"; then
			rm -f "$temp_bin"
			warn "download failed"
			info "manual install: $download_url"
			return 0
		fi
	elif command -v wget &>/dev/null; then
		if ! wget -q "$download_url" -O "$temp_bin"; then
			rm -f "$temp_bin"
			warn "download failed"
			info "manual install: $download_url"
			return 0
		fi
	else
		warn "curl or wget required"
		return 0
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
	if [[ "$COUNT_CATALOG_SKILLS" -gt 0 ]]; then
		echo -e "     skills/     ${D}$COUNT_SKILLS indexed orchestration mechanics skills${N}"
		echo -e "     loom-skill-catalog/ ${D}$COUNT_CATALOG_SKILLS catalogued domain knowledge modules${N}"
	else
		echo -e "     skills/     ${D}$COUNT_SKILLS indexed orchestration and domain skills${N}"
	fi
	echo -e "     hooks/      ${D}$COUNT_HOOKS lifecycle event handlers${N}"
	echo -e "     commands/   ${D}$COUNT_COMMANDS slash commands${N}"
	echo -e "     CLAUDE.md   ${D}orchestration rules${N}"
	echo ""
	echo -e "   ${D}~/.codex/${N}"
	echo -e "     skills/     ${D}pressure skill${N}"
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

	if is_curl_pipe; then
		info "downloading from github"
		echo ""
		check_dependencies
		ensure_claude_dir
		install_loom_remote
		install_agents_remote
		install_skills_remote
	else
		check_requirements
		confirm_overwrites
		ensure_claude_dir
		install_loom_local
		install_agents
		install_skills
	fi

	write_install_config

	if is_curl_pipe; then
		install_hooks_remote
		install_claude_md_remote
	else
		install_hooks
		install_claude_md
		install_commands
		install_codex_skill
		cleanup_backups
	fi

	update_completions
	print_summary
}

update_completions() {
	local loom_bin="$HOME/.local/bin/loom"
	[[ -x "$loom_bin" ]] || return 0

	local updated=0
	local shell path

	# Check each shell's known completion file location
	for shell in zsh bash fish; do
		case "$shell" in
			zsh)  path="$HOME/.zfunc/_loom" ;;
			bash) path="$HOME/.local/share/bash-completion/completions/loom" ;;
			fish) path="$HOME/.config/fish/completions/loom.fish" ;;
		esac

		if [[ -f "$path" ]]; then
			"$loom_bin" completions "$shell" > "$path"
			((++updated))
		fi
	done

	if [[ $updated -gt 0 ]]; then
		ok "updated $updated shell completion file(s)"
	fi
}

if [[ "${LOOM_INSTALL_LIB_ONLY:-0}" != "1" ]]; then
	main "$@"
fi
