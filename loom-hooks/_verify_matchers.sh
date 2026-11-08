#!/usr/bin/env bash
# _verify_matchers.sh - test, build and lint runner matchers for
# subagent-verify-guard.sh. Sourced by that hook after block() is defined;
# never registered as a hook and never run directly.
#
# check_command <idx> judges the command word at TOKENS[idx] and calls
# block <reason> (which exits 2) for a project-wide run. The helpers read the
# caller's globals at call time: TOKENS and NTOK (the tokenized command), ROLE
# (main or subagent) and TEST_ASSIGNED (a TEST=/TESTOPTS= assignment precedes
# the command word). Matching is token-based and conservative: an unrecognised
# command is allowed.
if [[ "${_LOOM_VERIFY_MATCHERS_LOADED:-}" == 1 ]]; then return 0; fi
_LOOM_VERIFY_MATCHERS_LOADED=1

# is_redirect_op <token> - a BARE redirection operator (`>`, `>>`, `2>`, `<`)
# whose target is the NEXT token. `2>&1` names its own target, so it is not one.
is_redirect_op() {
	[[ -n "$1" ]] || return 1
	case "$1" in
	*[!0-9\<\>\&]*) return 1 ;; # not made purely of redirection characters
	*\&[0-9]*) return 1 ;;      # `2>&1` - the target is part of the token
	*[\<\>]*) return 0 ;;
	esac
	return 1
}

# tok_kind <token> - sets TOK_KIND to sep | redirop | redir | word.
#
# A redirection is NEVER a positional argument. `cargo test 2>&1 | tail -50` is
# a FULL-SUITE run, and piping a test run through tail is a common way to write
# it; reading `2>&1` as a filter would wave it through.
TOK_KIND=""
tok_kind() {
	case "$1" in
	";" | "&&" | "||" | "|" | "&" | "(" | ")" | "{" | "}") TOK_KIND=sep ;;
	*[\<\>]*)
		if is_redirect_op "$1"; then TOK_KIND=redirop; else TOK_KIND=redir; fi
		;;
	*) TOK_KIND=word ;;
	esac
	return 0
}

# scan_args <start-index> - classify a command's argument list. Echoes:
#   all    - an explicit whole-project flag (--all/--workspace/--all-targets/--doc)
#   scoped - a positional argument (filter/path), or a scoping flag, was given
#   bare   - only flags, or nothing: the project-wide default run
#
# Stops at the next separator, so `cargo test -p loom && cargo build` is judged
# one command at a time. Flags taking a separate value (--manifest-path <path>)
# consume it, so it is not read as a positional.
scan_args() {
	local i="$1" skip=0 tok
	while [[ $i -lt $NTOK ]]; do
		tok="${TOKENS[$i]}"
		i=$((i + 1))
		tok_kind "$tok"
		if [[ "$TOK_KIND" == sep ]]; then break; fi
		if [[ "$TOK_KIND" == redirop ]]; then skip=1; continue; fi
		if [[ "$TOK_KIND" == redir ]]; then continue; fi
		if [[ $skip -eq 1 ]]; then skip=0; continue; fi
		case "$tok" in
		--all | --workspace | --all-targets | --doc) echo "all"; return 0 ;;
		--test | --test=* | --bin | --bin=* | --example | --example=* | --bench | --bench=* | \
			-p | --package | --package=* | -k | -m | --test-name-pattern | --test-name-pattern=*) echo "scoped"; return 0 ;;
		--manifest-path | --target-dir | --features | -F | --target | --profile | \
			--color | --message-format | --config | -j | --jobs) skip=1 ;;
		--) ;;
		-*) ;;
		*) echo "scoped"; return 0 ;;
		esac
	done
	echo "bare"
	return 0
}

# first_positional <idx> <value-flags> - first bare arg before the next separator.
first_positional() {
	local i="$1" vflags=" $2 " tok
	while [[ $i -lt $NTOK ]]; do
		tok="${TOKENS[$i]}"
		i=$((i + 1))
		tok_kind "$tok"
		if [[ "$TOK_KIND" == sep ]]; then break; fi
		if [[ "$TOK_KIND" == redirop ]]; then i=$((i + 1)); continue; fi
		if [[ "$TOK_KIND" == redir ]]; then continue; fi
		case "$vflags" in *" $tok "*) i=$((i + 1)); continue ;; esac
		case "$tok" in -*) continue ;; esac
		printf '%s' "$tok"
		return 0
	done
	return 0
}

# block_on_word <idx> <words> <label> - block if one of <words> is a bare arg.
block_on_word() {
	local i="$1" words=" $2 " label="$3" tok
	while [[ $i -lt $NTOK ]]; do
		tok="${TOKENS[$i]}"
		i=$((i + 1))
		tok_kind "$tok"
		if [[ "$TOK_KIND" == sep ]]; then break; fi
		# `make docs > test.log`: a redirection target is a filename, not a goal
		if [[ "$TOK_KIND" == redirop ]]; then i=$((i + 1)); continue; fi
		if [[ "$TOK_KIND" == redir ]]; then continue; fi
		case "$tok" in -j | -C | -f | --directory) i=$((i + 1)); continue ;; esac
		case "$words" in *" $tok "*) block "$label $tok" ;; esac
	done
	return 0
}

# cargo: a subagent may compile (`build`, `check`, `test --no-run`); clippy and
# every `fmt` form (`fmt -- <files>` still formats the whole package) block.
# `cargo test` and `cargo nextest run` are blocked unless they name a scope.
check_cargo() {
	local i="$1" sub=""
	# Skip a toolchain override (`cargo +nightly test`) and leading flags
	while [[ $i -lt $NTOK ]]; do
		sub="${TOKENS[$i]}"
		case "$sub" in
		+* | -*) i=$((i + 1)) ;;
		*) break ;;
		esac
	done
	[[ $i -lt $NTOK ]] || return 0

	sub="${TOKENS[$i]}"
	case "$sub" in
	clippy | fmt) [[ "$ROLE" == main ]] || block "cargo $sub" ;;
	test) runner_args_have $((i + 1)) "--no-run" -- || block_unless_scoped $((i + 1)) "cargo test" ;;
	nextest)
		# `r` is nextest's alias of `run`; -E/--filterset/--partition select a subset
		case "${TOKENS[$((i + 1))]:-}" in
		run | r) runner_args_have $((i + 2)) "-E --filterset* --filter-expr* --partition*" ||
			block_unless_scoped $((i + 2)) "cargo nextest run" ;;
		esac
		;;
	esac
	return 0
}

# block_unless_scoped <idx> <label> - block unless the args name a scope.
block_unless_scoped() {
	local verdict
	verdict=$(scan_args "$1")
	if [[ "$verdict" != "scoped" ]]; then block "$2 ($verdict)"; fi
	return 0
}

# tsc: a non-watching `--noEmit` run is a compile check and passes. Otherwise
# -p/-b and a bare `tsc` typecheck everything; only a file list is scoped.
check_tsc() {
	[[ "$ROLE" != main ]] || return 0
	tsc_compiles_only "$1" && return 0
	block_on_word "$1" "-p --project -b --build -w --watch" "tsc whole-project"
	if [[ -z "$(first_positional "$1" "--outDir --outFile --target -t --module --lib --rootDir")" ]]; then
		block "tsc (no file argument)"
	fi
	return 0
}

# tsc_compiles_only <idx> - `--noEmit` is given and no watch flag or `false`
# (`--noEmit false` emits) is. tsc reads option names case-insensitively.
tsc_compiles_only() {
	runner_args_have "$1" "--[nN][oO][eE][mM][iI][tT]" &&
		! runner_args_have "$1" "false -[wW] --[wW][aA][tT][cC][hH]*"
}

check_eslint() {
	local pos
	[[ "$ROLE" != main ]] || return 0
	pos=$(first_positional "$1" "--ext --config -c --format -f --rulesdir --resolve-plugins-relative-to")
	case "$pos" in
	"" | "." | "./" | "*" | "**" | "**/*") block "eslint over the whole project" ;;
	esac
	return 0
}

# go test/build/vet ./... walks the whole module; ./pkg/... is scoped. A bare
# `go test` or `go test .` runs the current package's whole suite.
GO_FILTER_FLAGS="-run -run=* -skip -skip=* -fuzz -fuzz=* -list -list=* -test.run -test.run=* -test.skip -test.skip=*"
GO_VALUE_FLAGS="-run -skip -bench -benchtime -test.bench -test.benchtime -fuzz -list -count -timeout -parallel -cpu -tags -mod -modfile -vet -exec -coverprofile -coverpkg -covermode -coverdir -gcflags -ldflags"
check_go() {
	local sub="${TOKENS[$1]:-}"
	case "$sub" in
	test)
		# `go test -run X ./...` names a test: filtered, whatever the package
		if ! runner_args_have $(($1 + 1)) "$GO_FILTER_FLAGS"; then
			block_on_word $(($1 + 1)) "./... ... all" "go test"
			case "$(first_positional $(($1 + 1)) "$GO_VALUE_FLAGS")" in
			"" | . | ./) block "go test (current package, unfiltered)" ;;
			esac
		fi
		;;
	build | vet) [[ "$ROLE" == main ]] || block_on_word $(($1 + 1)) "./... ... all" "go $sub" ;;
	esac
	return 0
}

check_node_pm() {
	local i="$1" pm="$2" tok
	tok="${TOKENS[$i]:-}"
	case "$tok" in
	run | run-script | exec | dlx)
		i=$((i + 1))
		tok="${TOKENS[$i]:-}"
		;;
	esac
	case "$tok" in
	test) block_unless_scoped $((i + 1)) "$pm $tok" ;;
	# `build` compiles and passes; the guard never reads a script's body
	lint | typecheck) [[ "$ROLE" == main ]] || block_unless_scoped $((i + 1)) "$pm $tok" ;;
	vitest | jest | mocha | rspec | phpunit | ctest) check_command "$i" ;; # pnpm exec vitest, yarn jest
	esac
	return 0
}

check_make() {
	[[ "$ROLE" != main ]] || return 0
	block_on_word "$1" "test build check lint" "make"
	return 0
}

# Options taking a separate value, so the value is not read as a test path.
VALUE_FLAGS=" --config -c --reporter -r --project --root --configuration --framework -f --logger -l --settings -s --test-dir -C -j --parallel -pl -p --seed --exclude -e --durations "

# runner_args_have <idx> <patterns> [<stop>] - an argument before the next
# separator (or the word <stop>) matches one of the space-separated glob patterns.
runner_args_have() {
	local i="$1" tok pat
	local -a pats
	IFS=' ' read -ra pats <<<"$2"
	while [[ $i -lt $NTOK ]]; do
		tok="${TOKENS[$i]}"
		i=$((i + 1))
		[[ "$tok" != ";" && "$tok" != "&&" && "$tok" != "||" && "$tok" != "|" && "$tok" != "&" && "$tok" != "${3:-;}" ]] || break
		for pat in "${pats[@]}"; do
			# shellcheck disable=SC2053
			[[ "$tok" == $pat ]] && return 0
		done
	done
	return 1
}

# PHPUnit and Pest select a subset by filter, group or test suite.
PHP_FILTERS="--filter* --group* --exclude-group* --testsuite*"

# sbt: a `test` (or <project>/test) task with no testOnly/testQuick task runs the
# whole suite. Tasks may be quoted into one word (`sbt "testOnly a.B"`), so the
# patterns allow one quote character around them.
check_sbt() {
	runner_args_have "$1" "test */test ?test? ?*/test?" || return 0
	runner_args_have "$1" "*testOnly* *testQuick*" || block "sbt test (unfiltered)"
	return 0
}

# check_test_runner <idx> <label> <need> <skip> <scope> - block an unfiltered run.
#   need  - words one of which must appear (the test subcommand); empty = none
#   skip  - subcommand words that are not a test path
#   scope - glob patterns of the runner's own name/tag/filter options
# A positional argument (path or test name) or a scope option makes it filtered.
check_test_runner() {
	local i="$1" label="$2" need=" $3 " skip=" $4 " scope="$5" tok
	local seen=0 scoped=0
	[[ -n "${3// /}" ]] || seen=1
	runner_args_have "$i" "$scope" && scoped=1
	while [[ $i -lt $NTOK ]]; do
		tok="${TOKENS[$i]}"
		i=$((i + 1))
		tok_kind "$tok"
		if [[ "$TOK_KIND" == sep ]]; then break; fi
		if [[ "$TOK_KIND" == redirop ]]; then i=$((i + 1)); continue; fi
		if [[ "$TOK_KIND" == redir ]]; then continue; fi
		case "$need" in *" $tok "*) seen=1; continue ;; esac
		case "$VALUE_FLAGS" in *" $tok "*) i=$((i + 1)); continue ;; esac
		case "$tok" in -*) continue ;; esac
		case "$skip" in *" $tok "*) continue ;; esac
		scoped=1
	done
	if [[ $seen -eq 1 && $scoped -eq 0 ]]; then block "$label (unfiltered)"; fi
	return 0
}

# uv run [options] <command> - judge the command that follows.
check_uv() {
	local i="$1" tok
	[[ "${TOKENS[$i]:-}" == run ]] || return 0
	i=$((i + 1))
	while [[ $i -lt $NTOK ]]; do
		tok="${TOKENS[$i]}"
		case "$tok" in
		--with | --python | -p | --project | --directory | --group | --extra) i=$((i + 2)) ;;
		-*) i=$((i + 1)) ;;
		*) break ;;
		esac
	done
	if [[ $i -lt $NTOK ]]; then check_command "$i"; fi
	return 0
}

check_command() {
	local idx="$1"
	local cmd="${TOKENS[$idx]}"
	# `##*/` so an absolute path to the runner (/usr/bin/pytest) still matches
	local base="${cmd##*/}"
	case "$base" in
	cargo) check_cargo $((idx + 1)) ;;
	pytest) block_unless_scoped $((idx + 1)) "pytest" ;;
	python | python[0-9]*)
		# `python -m <module>` runs the module as the command, as recognize.rs reads it
		if [[ "${TOKENS[$((idx + 1))]:-}" == "-m" && $((idx + 2)) -lt $NTOK ]]; then
			check_command $((idx + 2))
		fi
		;;
	bundle)
		if [[ "${TOKENS[$((idx + 1))]:-}" == "exec" && $((idx + 2)) -lt $NTOK ]]; then
			check_command $((idx + 2))
		fi
		;;
	php) # `php vendor/bin/pest` and `php vendor/bin/phpunit`
		case "${TOKENS[$((idx + 1))]:-}" in
		pest | */pest | phpunit | */phpunit) check_command $((idx + 1)) ;;
		esac
		;;
	uv) check_uv $((idx + 1)) ;;
	unittest) check_test_runner $((idx + 1)) unittest "" "discover" "-k* -s* -p* -t* --start-directory* --pattern* --top-level-directory*" ;;
	sbt) check_sbt $((idx + 1)) ;;
	rake | rails)
		# TEST=/TESTOPTS= narrow a run even when they precede the command word
		if [[ "${TOKENS[$((idx + 1))]:-}" == "test" && $TEST_ASSIGNED -eq 0 ]]; then
			check_test_runner $((idx + 2)) "$base test" "" "" "-n* --name* TEST=* TESTOPTS=*"
		fi
		;;
	vitest) check_test_runner $((idx + 1)) vitest "" "run watch" "-t --testNamePattern --testNamePattern=*" ;;
	jest) check_test_runner $((idx + 1)) jest "" "" "-t --testNamePattern --testNamePattern=* --testPathPattern*" ;;
	mocha) check_test_runner $((idx + 1)) mocha "" "" "-g --grep --grep=*" ;;
	rspec) check_test_runner $((idx + 1)) rspec "" "" "-e --example --example=* --tag --tag=*" ;;
	phpunit) check_test_runner $((idx + 1)) phpunit "" "" "$PHP_FILTERS" ;;
	pest) check_test_runner $((idx + 1)) pest "" "" "$PHP_FILTERS" ;;
	dotnet) check_test_runner $((idx + 1)) "dotnet test" "test" "" "--filter --filter=*" ;;
	mvn | mvnw) check_test_runner $((idx + 1)) "mvn test" "test verify" "clean compile package install" "-Dtest=*" ;;
	gradle | gradlew) check_test_runner $((idx + 1)) "gradle test" "test" "clean build check" "--tests --tests=*" ;;
	ctest) check_test_runner $((idx + 1)) ctest "" "" "-R --tests-regex --tests-regex=*" ;;
	swift) check_test_runner $((idx + 1)) "swift test" "test" "" "--filter --filter=*" ;;
	# `dart run test` is `dart test`: `run` is no test path
	dart | flutter) check_test_runner $((idx + 1)) "$base test" "test" "run" "-n --name* -N --plain-name* -t --tags* -x --exclude-tags*" ;;
	mix) check_test_runner $((idx + 1)) "mix test" "test" "" "--only --only=*" ;;
	node) check_test_runner $((idx + 1)) "node --test" "--test" "" "--test-name-pattern --test-name-pattern=* --test-only" ;;
	tsc) check_tsc $((idx + 1)) ;;
	eslint) check_eslint $((idx + 1)) ;;
	go) check_go $((idx + 1)) ;;
	npm | bun | pnpm | yarn) check_node_pm $((idx + 1)) "$base" ;;
	make) check_make $((idx + 1)) ;;
	esac
	return 0
}
